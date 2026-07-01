//! The read-through cache: serve a project's simple page and file bytes, fetching and caching from
//! upstream on a miss.

use bytes::Bytes;
use url::Url;
use velox_core::pypi::{
    CoreMetadata, File, Meta, ParsedDetail, ProjectDetail, ProjectList, ProjectListEntry, parse_detail,
    parse_detail_html, to_json,
};
use velox_storage::blob::Digest;
use velox_storage::meta::CachedIndex;
use velox_upstream::SimpleResponse;

use crate::state::AppState;

/// An error while producing a cached response.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error(transparent)]
    Meta(#[from] velox_storage::meta::MetaError),
    #[error(transparent)]
    Blob(#[from] velox_storage::blob::BlobError),
    #[error(transparent)]
    Upstream(#[from] velox_upstream::UpstreamError),
    #[error(transparent)]
    Parse(#[from] serde_json::Error),
    #[error("upstream unreachable and nothing cached")]
    Unavailable,
    #[error("no known source for this file")]
    FileNotFound,
}

/// Resolve a project's detail, serving from cache when fresh and revalidating or fetching upstream
/// otherwise. Returns `None` when the project does not exist upstream.
///
/// # Errors
/// Returns [`CacheError`] if a store, parse, or (with no cached fallback) upstream error occurs.
pub async fn project_detail(state: &AppState, project: &str) -> Result<Option<ProjectDetail>, CacheError> {
    let key = format!("{}/{project}", state.index);
    let now = (state.clock)();
    let cached = state.meta.get_index(&key)?;

    if let Some(record) = &cached
        && now - record.fetched_at_unix < state.ttl_secs
    {
        return Ok(Some(decode_detail(&record.body)?));
    }

    let etag = cached.as_ref().and_then(|record| record.etag.clone());
    match state.upstream.fetch_project(project, etag.as_deref()).await {
        Ok(response) if response.status == 200 => Ok(Some(store_fresh(state, &key, project, response, now)?)),
        Ok(response) if response.status == 304 => {
            let mut record = cached.ok_or(CacheError::Unavailable)?;
            record.fetched_at_unix = now;
            state.meta.put_index(&key, &record)?;
            Ok(Some(decode_detail(&record.body)?))
        }
        Ok(response) if response.status == 404 => Ok(None),
        Ok(_) => serve_stale(cached.as_ref()),
        Err(err) => match cached.as_ref() {
            Some(record) => Ok(Some(decode_detail(&record.body)?)),
            None => Err(CacheError::Upstream(err)),
        },
    }
}

fn serve_stale(cached: Option<&CachedIndex>) -> Result<Option<ProjectDetail>, CacheError> {
    match cached {
        Some(record) => Ok(Some(decode_detail(&record.body)?)),
        None => Err(CacheError::Unavailable),
    }
}

fn store_fresh(
    state: &AppState,
    key: &str,
    project: &str,
    response: SimpleResponse,
    now: i64,
) -> Result<ProjectDetail, CacheError> {
    let parsed = parse_upstream(project, response.content_type.as_deref(), &response.url, &response.body)?;
    let files = parsed
        .files
        .into_iter()
        .map(|file| register_file(state, file))
        .collect::<Result<Vec<_>, _>>()?;
    let detail = ProjectDetail {
        meta: Meta::default(),
        name: parsed.name,
        versions: parsed.versions,
        files,
    };
    let record = CachedIndex {
        etag: response.etag,
        last_serial: response.last_serial,
        fetched_at_unix: now,
        body: to_json(&detail).into_bytes(),
    };
    state.meta.put_index(key, &record)?;
    state.meta.put_project(&state.index, project, &detail.name)?;
    Ok(detail)
}

/// The list of projects velox has observed on this index (its own `/simple/` root). A mirror does
/// not enumerate all of the upstream, so this reflects what has been requested and cached.
///
/// # Errors
/// Returns [`CacheError`] if the store read fails.
pub fn project_list(state: &AppState) -> Result<ProjectList, CacheError> {
    let projects = state
        .meta
        .list_projects(&state.index)?
        .into_iter()
        .map(|name| ProjectListEntry { name })
        .collect();
    Ok(ProjectList {
        meta: Meta::default(),
        projects,
    })
}

/// Parse an upstream simple page as PEP 691 JSON, or fall back to PEP 503 HTML for indexes that do
/// not serve JSON. This is the JSON-to-HTML degradation from the upstream adapter.
fn parse_upstream(
    project: &str,
    content_type: Option<&str>,
    url: &Url,
    body: &[u8],
) -> Result<ParsedDetail, CacheError> {
    if content_type.is_some_and(|content_type| content_type.contains("json")) {
        Ok(parse_detail(body)?)
    } else {
        Ok(parse_detail_html(project, &String::from_utf8_lossy(body), url))
    }
}

/// Record a file's upstream URL under its digest and rewrite its URL to velox's own file route.
/// A file without a sha256 hash is left as-is (it cannot be content-addressed).
fn register_file(state: &AppState, mut file: File) -> Result<File, CacheError> {
    // Phase 1 does not serve the PEP 658 `.metadata` sibling, so do not advertise it; clients
    // download the full artifact instead. Proper metadata backfill arrives in Phase 2.
    file.core_metadata = CoreMetadata::Absent;
    let Some(sha256) = file.hashes.get("sha256").cloned() else {
        return Ok(file);
    };
    state.meta.put_file_url(&sha256, &file.url)?;
    file.url = format!("/{}/files/{sha256}/{}", state.index, file.filename);
    Ok(file)
}

fn decode_detail(body: &[u8]) -> Result<ProjectDetail, CacheError> {
    let parsed = parse_detail(body)?;
    Ok(ProjectDetail {
        meta: Meta::default(),
        name: parsed.name,
        versions: parsed.versions,
        files: parsed.files,
    })
}

/// Resolve a file's bytes, serving the cached blob or fetching, verifying, and caching it.
///
/// # Errors
/// Returns [`CacheError::FileNotFound`] if the digest has no known source, or another
/// [`CacheError`] on a store or upstream failure.
pub async fn file_bytes(state: &AppState, digest: &Digest) -> Result<Bytes, CacheError> {
    if state.blobs.exists(digest) {
        return Ok(Bytes::from(state.blobs.read(digest)?));
    }
    let url = state
        .meta
        .get_file_url(digest.as_str())?
        .ok_or(CacheError::FileNotFound)?;
    let bytes = state.upstream.fetch_bytes(&url).await?;
    state.blobs.write_verified(&bytes, digest)?;
    Ok(bytes)
}
