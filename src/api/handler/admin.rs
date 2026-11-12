use crate::api::{auth, ok, AppState};
use crate::db::repo;
use crate::error::AppError;
use crate::utils::verify_password;
use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct LoginReq {
    pub admin_name: String,
    pub password: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginReq>,
) -> Result<Json<Value>, AppError> {
    let admin = repo::get_admin_by_name(&state.pool, req.admin_name.trim())
        .await?
        .ok_or_else(|| AppError::Unauthorized("账号或密码错误".to_string()))?;

    if admin.enabled == 0 {
        return Err(AppError::Unauthorized("账号已被禁用".to_string()));
    }

    let hash = admin.password_hash.clone();
    let password = req.password.clone();
    let valid = tokio::task::spawn_blocking(move || verify_password(&hash, &password))
        .await
        .unwrap_or(false);
    if !valid {
        return Err(AppError::Unauthorized("账号或密码错误".to_string()));
    }

    let token = auth::issue_token(&state, admin.id, &admin.admin_name)?;
    tracing::info!(admin = %admin.admin_name, "后台管理员登录成功");
    Ok(ok(json!({
        "token": token,
        "admin_name": admin.admin_name,
        "expires_in": state.cfg.jwt_expire_hours * 3600,
    })))
}
