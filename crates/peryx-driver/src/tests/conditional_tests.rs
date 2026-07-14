//! Matching an `If-None-Match` or `If-Range` field against an entity tag.

use axum::http::HeaderMap;
use rstest::rstest;

use crate::conditional::{applicable_range, if_none_match};

const ETAG: &str = "\"9f86d081\"";

#[rstest]
#[case::exact("\"9f86d081\"")]
#[case::weak("W/\"9f86d081\"")]
#[case::any("*")]
#[case::list("\"other\", \"9f86d081\"")]
#[case::list_unspaced("\"other\",W/\"9f86d081\"")]
fn test_if_none_match_names_the_representation(#[case] field: &str) {
    assert!(if_none_match(field, ETAG), "{field}");
}

#[rstest]
#[case::other_tag("\"other\"")]
#[case::unquoted("9f86d081")]
#[case::prefix("\"9f86d081x\"")]
#[case::empty("")]
#[case::malformed("W/*")]
fn test_if_none_match_leaves_the_full_response(#[case] field: &str) {
    assert!(!if_none_match(field, ETAG), "{field}");
}

fn headers(fields: &[(&str, &str)]) -> HeaderMap {
    fields
        .iter()
        .map(|(name, value)| {
            (
                name.parse().expect("a test header name"),
                value.parse().expect("a test header value"),
            )
        })
        .collect()
}

#[rstest]
#[case::no_condition(&[("range", "bytes=0-3")])]
#[case::matching_tag(&[("range", "bytes=0-3"), ("if-range", ETAG)])]
fn test_applicable_range_serves_the_range(#[case] fields: &[(&str, &str)]) {
    assert_eq!(applicable_range(&headers(fields), ETAG), Some("bytes=0-3"));
}

#[rstest]
#[case::stale_tag("\"other\"")]
#[case::weak_tag("W/\"9f86d081\"")]
#[case::date("Wed, 21 Oct 2015 07:28:00 GMT")]
#[case::malformed("9f86d081")]
#[case::any("*")]
fn test_applicable_range_drops_the_range_a_stale_if_range_asks_for(#[case] field: &str) {
    let fields = [("range", "bytes=0-3"), ("if-range", field)];
    assert_eq!(applicable_range(&headers(&fields), ETAG), None, "{field}");
}

#[test]
fn test_applicable_range_ignores_an_if_range_without_a_range() {
    assert_eq!(applicable_range(&headers(&[("if-range", "\"other\"")]), ETAG), None);
}
