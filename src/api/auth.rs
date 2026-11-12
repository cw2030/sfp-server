use crate::api::AppState;
use crate::error::AppError;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i64,
    pub name: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue_token(state: &AppState, admin_id: i64, name: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: admin_id,
        name: name.to_string(),
        iat: now,
        exp: now + state.cfg.jwt_expire_hours * 3600,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.cfg.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("JWT签发失败: {e}")))
}

pub fn verify_token(state: &AppState, token: &str) -> Result<Claims, AppError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.cfg.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized("登录状态无效或已过期".to_string()))?;
    Ok(data.claims)
}

/// Bearer Token 鉴权中间件
pub async fn require_admin(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::Unauthorized("缺少 Authorization: Bearer token".to_string()))?;

    let claims = verify_token(&state, token)?;
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}
