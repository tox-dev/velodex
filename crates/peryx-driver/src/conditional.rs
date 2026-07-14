//! Conditional requests against the entity tag of a content-addressed representation.
//!
//! The verified digest names an artifact's bytes, so it is the strong validator for them. A client
//! holding those bytes gets its answer from the request line, with no blob opened and no upstream
//! fetch started, which is why this sits next to the range grammar both blob servers lean on.

use axum::http::{HeaderMap, header};

/// Does an `If-None-Match` field name the representation `etag` identifies?
///
/// RFC 9110 s13.1.2: `*` matches whenever a representation exists, a list matches when any member
/// does, and the comparison is weak, so `W/"x"` and `"x"` name the same bytes. A member that is not
/// an entity tag matches nothing, which leaves the full response a `304` would have replaced.
#[must_use]
pub fn if_none_match(field: &str, etag: &str) -> bool {
    field
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag)
}

/// The `Range` to serve of the representation `etag` identifies, or `None` for the whole of it.
///
/// RFC 9110 s13.1.5: an `If-Range` carries the validator of the copy the client is resuming, and the
/// range is honored only while that validator still names the current representation. Anything else
/// drops the `Range` and serves the whole `200` the client would otherwise have to ask for a second
/// time: a weak tag says the bytes may differ, and a date names a validator these responses never
/// send, so neither can vouch for what the client already holds. A `416` would be wrong here — the
/// request is well formed, the copy behind it is stale.
///
/// Only an exact strong match lets the range through, which is a byte comparison against a tag the
/// caller already holds: no allocation on the artifact hot path. An `If-Range` on a request without a
/// `Range` conditions nothing, so it is ignored.
#[must_use]
pub fn applicable_range<'h>(headers: &'h HeaderMap, etag: &str) -> Option<&'h str> {
    let range = headers.get(header::RANGE)?.to_str().ok()?;
    headers.get(header::IF_RANGE).map_or(Some(range), |field| {
        field.to_str().is_ok_and(|field| field == etag).then_some(range)
    })
}
