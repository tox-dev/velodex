//! The machine-readable `(rule, field)` every upload rejection carries for a browser upload UI.

use rstest::rstest;

use crate::upload::UploadError;
use crate::{DistributionFilenameError, MetadataError};

fn text(value: &str) -> String {
    value.to_owned()
}

#[rstest]
#[case::action(UploadError::NotFileUpload, "unsupported-action", ":action")]
#[case::missing(UploadError::Missing("content"), "missing-field", "content")]
#[case::name(UploadError::InvalidName(text("A B")), "invalid-name", "name")]
#[case::version(UploadError::InvalidVersion(text("x")), "invalid-version", "version")]
#[case::filename(UploadError::InvalidFilename(text("../x")), "invalid-filename", "content")]
#[case::distribution_filename(
    UploadError::InvalidDistributionFilename { filename: text("x.txt"), error: DistributionFilenameError::UnsupportedExtension },
    "invalid-distribution-filename",
    "content"
)]
#[case::filetype(
    UploadError::FiletypeMismatch { expected: text("bdist_wheel"), actual: text("sdist") },
    "filetype-mismatch",
    "filetype"
)]
#[case::filename_name(
    UploadError::FilenameNameMismatch { filename: text("flask"), form: text("django") },
    "filename-name-mismatch",
    "content"
)]
#[case::filename_version(
    UploadError::FilenameVersionMismatch { filename: text("1.0"), form: text("2.0") },
    "filename-version-mismatch",
    "content"
)]
#[case::digest_mismatch(UploadError::DigestMismatch("sha256_digest"), "digest-mismatch", "sha256_digest")]
#[case::invalid_digest(
    UploadError::InvalidDigest { field: "md5_digest", value: text("zz") },
    "invalid-digest",
    "md5_digest"
)]
#[case::requires_python(
    UploadError::InvalidRequiresPython(text(">=x")),
    "invalid-requires-python",
    "requires_python"
)]
#[case::content(UploadError::InvalidContent(text("bad zip")), "invalid-content", "content")]
#[case::metadata_utf8(UploadError::InvalidMetadataUtf8, "invalid-metadata-utf8", "content")]
#[case::malformed_metadata(
    UploadError::MalformedMetadata(MetadataError::MissingHeaderName(text(": value"))),
    "malformed-metadata",
    "content"
)]
#[case::project_url(
    UploadError::InvalidProjectUrl { label: text("Home"), url: text("ftp://x") },
    "invalid-project-url",
    "project_urls"
)]
#[case::license_file(
    UploadError::InvalidLicenseFile { value: text("../LICENSE"), reason: "escapes the project root" },
    "invalid-license-file",
    "license_files"
)]
#[case::conflicting_license(UploadError::ConflictingLicenseFields, "conflicting-license-fields", "license")]
#[case::missing_metadata_version(UploadError::MissingMetadataVersion, "missing-metadata-version", "metadata_version")]
#[case::unsupported_metadata_version(
    UploadError::UnsupportedMetadataVersion(text("3.0")),
    "unsupported-metadata-version",
    "metadata_version"
)]
#[case::metadata_value(
    UploadError::InvalidMetadataValue { field: "Summary", value: text("multi\nline"), reason: "spans lines" },
    "invalid-metadata-value",
    "Summary"
)]
#[case::metadata_name(
    UploadError::MetadataNameMismatch { metadata: text("Flask"), form: text("django") },
    "metadata-name-mismatch",
    "name"
)]
#[case::metadata_version_mismatch(
    UploadError::MetadataVersionMismatch { metadata: text("1.0"), form: text("2.0") },
    "metadata-version-mismatch",
    "version"
)]
#[case::metadata_field(
    UploadError::MetadataFieldMismatch { field: "Requires-Python", metadata: text(">=3.8"), form: text(">=3.9") },
    "metadata-field-mismatch",
    "Requires-Python"
)]
#[case::upload_time(UploadError::InvalidUploadTime, "invalid-upload-time", "upload_time")]
fn test_upload_error_rule_names_the_rule_and_field(
    #[case] error: UploadError,
    #[case] rule: &str,
    #[case] field: &str,
) {
    assert_eq!(error.rule(), (rule, field));
}
