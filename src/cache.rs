use crate::audit_log::AuditEvent;
use crate::db::{self, model::SftpUserRow};
use crate::utils::parse_cidr;
use dashmap::DashMap;
use ipnet::IpNet;
use sqlx::mysql::MySqlPool;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct SftpUser {
    pub username: String,
    pub password_hash: Option<String>,
    pub root_dir: PathBuf,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub ip_whitelist: Vec<IpNet>,
    pub enabled: bool,
    /// 读写权限："rw" 可读可写，"r" 只读。任何非 "r" 的值都视为可写。
    pub permission: String,
}

impl SftpUser {
    pub fn from_row(row: SftpUserRow, cidrs: Vec<String>) -> Self {
        let mut ip_whitelist = Vec::new();
        for cidr in cidrs {
            match parse_cidr(&cidr) {
                Ok(net) => ip_whitelist.push(net),
                Err(e) => tracing::warn!(username = %row.username, error = %e, "IP白名单解析失败，忽略"),
            }
        }

        let permission = if row.permission == "r" {
            "r".to_string()
        } else {
            "rw".to_string()
        };

        Self {
            username: row.username,
            password_hash: row.password_hash,
            root_dir: PathBuf::from(row.root_dir),
            upload_speed: row.upload_speed.max(0) as u64,
            download_speed: row.download_speed.max(0) as u64,
            ip_whitelist,
            enabled: row.enabled != 0,
            permission,
        }
    }

    pub fn ip_allowed(&self, ip: IpAddr) -> bool {
        self.ip_whitelist.is_empty() || self.ip_whitelist.iter().any(|n| n.contains(&ip))
    }

    /// 只读用户禁止所有写操作
    pub fn read_only(&self) -> bool {
        self.permission == "r"
    }
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub global_ip_whitelist: Vec<IpNet>,
    pub max_global_conn: u64,
    pub max_user_conn: u64,
    pub idle_timeout_sec: u64,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            global_ip_whitelist: Vec::new(),
            max_global_conn: 0,
            max_user_conn: 0,
            idle_timeout_sec: 600,
        }
    }
}

impl GlobalConfig {
    pub fn ip_allowed(&self, ip: IpAddr) -> bool {
        self.global_ip_whitelist.is_empty()
            || self.global_ip_whitelist.iter().any(|n| n.contains(&ip))
    }
}

/// 连接计数器守卫，Drop 时自动减一
pub struct ConnGuard {
    counter: Arc<AtomicU64>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct AppCache {
    pub users: DashMap<String, SftpUser>,
    global: RwLock<GlobalConfig>,
    global_conn: Arc<AtomicU64>,
    user_conn: DashMap<String, Arc<AtomicU64>>,
    pub audit_tx: UnboundedSender<AuditEvent>,
}

impl AppCache {
    pub fn new(audit_tx: UnboundedSender<AuditEvent>) -> Self {
        Self {
            users: DashMap::new(),
            global: RwLock::new(GlobalConfig::default()),
            global_conn: Arc::new(AtomicU64::new(0)),
            user_conn: DashMap::new(),
            audit_tx,
        }
    }

    pub async fn global(&self) -> GlobalConfig {
        self.global.read().await.clone()
    }

    pub async fn set_global(&self, cfg: GlobalConfig) {
        *self.global.write().await = cfg;
    }

    /// 全量从 MySQL 刷新缓存（DB 异常只告警，不影响 SFTP 服务）
    pub async fn reload(&self, pool: &MySqlPool) -> anyhow::Result<()> {
        let users = db::repo::load_users(pool).await?;
        let mut seen = std::collections::HashSet::new();
        for (row, cidrs) in users {
            seen.insert(row.username.clone());
            self.users
                .insert(row.username.clone(), SftpUser::from_row(row, cidrs));
        }
        // 删除已不存在的用户（不影响已建立的会话）
        self.users.retain(|name, _| seen.contains(name));

        self.reload_global(pool).await?;
        Ok(())
    }

    pub async fn reload_global(&self, pool: &MySqlPool) -> anyhow::Result<()> {
        let rows = db::repo::load_global_config(pool).await?;
        let mut cfg = GlobalConfig::default();
        for row in rows {
            let val = row.cfg_value.unwrap_or_default();
            match row.cfg_key.as_str() {
                "global_ip_whitelist" => {
                    let list: Vec<String> = serde_json::from_str(&val).unwrap_or_default();
                    cfg.global_ip_whitelist = list
                        .iter()
                        .filter_map(|s| match parse_cidr(s) {
                            Ok(n) => Some(n),
                            Err(e) => {
                                tracing::warn!(error = %e, "全局IP白名单解析失败，忽略");
                                None
                            }
                        })
                        .collect();
                }
                "max_global_conn" => cfg.max_global_conn = val.parse().unwrap_or(0),
                "max_user_conn" => cfg.max_user_conn = val.parse().unwrap_or(0),
                "idle_timeout_sec" => {
                    cfg.idle_timeout_sec = val.parse().unwrap_or(600);
                }
                _ => {}
            }
        }
        *self.global.write().await = cfg;
        Ok(())
    }

    pub fn apply_user(&self, user: SftpUser) {
        self.users.insert(user.username.clone(), user);
    }

    pub fn remove_user(&self, username: &str) {
        self.users.remove(username);
        self.user_conn.remove(username);
    }

    pub fn try_acquire_global(&self, max: u64) -> Option<ConnGuard> {
        acquire(&self.global_conn, max)
    }

    pub fn try_acquire_user(&self, username: &str, max: u64) -> Option<ConnGuard> {
        let counter = self
            .user_conn
            .entry(username.to_string())
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();
        acquire(&counter, max)
    }

    pub fn emit(&self, event: AuditEvent) {
        let _ = self.audit_tx.send(event);
    }
}

fn acquire(counter: &Arc<AtomicU64>, max: u64) -> Option<ConnGuard> {
    loop {
        let cur = counter.load(Ordering::SeqCst);
        if max > 0 && cur >= max {
            return None;
        }
        if counter
            .compare_exchange(cur, cur + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Some(ConnGuard {
                counter: counter.clone(),
            });
        }
    }
}
