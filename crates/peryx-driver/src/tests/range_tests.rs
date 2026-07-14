//! Parsing a `Range` header against a representation of known size.

use crate::range::{RangeSpec, parse_range};

#[test]
fn test_parse_range_reads_a_satisfiable_span() {
    assert_eq!(parse_range("bytes=0-3", 10), RangeSpec::Satisfiable(0, 3));
    assert_eq!(parse_range("bytes=5-", 10), RangeSpec::Satisfiable(5, 9));
    assert_eq!(parse_range("bytes=-3", 10), RangeSpec::Satisfiable(7, 9));
    // An end past the last byte is clamped, not refused.
    assert_eq!(parse_range("bytes=8-99", 10), RangeSpec::Satisfiable(8, 9));
}

#[test]
fn test_parse_range_ignores_what_is_not_a_range() {
    // RFC 9110 s14.2: an unparseable `Range` is ignored, and the whole representation served.
    for header in [
        "bytes=",
        "bytes=-",
        "bytes=abc-",
        "bytes=-xyz",
        "bytes=5-2",
        "items=0-1",
        // One half of the pair reads and the other does not; still not a range.
        "bytes=1-abc",
        "bytes=abc-9",
        // Multiple ranges: no caller serves multipart.
        "bytes=0-1,3-4",
    ] {
        assert_eq!(parse_range(header, 10), RangeSpec::Ignore, "{header}");
    }
}

#[test]
fn test_parse_range_refuses_only_what_the_file_cannot_meet() {
    assert_eq!(parse_range("bytes=10-", 10), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=99-100", 10), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=-0", 10), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=0-0", 0), RangeSpec::Unsatisfiable);
}

#[test]
fn test_parse_range_serves_the_whole_file_for_an_oversized_suffix() {
    // RFC 9110 s14.1.2: a suffix longer than the representation uses the entire representation.
    assert_eq!(parse_range("bytes=-99", 10), RangeSpec::Satisfiable(0, 9));
}
