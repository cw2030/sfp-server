use crate::api::{ok, AppState};
use crate::db::repo::{self, AuditFilter};
use crate::error::AppError;
use crate::utils::format_dt;
use axum::extract::{Query, State};
use axum::Json;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AuditQuery {
    pub username: Option<String>,
    pub op_type: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Serialize)]
struct AuditVo {
    id: i64,
    session_id: String,
    username: Option<String>,
    client_ip: String,
    op_type: String,
    file_path: Option<String>,
    file_size: u64,
    transferred_bytes: u64,
    speed_bps: u64,
    result: String,
    msg: Option<String>,
    created_at: Option<String>,
}

fn parse_time(s: Option<&str>, end_of_day: bool) -> Result<Option<NaiveDateTime>, AppError> {
    let Some(s) = s.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if s.len() == 10 {
        let base = format!("{s} {}", if end_of_day { "23:59:59" } else { "00:00:00" });
        return NaiveDateTime::parse_from_str(&base, "%Y-%m-%d %H:%M:%S")
            .map(Some)
            .map_err(|_| AppError::BadRequest(format!("时间格式错误: {s}")));
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(Some)
        .map_err(|_| AppError::BadRequest(format!("时间格式错误: {s}")))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(20).clamp(1, 200);
    let filter = AuditFilter {
        username: q
            .username
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        op_type: q
            .op_type
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        start_time: parse_time(q.start_time.as_deref(), false)?,
        end_time: parse_time(q.end_time.as_deref(), true)?,
    };

    let (total, rows) = repo::query_audit_logs(&state.pool, &filter, page, page_size).await?;
    let list: Vec<AuditVo> = rows
        .into_iter()
        .map(|r| AuditVo {
            id: r.id,
            session_id: r.session_id,
            username: r.username,
            client_ip: r.client_ip,
            op_type: r.op_type,
            file_path: r.file_path,
            file_size: r.file_size.max(0) as u64,
            transferred_bytes: r.transferred_bytes.max(0) as u64,
            speed_bps: r.speed_bps.max(0) as u64,
            result: r.result,
            msg: r.msg,
            created_at: format_dt(r.created_at),
        })
        .collect();

    Ok(ok(json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "list": list,
    })))
}
