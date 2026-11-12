use axum::http::StatusCode as HttpStatus;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),
    #[error("内部错误: {0}")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status_and_code(&self) -> (HttpStatus, i32) {
        match self {
            AppError::BadRequest(_) => (HttpStatus::BAD_REQUEST, 400),
            AppError::Unauthorized(_) => (HttpStatus::UNAUTHORIZED, 401),
            AppError::NotFound(_) => (HttpStatus::NOT_FOUND, 404),
            AppError::Conflict(_) => (HttpStatus::CONFLICT, 409),
            AppError::Db(_) | AppError::Internal(_) => (HttpStatus::INTERNAL_SERVER_ERROR, 500),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if status == HttpStatus::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "api internal error");
        }
        let body = Json(json!({
            "code": code,
            "msg": self.to_string(),
            "data": null,
        }));
        (status, body).into_response()
    }
}
