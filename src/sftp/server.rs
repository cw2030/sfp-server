use crate::audit_log::AuditEvent;
use crate::cache::{AppCache, ConnGuard};
use crate::sftp::fs_handler::SftpSession;
use crate::sftp::ip_acl::ip_allowed;
use crate::utils::{new_session_id, verify_password};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

pub struct SftpOptions {
    pub bind: String,
    pub host_key_path: PathBuf,
}

pub async fn run(opts: SftpOptions, cache: Arc<AppCache>) -> anyhow::Result<()> {
    let host_key = load_or_create_host_key(&opts.host_key_path)?;

    let listener = TcpListener::bind(&opts.bind)
        .await
        .map_err(|e| anyhow::anyhow!("SFTP端口 {} 监听失败: {e}", opts.bind))?;
    tracing::info!(bind = %opts.bind, "SFTP服务已启动");

    loop {
        let (socket, peer) = listener.accept().await?;
        let cache = cache.clone();
        let ip = peer.ip();

        // 第一级：全局 IP 白名单，握手前直接断开
        let g = cache.global().await;
        if !ip_allowed(ip, &g.global_ip_whitelist) {
            tracing::warn!(client_ip = %ip, "全局IP白名单拒绝连接");
            drop(socket);
            continue;
        }

        // 全局最大并发连接
        let Some(guard) = cache.try_acquire_global(g.max_global_conn) else {
            tracing::warn!(client_ip = %ip, "超过全局最大并发连接数，拒绝连接");
            // 记录审计日志
            let mut ev = AuditEvent::new(
                "",
                None,
                ip.to_string().as_str(),
                "login",
                "fail",
            );
            ev.msg = Some("超过全局最大并发连接数，拒绝连接".to_string());
            let _ = cache.audit_tx.send(ev);
            drop(socket);
            continue;
        };

        // 每次连接读取最新全局配置，空闲超时热生效
        let config = Arc::new(russh::server::Config {
            keys: vec![host_key.clone()],
            auth_rejection_time: Duration::from_secs(2),
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: Some(Duration::from_secs(g.idle_timeout_sec.max(30))),
            ..Default::default()
        });
        drop(g);

        let handler = ClientHandler::new(peer, cache, guard);
        let config = config.clone();
        tokio::spawn(async move {
            match russh::server::run_stream(config, socket, handler).await {
                Ok(session) => {
                    let _ = session.await;
                }
                Err(e) => tracing::debug!(error = %e, "SSH握手/会话异常结束"),
            }
        });
    }
}

struct ClientHandler {
    peer_ip: IpAddr,
    peer_addr: String,
    session_id: String,
    cache: Arc<AppCache>,
    _global_guard: ConnGuard,
    user_guard: Option<ConnGuard>,
    username: Option<String>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    /// 同一连接内 login 失败审计只记一次：russh 会按 auth_none → password → publickey 顺序依次
    /// 调用回调，客户端也会重试多个认证方法，每个方法都会走 reject 路径；但它们本质上属
    /// 于同一次登录尝试，重复写审计没有意义。
    login_failed: bool,
}

impl ClientHandler {
    fn new(peer: SocketAddr, cache: Arc<AppCache>, guard: ConnGuard) -> Self {
        Self {
            peer_ip: peer.ip(),
            peer_addr: peer.ip().to_string(),
            session_id: new_session_id(),
            cache,
            _global_guard: guard,
            user_guard: None,
            username: None,
            channels: HashMap::new(),
            login_failed: false,
        }
    }

    fn login_fail(&mut self, username: &str, reason: &str) {
        if self.login_failed {
            return;
        }
        self.login_failed = true;
        let mut ev = AuditEvent::new(
            &self.session_id,
            Some(username.to_string()),
            &self.peer_addr,
            "login",
            "fail",
        );
        ev.msg = Some(reason.to_string());
        let _ = self.cache.audit_tx.send(ev);
    }

    async fn attach_user(&mut self, user: &str) -> bool {
        if self.username.is_some() {
            return true;
        }
        let max_user_conn = self.cache.global().await.max_user_conn;
        match self.cache.try_acquire_user(user, max_user_conn) {
            Some(guard) => {
                self.user_guard = Some(guard);
                self.username = Some(user.to_string());
                true
            }
            None => false,
        }
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        if let Some(username) = &self.username {
            let mut ev = AuditEvent::new(
                &self.session_id,
                Some(username.clone()),
                &self.peer_addr,
                "disconnect",
                "success",
            );
            ev.msg = Some("会话结束".to_string());
            let _ = self.cache.audit_tx.send(ev);
        }
    }
}

impl russh::server::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::reject())
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let Some(u) = self.cache.users.get(user).map(|r| r.clone()) else {
            self.login_fail(user, "用户不存在");
            return Ok(Auth::reject());
        };
        if !u.enabled {
            self.login_fail(user, "账号已禁用");
            return Ok(Auth::reject());
        }
        if !u.ip_allowed(self.peer_ip) {
            self.login_fail(user, "客户端IP不在用户白名单内");
            return Ok(Auth::reject());
        }
        let Some(hash) = u.password_hash.clone() else {
            self.login_fail(user, "该账号仅允许公钥登录");
            return Ok(Auth::reject());
        };

        let password = password.to_string();
        let ok = tokio::task::spawn_blocking(move || verify_password(&hash, &password))
            .await
            .unwrap_or(false);
        if !ok {
            self.login_fail(user, "密码错误");
            return Ok(Auth::reject());
        }
        if !self.attach_user(user).await {
            self.login_fail(user, "超过单用户最大并发会话数");
            return Ok(Auth::reject());
        }
        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        _public_key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        // 已停用公钥登录，统一拒绝
        // self.login_fail(user, "公钥登录已禁用，请使用密码登录");
        Ok(Auth::reject())
    }

    async fn auth_succeeded(&mut self, _session: &mut Session) -> Result<(), Self::Error> {
        if let Some(username) = &self.username {
            let ev = AuditEvent::new(
                &self.session_id,
                Some(username.clone()),
                &self.peer_addr,
                "login",
                "success",
            );
            let _ = self.cache.audit_tx.send(ev);
        }
        Ok(())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel_id)?;
            return Ok(());
        }
        let Some(username) = self.username.clone() else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };
        let Some(user) = self.cache.users.get(&username).map(|r| r.clone()) else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };
        let Some(channel) = self.channels.remove(&channel_id) else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };

        // 用户根目录不存在时自动创建
        if let Err(e) = tokio::fs::create_dir_all(&user.root_dir).await {
            tracing::warn!(username = %username, dir = %user.root_dir.display(), error = %e, "创建用户根目录失败");
        }

        session.channel_success(channel_id)?;
        let sftp = SftpSession::new(
            self.session_id.clone(),
            username,
            self.peer_addr.clone(),
            user.root_dir.clone(),
            user.upload_speed,
            user.download_speed,
            user.read_only(),
            self.cache.audit_tx.clone(),
        );
        russh_sftp::server::run(channel.into_stream(), sftp).await;
        Ok(())
    }
}

fn load_or_create_host_key(path: &Path) -> anyhow::Result<russh::keys::PrivateKey> {
    if path.exists() {
        let data = std::fs::read_to_string(path)?;
        return Ok(russh::keys::PrivateKey::from_openssh(&data)?);
    }

    let key = russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)?;
    let pem = key.to_openssh(russh::keys::ssh_key::LineEnding::LF)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, pem.as_str())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!(path = %path.display(), "已生成新的SFTP主机密钥");
    Ok(key)
}
