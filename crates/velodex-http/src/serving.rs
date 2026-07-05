//! The ecosystem serving interface.
//!
//! The router is ecosystem-neutral: it resolves a request to a configured index and hands the request
//! to that index's ecosystem driver. Each ecosystem implements [`EcosystemServing`] to serve its own
//! wire protocol (`PyPI`'s Simple API today; an `OCI` `/v2/` or npm registry later). The driver is held on
//! [`AppState`] and dispatched dynamically once per request, so adding an ecosystem is a new driver,
//! not a change to the router.

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::Multipart;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;

use crate::state::AppState;

/// How one ecosystem serves requests routed to an index of its kind.
#[async_trait]
pub trait EcosystemServing: Send + Sync {
    /// Serve a GET (index listing, project detail, file, archive inspection).
    async fn get(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response;

    /// Serve a POST (publish/upload).
    async fn post(&self, state: Arc<AppState>, path: String, headers: HeaderMap, multipart: Multipart) -> Response;

    /// Serve a PUT (yank, restore, promote).
    async fn put(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response;

    /// Serve a DELETE (remove or un-yank).
    async fn delete(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response;
}

/// The `PyPI` ecosystem serving driver.
#[derive(Debug, Clone, Copy, Default)]
pub struct PypiServing;

#[async_trait]
impl EcosystemServing for PypiServing {
    async fn get(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response {
        crate::handlers::pypi_dispatch_get(state, uri, headers).await
    }

    async fn post(&self, state: Arc<AppState>, path: String, headers: HeaderMap, multipart: Multipart) -> Response {
        crate::handlers::pypi_dispatch_post(state, path, headers, multipart).await
    }

    async fn put(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response {
        crate::handlers::pypi_dispatch_put(state, uri, headers).await
    }

    async fn delete(&self, state: Arc<AppState>, uri: Uri, headers: HeaderMap) -> Response {
        crate::handlers::pypi_dispatch_delete(state, uri, headers).await
    }
}
