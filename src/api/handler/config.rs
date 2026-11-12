use crate::api::{ok, AppState};
use crate::db::repo;
use crate::error::AppError;
use crate::utils::parse_cidr;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Serialize)]
struct GlobalVo {
    global_ip_whitelist: Vec<String>,
    max_global_conn: u64,
    max_user_conn: u64,
    idle_timeout_sec: u64,
}

pub async fn get(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let cfg = state.cache.global().await;
    let vo = GlobalVo {
        global_ip_whitelist: cfg
            .global_ip_whitelist
            .iter()
            .map(|n| n.to_string())
            .collect(),
        max_global_conn: cfg.max_global_conn,
        max_user_conn: cfg.max_user_conn,
        idle_timeout_sec: cfg.idle_timeout_sec,
    };
    Ok(ok(json!(vo)))
}

#[derive(Deserialize)]
pub struct GlobalReq {
    pub global_ip_whitelist: Option<Vec<String>>,
    pub max_global_conn: Option<u64>,
    pub max_user_conn: Option<u64>,
    pub idle_timeout_sec: Option<u64>,
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GlobalReq>,
) -> Result<Json<Value>, AppError> {
    let current = state.cache.global().await;

    let whitelist = match req.global_ip_whitelist {
        Some(list) => {
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
            out
        }
        None => current
            .global_ip_whitelist
            .iter()
            .map(|n| n.to_string())
            .collect(),
    };

    let max_global_conn = req.max_global_conn.unwrap_or(current.max_global_conn);
    let max_user_conn = req.max_user_conn.unwrap_or(current.max_user_conn);
    let idle_timeout_sec = req.idle_timeout_sec.unwrap_or(current.idle_timeout_sec).max(30);

    repo::upsert_global_config(
        &state.pool,
        "global_ip_whitelist",
        &serde_json::to_string(&whitelist).unwrap_or_else(|_| "[]".to_string()),
    )
    .await?;
    repo::upsert_global_config(&state.pool, "max_global_conn", &max_global_conn.to_string()).await?;
    repo::upsert_global_config(&state.pool, "max_user_conn", &max_user_conn.to_string()).await?;
    repo::upsert_global_config(&state.pool, "idle_timeout_sec", &idle_timeout_sec.to_string()).await?;

    state.cache.reload_global(&state.pool).await?;
    tracing::info!("更新全局配置");
    Ok(ok(json!({})))
}
