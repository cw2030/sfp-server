use crate::api::auth::require_admin;
use crate::api::handler::{admin, audit, config, user};
use crate::api::AppState;
use axum::routing::{delete, get, post, put};
use axum::{middleware, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub fn build(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/sftp/user/list", get(user::list))
        .route("/sftp/user/add", post(user::add))
        .route("/sftp/user/update", put(user::update))
        .route("/sftp/user/del", delete(user::del))
        .route("/config/global", get(config::get).put(config::update))
        .route("/audit/log", get(audit::list))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_admin,
        ));

    let api = Router::new()
        .route("/admin/login", post(admin::login))
        .merge(protected);

    Router::new()
        .nest("/api", api)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
