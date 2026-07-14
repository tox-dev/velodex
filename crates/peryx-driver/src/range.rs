//! HTTP single-range requests over a stored file of known size.
//!
//! Pulling one layer of a large image is often a range request, and pip resumes an interrupted wheel
//! download with one, so this is the grammar both blob servers lean on.

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;

/// The `416` a well-formed but unmeetable range earns, naming the size the client should retry against.
///
/// # Panics
/// Never in practice: the status and both header values are constructed here, not taken from a request.
#[must_use]
pub fn unsatisfiable_range(size: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes */{size}"))
        .body(Body::empty())
        .expect("range response builds from validated header parts")
}

/// What a `Range` header asks of a representation of a known size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSpec {
    /// Not a range this server understands. RFC 9110 s14.2: an unparseable `Range` is ignored and
    /// the whole representation served, never refused.
    Ignore,
    /// A well-formed range that no part of the representation can satisfy: a `416`.
    Unsatisfiable,
    /// An inclusive byte range within the representation.
    Satisfiable(u64, u64),
}

/// Parse a `Range: bytes=...` header against a known size, inclusive per HTTP semantics.
///
/// The distinction that matters is between a range that is *wrong* and one that is *unmeetable*.
/// `bytes=abc-` is not a range at all, so it is ignored; `bytes=9999-` against a 10-byte file is a
/// range that file cannot satisfy, so it earns a `416`. Answering `416` to the former tells a client
/// its perfectly good request was out of bounds.
#[must_use]
pub fn parse_range(header: &str, size: u64) -> RangeSpec {
    // No caller answers multipart, so a multi-range request is a range this server does not speak.
    if header.contains(',') {
        return RangeSpec::Ignore;
    }
    let Some(spec) = header.strip_prefix("bytes=") else {
        return RangeSpec::Ignore;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return RangeSpec::Ignore;
    };
    let Some(last_byte) = size.checked_sub(1) else {
        // An empty representation satisfies no range, but a well-formed one still earns a `416`.
        return RangeSpec::Unsatisfiable;
    };
    match (first.is_empty(), last.is_empty()) {
        // `bytes=-N`: the last N bytes. RFC 9110 s14.1.2: when N exceeds the size, the whole
        // representation is used rather than the range being refused.
        (true, false) => match last.parse::<u64>() {
            Ok(0) => RangeSpec::Unsatisfiable,
            Ok(suffix) => RangeSpec::Satisfiable(size.saturating_sub(suffix), last_byte),
            Err(_) => RangeSpec::Ignore,
        },
        // `bytes=N-`: from N to the end.
        (false, true) => match first.parse::<u64>() {
            Ok(start) if start > last_byte => RangeSpec::Unsatisfiable,
            Ok(start) => RangeSpec::Satisfiable(start, last_byte),
            Err(_) => RangeSpec::Ignore,
        },
        (false, false) => match (first.parse::<u64>(), last.parse::<u64>()) {
            // `last < first` is not a range; nothing can be read backwards.
            (Ok(start), Ok(end)) if start > end => RangeSpec::Ignore,
            (Ok(start), Ok(_)) if start > last_byte => RangeSpec::Unsatisfiable,
            (Ok(start), Ok(end)) => RangeSpec::Satisfiable(start, end.min(last_byte)),
            _ => RangeSpec::Ignore,
        },
        (true, true) => RangeSpec::Ignore,
    }
}
