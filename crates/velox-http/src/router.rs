//! The axum router.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

/// Build the velox HTTP router.
///
/// All index traffic lands on a catch-all path that the handlers resolve to a configured index by
/// longest route prefix, so routes are data, not hardcoded. Every request is traced (method, path,
/// status) at debug level, which is how the `.metadata` fast path can be observed in the logs.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/+status", get(handlers::status))
        .route("/metrics", get(handlers::metrics))
        .route(
            "/{*path}",
            get(handlers::dispatch_get)
                .post(handlers::dispatch_post)
                .put(handlers::dispatch_put)
                .delete(handlers::dispatch_delete),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
