mod api;
mod audit_log;
mod cache;
mod config;
mod db;
mod error;
mod sftp;
mod utils;

use crate::api::AppState;
use crate::cache::AppCache;
use crate::config::AppConfig;
use crate::sftp::SftpOptions;
use sqlx::mysql::MySqlPoolOptions;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cfg = AppConfig::load()?;
    let _log_guard = init_tracing(&cfg)?;

    tracing::info!("启动 SFTP 管理服务");

    let tz = cfg.db_timezone.clone();
    let pool = MySqlPoolOptions::new()
        .max_connections(10)
        .after_connect(move |conn, _meta| {
            let tz = tz.clone();
            Box::pin(async move {
                let sql = format!("SET time_zone = '{}'", tz);
                sqlx::query(&sql).execute(&mut *conn).await?;
                Ok(())
            })
        })
        .connect(&cfg.database_url)
        .await
        .map_err(|e| anyhow::anyhow!("连接MySQL失败: {e}"))?;
    tracing::info!(db_timezone = %cfg.db_timezone, "MySQL会话时区已设置");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("数据库迁移失败: {e}"))?;
    tracing::info!("数据库迁移完成");

    ensure_default_admin(&pool, &cfg).await?;

    let (audit_tx, _audit_handle) = audit_log::spawn(pool.clone());
    let cache = Arc::new(AppCache::new(audit_tx));
    cache
        .reload(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("加载缓存失败: {e}"))?;
    tracing::info!(users = cache.users.len(), "用户配置缓存已加载");

    // 后台任务：每30秒全量同步 MySQL -> 内存缓存；DB异常只告警
    {
        let cache = cache.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(30));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = cache.reload(&pool).await {
                    tracing::warn!(error = %e, "缓存同步失败，继续使用旧缓存");
                }
            }
        });
    }

    // SFTP 服务
    {
        let cache = cache.clone();
        let opts = SftpOptions {
            bind: cfg.sftp_bind.clone(),
            host_key_path: PathBuf::from(&cfg.host_key_path),
        };
        tokio::spawn(async move {
            if let Err(e) = sftp::run(opts, cache).await {
                tracing::error!(error = %e, "SFTP服务异常退出");
                std::process::exit(1);
            }
        });
    }

    // HTTP 管理 API
    let state = Arc::new(AppState {
        pool: pool.clone(),
        cache: cache.clone(),
        cfg: Arc::new(cfg.clone()),
    });
    let app = api::router::build(state);
    let listener = tokio::net::TcpListener::bind(&cfg.http_bind)
        .await
        .map_err(|e| anyhow::anyhow!("HTTP端口 {} 监听失败: {e}", cfg.http_bind))?;
    tracing::info!(bind = %cfg.http_bind, "HTTP管理API已启动");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn ensure_default_admin(pool: &sqlx::MySqlPool, cfg: &AppConfig) -> anyhow::Result<()> {
    let count = db::repo::count_admins(pool).await?;
    if count > 0 {
        return Ok(());
    }
    let password = cfg.admin_password.clone();
    let hash = tokio::task::spawn_blocking(move || crate::utils::hash_password(&password)).await??;
    db::repo::create_admin(pool, &cfg.admin_user, &hash).await?;
    tracing::warn!(
        admin = %cfg.admin_user,
        "admin_user 表为空，已创建默认管理员账号，请尽快修改密码"
    );
    Ok(())
}

fn init_tracing(cfg: &AppConfig) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    std::fs::create_dir_all(&cfg.log_dir)?;
    let file_appender = tracing_appender::rolling::daily(&cfg.log_dir, "sftp-server.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,russh=warn,russh_sftp=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .compact(),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    Ok(guard)
}
