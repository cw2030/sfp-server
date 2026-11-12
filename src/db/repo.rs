use crate::db::model::{AdminUserRow, AuditLogRow, GlobalConfigRow, SftpUserRow, WhitelistRow};
use chrono::NaiveDateTime;
use sqlx::mysql::MySqlPool;
use sqlx::MySql;
use std::collections::HashMap;

pub struct NewUser {
    pub username: String,
    pub password_hash: Option<String>,
    pub root_dir: String,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub enabled: bool,
    pub permission: String,
    pub ip_whitelist: Vec<String>,
}

pub async fn load_users(pool: &MySqlPool) -> sqlx::Result<Vec<(SftpUserRow, Vec<String>)>> {
    let users: Vec<SftpUserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, root_dir, upload_speed, download_speed, enabled, permission, created_at, updated_at FROM sftp_user ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let rows: Vec<WhitelistRow> =
        sqlx::query_as("SELECT user_id, cidr FROM sftp_user_ip_whitelist ORDER BY id")
            .fetch_all(pool)
            .await?;

    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    for r in rows {
        map.entry(r.user_id).or_default().push(r.cidr);
    }

    Ok(users
        .into_iter()
        .map(|u| {
            let list = map.remove(&u.id).unwrap_or_default();
            (u, list)
        })
        .collect())
}

pub async fn get_user(pool: &MySqlPool, username: &str) -> sqlx::Result<Option<(SftpUserRow, Vec<String>)>> {
    let user: Option<SftpUserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, root_dir, upload_speed, download_speed, enabled, permission, created_at, updated_at FROM sftp_user WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    let Some(user) = user else {
        return Ok(None);
    };
    let rows: Vec<WhitelistRow> =
        sqlx::query_as("SELECT user_id, cidr FROM sftp_user_ip_whitelist WHERE user_id = ? ORDER BY id")
            .bind(user.id)
            .fetch_all(pool)
            .await?;
    Ok(Some((user, rows.into_iter().map(|r| r.cidr).collect())))
}

pub async fn list_users(
    pool: &MySqlPool,
    keyword: Option<&str>,
    page: i64,
    page_size: i64,
) -> sqlx::Result<(i64, Vec<SftpUserRow>, HashMap<i64, Vec<String>>)> {
    let offset = (page - 1).max(0) * page_size;
    let like = keyword.map(|k| format!("%{k}%"));

    let total: i64 = match &like {
        Some(k) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sftp_user WHERE username LIKE ?")
                .bind(k)
                .fetch_one(pool)
                .await?
        }
        None => sqlx::query_scalar("SELECT COUNT(*) FROM sftp_user")
            .fetch_one(pool)
            .await?,
    };

    let base = "SELECT id, username, password_hash, root_dir, upload_speed, download_speed, enabled, permission, created_at, updated_at FROM sftp_user";
    let users: Vec<SftpUserRow> = match &like {
        Some(k) => {
            sqlx::query_as(&format!("{base} WHERE username LIKE ? ORDER BY id DESC LIMIT ? OFFSET ?"))
                .bind(k)
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query_as(&format!("{base} ORDER BY id DESC LIMIT ? OFFSET ?"))
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool)
                .await?
        }
    };

    let ids: Vec<i64> = users.iter().map(|u| u.id).collect();
    let mut whitelist: HashMap<i64, Vec<String>> = HashMap::new();
    if !ids.is_empty() {
        let mut qb = sqlx::QueryBuilder::<MySql>::new(
            "SELECT user_id, cidr FROM sftp_user_ip_whitelist WHERE user_id IN (",
        );
        let mut sep = qb.separated(", ");
        for id in &ids {
            sep.push_bind(id);
        }
        qb.push(") ORDER BY id");
        let rows: Vec<WhitelistRow> = qb.build_query_as().fetch_all(pool).await?;
        for r in rows {
            whitelist.entry(r.user_id).or_default().push(r.cidr);
        }
    }

    Ok((total, users, whitelist))
}

pub async fn create_user(pool: &MySqlPool, user: &NewUser) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "INSERT INTO sftp_user (username, password_hash, root_dir, upload_speed, download_speed, enabled, permission) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&user.username)
    .bind(&user.password_hash)
    .bind(&user.root_dir)
    .bind(user.upload_speed as i64)
    .bind(user.download_speed as i64)
    .bind(user.enabled as i8)
    .bind(&user.permission)
    .execute(&mut *tx)
    .await?;
    let user_id = res.last_insert_id() as i64;

    for cidr in &user.ip_whitelist {
        sqlx::query("INSERT INTO sftp_user_ip_whitelist (user_id, cidr) VALUES (?, ?)")
            .bind(user_id)
            .bind(cidr)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn update_user(pool: &MySqlPool, user: &NewUser) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "UPDATE sftp_user SET password_hash = ?, root_dir = ?, upload_speed = ?, download_speed = ?, enabled = ?, permission = ? WHERE username = ?",
    )
    .bind(&user.password_hash)
    .bind(&user.root_dir)
    .bind(user.upload_speed as i64)
    .bind(user.download_speed as i64)
    .bind(user.enabled as i8)
    .bind(&user.permission)
    .bind(&user.username)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query("DELETE FROM sftp_user_ip_whitelist WHERE user_id = (SELECT id FROM sftp_user WHERE username = ?)")
        .bind(&user.username)
        .execute(&mut *tx)
        .await?;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM sftp_user WHERE username = ?")
        .bind(&user.username)
        .fetch_one(&mut *tx)
        .await?;

    for cidr in &user.ip_whitelist {
        sqlx::query("INSERT INTO sftp_user_ip_whitelist (user_id, cidr) VALUES (?, ?)")
            .bind(user_id)
            .bind(cidr)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn delete_user(pool: &MySqlPool, username: &str) -> sqlx::Result<bool> {
    let res = sqlx::query("DELETE FROM sftp_user WHERE username = ?")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn load_global_config(pool: &MySqlPool) -> sqlx::Result<Vec<GlobalConfigRow>> {
    sqlx::query_as("SELECT cfg_key, cfg_value FROM sftp_global_config")
        .fetch_all(pool)
        .await
}

pub async fn upsert_global_config(pool: &MySqlPool, key: &str, value: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO sftp_global_config (cfg_key, cfg_value) VALUES (?, ?) ON DUPLICATE KEY UPDATE cfg_value = VALUES(cfg_value)",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct AuditFilter {
    pub username: Option<String>,
    pub op_type: Option<String>,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
}

pub async fn query_audit_logs(
    pool: &MySqlPool,
    filter: &AuditFilter,
    page: i64,
    page_size: i64,
) -> sqlx::Result<(i64, Vec<AuditLogRow>)> {
    fn apply<'a>(
        qb: &mut sqlx::QueryBuilder<'a, MySql>,
        filter: &'a AuditFilter,
    ) {
        qb.push(" WHERE 1=1");
        if let Some(username) = &filter.username {
            qb.push(" AND username = ").push_bind(username);
        }
        if let Some(op_type) = &filter.op_type {
            qb.push(" AND op_type = ").push_bind(op_type);
        }
        if let Some(start) = &filter.start_time {
            qb.push(" AND created_at >= ").push_bind(start);
        }
        if let Some(end) = &filter.end_time {
            qb.push(" AND created_at <= ").push_bind(end);
        }
    }

    let mut count_qb = sqlx::QueryBuilder::<MySql>::new("SELECT COUNT(*) FROM sftp_audit_log");
    apply(&mut count_qb, filter);
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut qb = sqlx::QueryBuilder::<MySql>::new(
        "SELECT id, session_id, username, client_ip, op_type, file_path, file_size, transferred_bytes, speed_bps, result, msg, created_at FROM sftp_audit_log",
    );
    apply(&mut qb, filter);
    qb.push(" ORDER BY id DESC LIMIT ").push_bind(page_size);
    qb.push(" OFFSET ").push_bind((page - 1).max(0) * page_size);
    let rows: Vec<AuditLogRow> = qb.build_query_as().fetch_all(pool).await?;
    Ok((total, rows))
}

pub async fn get_admin_by_name(pool: &MySqlPool, name: &str) -> sqlx::Result<Option<AdminUserRow>> {
    sqlx::query_as("SELECT id, admin_name, password_hash, enabled FROM admin_user WHERE admin_name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
}

pub async fn count_admins(pool: &MySqlPool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM admin_user")
        .fetch_one(pool)
        .await
}

pub async fn create_admin(pool: &MySqlPool, name: &str, password_hash: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO admin_user (admin_name, password_hash, enabled) VALUES (?, ?, 1)")
        .bind(name)
        .bind(password_hash)
        .execute(pool)
        .await?;
    Ok(())
}
