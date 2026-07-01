//! Assembling the HTTP server from configuration.

use std::sync::Arc;

use anyhow::Context as _;
use axum::Router;
use velox_http::{AppState, router};
use velox_storage::blob::BlobStore;
use velox_storage::meta::MetaStore;
use velox_upstream::UpstreamClient;

use crate::config::Config;

/// The default cached-index freshness window, in seconds.
const DEFAULT_TTL_SECS: i64 = 1800;
/// The route prefix of the built-in pypi.org mirror.
const ROOT_INDEX: &str = "root/pypi";

/// Build the velox router: open the metadata store and blob store under the data directory and wire
/// up the upstream mirror. Does not bind a socket, so it is testable in isolation.
///
/// # Errors
/// Returns an error if the data directory cannot be created, the store cannot be opened, or the
/// upstream URL is invalid.
pub fn build_router(config: &Config) -> anyhow::Result<Router> {
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create data directory {}", config.data_dir.display()))?;
    let meta = MetaStore::open(config.data_dir.join("velox.redb"))?;
    let blobs = BlobStore::new(config.data_dir.join("blobs"));
    let upstream = UpstreamClient::new(&config.upstream_url)?;
    let state = Arc::new(AppState::new(
        meta,
        blobs,
        upstream,
        ROOT_INDEX.to_owned(),
        DEFAULT_TTL_SECS,
    ));
    Ok(router(state))
}
