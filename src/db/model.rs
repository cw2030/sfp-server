use chrono::NaiveDateTime;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SftpUserRow {
    pub id: i64,
    pub username: String,
    pub password_hash: Option<String>,
    pub root_dir: String,
    pub upload_speed: i64,
    pub download_speed: i64,
    pub enabled: i8,
    pub permission: String,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WhitelistRow {
    pub user_id: i64,
    pub cidr: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GlobalConfigRow {
    pub cfg_key: String,
    pub cfg_value: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: i64,
    pub session_id: String,
    pub username: Option<String>,
    pub client_ip: String,
    pub op_type: String,
    pub file_path: Option<String>,
    pub file_size: i64,
    pub transferred_bytes: i64,
    pub speed_bps: i64,
    pub result: String,
    pub msg: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminUserRow {
    pub id: i64,
    pub admin_name: String,
    pub password_hash: String,
    pub enabled: i8,
}
