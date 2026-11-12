pub mod auth;
pub mod handler;
pub mod router;

use crate::cache::AppCache;
use crate::config::AppConfig;
use axum::Json;
use serde_json::{json, Value};
use sqlx::mysql::MySqlPool;
use std::sync::Arc;

pub struct AppState {
    pub pool: MySqlPool,
    pub cache: Arc<AppCache>,
    pub cfg: Arc<AppConfig>,
}

/// 统一成功响应
pub fn ok(data: Value) -> Json<Value> {
    Json(json!({"code": 0, "msg": "ok", "data": data}))
}
