use crate::api::{ok, AppState};
use crate::cache::SftpUser;
use crate::db::repo::{self, NewUser};
use crate::error::AppError;
use crate::utils::{format_dt, hash_password, parse_cidr};
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListQuery {
    pub keyword: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Serialize)]
struct UserVo {
    id: i64,
    username: String,
    root_dir: String,
    upload_speed: u64,
    download_speed: u64,
    enabled: bool,
    permission: String,
    ip_whitelist: Vec<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(20).clamp(1, 200);
    let keyword = q.keyword.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

    let (total, rows, whitelist) = repo::list_users(&state.pool, keyword, page, page_size).await?;

    let list: Vec<UserVo> = rows
        .into_iter()
        .map(|u| {
            let cidrs = whitelist.get(&u.id).cloned().unwrap_or_default();
            UserVo {
                id: u.id,
                username: u.username,
                root_dir: u.root_dir,
                upload_speed: u.upload_speed.max(0) as u64,
                download_speed: u.download_speed.max(0) as u64,
                enabled: u.enabled != 0,
                permission: u.permission,
                ip_whitelist: cidrs,
                created_at: format_dt(u.created_at),
                updated_at: format_dt(u.updated_at),
            }
        })
        .collect();

    Ok(ok(json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "list": list,
    })))
}

#[derive(Deserialize)]
pub struct UpsertReq {
    pub username: String,
    pub password: Option<String>,
    pub root_dir: String,
    pub upload_speed: Option<u64>,
    pub download_speed: Option<u64>,
    pub enabled: Option<bool>,
    /// 可选值：rw（默认，可读可写）、r（只读）
    pub permission: Option<String>,
    pub ip_whitelist: Option<Vec<String>>,
}

/// 校验并规范化权限值：只接受 "rw" / "r"，其他一律回退到默认 "rw"
fn normalize_permission(raw: &Option<String>) -> String {
    match raw.as_deref().map(|s| s.trim()) {
        Some("r") => "r".to_string(),
        _ => "rw".to_string(),
    }
}

fn validate_username(username: &str) -> Result<(), AppError> {
    let valid = !username.is_empty()
        && username.len() <= 64
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !valid {
        return Err(AppError::BadRequest(
            "用户名只能包含字母、数字、下划线、中划线和点，长度1-64".to_string(),
        ));
    }
    Ok(())
}

fn validate_root_dir(dir: &str) -> Result<(), AppError> {
    if !Path::new(dir).is_absolute() {
        return Err(AppError::BadRequest("根目录必须是绝对路径".to_string()));
    }
    Ok(())
}

fn normalize_whitelist(list: &[String]) -> Result<Vec<String>, AppError> {
    let mut out = Vec::new();
    for item in list {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        parse_cidr(item).map_err(AppError::BadRequest)?;
        if !out.iter().any(|v| v == item) {
            out.push(item.to_string());
        }
    }
    Ok(out)
}

async fn refresh_cache(state: &Arc<AppState>, username: &str) -> Result<(), AppError> {
    match repo::get_user(&state.pool, username).await? {
        Some((row, cidrs)) => {
            state.cache.apply_user(SftpUser::from_row(row, cidrs));
            Ok(())
        }
        None => {
            state.cache.remove_user(username);
            Ok(())
        }
    }
}

pub async fn add(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpsertReq>,
) -> Result<Json<Value>, AppError> {
    let username = req.username.trim().to_string();
    validate_username(&username)?;
    validate_root_dir(req.root_dir.trim())?;

    if repo::get_user(&state.pool, &username).await?.is_some() {
        return Err(AppError::Conflict("用户名已存在".to_string()));
    }

    let password = req.password.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(password) = password else {
        return Err(AppError::BadRequest("请填写密码".to_string()));
    };

    let password_hash = {
        let p = password.to_string();
        Some(
            tokio::task::spawn_blocking(move || hash_password(&p))
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("哈希任务失败: {e}")))??,
        )
    };

    let new_user = NewUser {
        username: username.clone(),
        password_hash,
        root_dir: req.root_dir.trim().to_string(),
        upload_speed: req.upload_speed.unwrap_or(0),
        download_speed: req.download_speed.unwrap_or(0),
        enabled: req.enabled.unwrap_or(true),
        permission: normalize_permission(&req.permission),
        ip_whitelist: normalize_whitelist(req.ip_whitelist.as_deref().unwrap_or(&[]))?,
    };
    repo::create_user(&state.pool, &new_user).await?;
    refresh_cache(&state, &username).await?;
    tracing::info!(username = %username, "新增SFTP用户");
    Ok(ok(json!({})))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpsertReq>,
) -> Result<Json<Value>, AppError> {
    let username = req.username.trim().to_string();
    validate_username(&username)?;
    validate_root_dir(req.root_dir.trim())?;

    let Some((existing, _)) = repo::get_user(&state.pool, &username).await? else {
        return Err(AppError::NotFound("用户不存在".to_string()));
    };

    let password = req.password.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let password_hash = match password {
        Some(p) => {
            let p = p.to_string();
            Some(
                tokio::task::spawn_blocking(move || hash_password(&p))
                    .await
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("哈希任务失败: {e}")))??,
            )
        }
        None => existing.password_hash.clone(),
    };

    let new_user = NewUser {
        username: username.clone(),
        password_hash,
        root_dir: req.root_dir.trim().to_string(),
        upload_speed: req.upload_speed.unwrap_or(0),
        download_speed: req.download_speed.unwrap_or(0),
        enabled: req.enabled.unwrap_or(true),
        permission: normalize_permission(&req.permission),
        ip_whitelist: normalize_whitelist(req.ip_whitelist.as_deref().unwrap_or(&[]))?,
    };
    if !repo::update_user(&state.pool, &new_user).await? {
        return Err(AppError::NotFound("用户不存在".to_string()));
    }
    refresh_cache(&state, &username).await?;
    tracing::info!(username = %username, "更新SFTP用户");
    Ok(ok(json!({})))
}

#[derive(Deserialize)]
pub struct DelQuery {
    pub username: String,
}

pub async fn del(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DelQuery>,
) -> Result<Json<Value>, AppError> {
    let username = q.username.trim();
    if username.is_empty() {
        return Err(AppError::BadRequest("缺少 username".to_string()));
    }
    if !repo::delete_user(&state.pool, username).await? {
        return Err(AppError::NotFound("用户不存在".to_string()));
    }
    state.cache.remove_user(username);
    tracing::info!(username = %username, "删除SFTP用户");
    Ok(ok(json!({})))
}
