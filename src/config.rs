use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tracing::info;
use std::env;
use std::path::Path;

/// 应用配置。
///
/// 加载优先级（后者覆盖前者）：app.yaml 默认值 -> 环境变量。
/// 其中 `database_url` 与 `jwt_secret` 必须由 YAML 或环境变量提供。
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    #[serde(default = "default_db_timezone")]
    pub db_timezone: String,
    #[serde(default = "default_sftp_bind")]
    pub sftp_bind: String,
    #[serde(default = "default_http_bind")]
    pub http_bind: String,
    pub jwt_secret: String,
    #[serde(default = "default_jwt_expire_hours")]
    pub jwt_expire_hours: i64,
    #[serde(default = "default_host_key_path")]
    pub host_key_path: String,
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    #[serde(default = "default_admin_user")]
    pub admin_user: String,
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
}

/// MySQL 会话时区，直接写入 `SET time_zone = '<db_timezone>'`。
/// 示例：`+08:00`、`-05:00`、`UTC`。
/// 注意：MySQL `DATETIME DEFAULT CURRENT_TIMESTAMP` 按此会话时区生成时间，
/// 所以此字段必须与你期望的存储时区一致。
fn default_db_timezone() -> String {
    "+08:00".to_string()
}

fn default_sftp_bind() -> String {
    "0.0.0.0:2222".to_string()
}
fn default_http_bind() -> String {
    "0.0.0.0:8080".to_string()
}
fn default_jwt_expire_hours() -> i64 {
    12
}
fn default_host_key_path() -> String {
    "host_key".to_string()
}
fn default_log_dir() -> String {
    "logs".to_string()
}
fn default_admin_user() -> String {
    "admin".to_string()
}
fn default_admin_password() -> String {
    "Admin@123456".to_string()
}

/// 用于反序列化 app.yaml：所有字段均为可选，由默认值/环境变量补齐。
#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    database_url: Option<String>,
    db_timezone: Option<String>,
    sftp_bind: Option<String>,
    http_bind: Option<String>,
    jwt_secret: Option<String>,
    jwt_expire_hours: Option<i64>,
    host_key_path: Option<String>,
    log_dir: Option<String>,
    admin_user: Option<String>,
    admin_password: Option<String>,
}

fn env_override<T>(val: &mut Option<T>, key: &str)
where
    T: std::str::FromStr,
{
    if let Ok(v) = env::var(key) {
        if let Ok(parsed) = v.parse::<T>() {
            *val = Some(parsed);
        } else {
            tracing::warn!(env = key, value = %v, "环境变量解析失败，忽略");
        }
    }
}

fn env_override_str(val: &mut Option<String>, key: &str) {
    if let Ok(v) = env::var(key) {
        *val = Some(v);
    }
}

impl AppConfig {
    /// 加载配置：优先读 app.yaml（路径可由 `CONFIG_FILE` 环境变量覆盖），
    /// 再用环境变量覆盖对应字段，最后填充默认值并校验必填项。
    pub fn load() -> Result<Self> {
        let path = env::var("CONFIG_FILE").unwrap_or_else(|_| "app.yaml".to_string());
        let mut raw: RawConfig = if Path::new(&path).exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("读取配置文件 {} 失败", path))?;
            serde_yaml::from_str(&content)
                .with_context(|| format!("解析配置文件 {} 失败", path))?
        } else {
            tracing::info!(path = %path, "未找到配置文件，使用环境变量与默认值");
            RawConfig::default()
        };
        tracing::info!("{:?}", raw);

        // 环境变量覆盖（优先级最高）
        env_override_str(&mut raw.database_url, "DATABASE_URL");
        env_override_str(&mut raw.db_timezone, "DB_TIMEZONE");
        env_override_str(&mut raw.sftp_bind, "SFTP_BIND");
        env_override_str(&mut raw.http_bind, "HTTP_BIND");
        env_override_str(&mut raw.jwt_secret, "JWT_SECRET");
        env_override(&mut raw.jwt_expire_hours, "JWT_EXPIRE_HOURS");
        env_override_str(&mut raw.host_key_path, "HOST_KEY_PATH");
        env_override_str(&mut raw.log_dir, "LOG_DIR");
        env_override_str(&mut raw.admin_user, "ADMIN_USER");
        env_override_str(&mut raw.admin_password, "ADMIN_PASSWORD");

        let database_url = raw
            .database_url
            .ok_or_else(|| anyhow!("database_url 未配置：请在 app.yaml 或环境变量 DATABASE_URL 中设置"))?;
        let jwt_secret = raw
            .jwt_secret
            .ok_or_else(|| anyhow!("jwt_secret 未配置：请在 app.yaml 或环境变量 JWT_SECRET 中设置"))?;

        Ok(Self {
            database_url,
            db_timezone: raw.db_timezone.unwrap_or_else(default_db_timezone),
            sftp_bind: raw.sftp_bind.unwrap_or_else(default_sftp_bind),
            http_bind: raw.http_bind.unwrap_or_else(default_http_bind),
            jwt_secret,
            jwt_expire_hours: raw.jwt_expire_hours.unwrap_or_else(default_jwt_expire_hours),
            host_key_path: raw.host_key_path.unwrap_or_else(default_host_key_path),
            log_dir: raw.log_dir.unwrap_or_else(default_log_dir),
            admin_user: raw.admin_user.unwrap_or_else(default_admin_user),
            admin_password: raw.admin_password.unwrap_or_else(default_admin_password),
        })
    }
}
