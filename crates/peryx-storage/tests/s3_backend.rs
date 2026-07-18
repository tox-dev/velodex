//! S3-compatible blob backend exercised against an in-process S3 mock.
//!
//! The mock keeps objects and multipart parts in memory and speaks enough of the REST surface for the
//! backend to put, get, range, head, verify, delete, and multipart-upload without a network or real
//! bucket. Focused mocks cover the failure branches: retries, unexpected status, aborted multipart,
//! and malformed responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use peryx_storage::blob::{
    BlobBlocking, BlobErrorKind, BlobStorage, Digest, S3Client, S3Config, S3Credentials, S3Settings,
};
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Default)]
struct BucketState {
    objects: HashMap<String, Vec<u8>>,
    uploads: HashMap<String, HashMap<u32, Vec<u8>>>,
    next_upload: u64,
    fail_parts: bool,
    fail_complete: bool,
}

#[derive(Clone)]
struct MockS3 {
    state: Arc<Mutex<BucketState>>,
}

impl MockS3 {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BucketState::default())),
        }
    }
}

impl Respond for MockS3 {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let queries: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let key = request
            .url
            .path()
            .strip_prefix("/bucket/")
            .unwrap_or_default()
            .to_owned();
        route(&mut self.state.lock().unwrap(), request, &queries, &key)
    }
}

fn route(state: &mut BucketState, request: &Request, queries: &HashMap<String, String>, key: &str) -> ResponseTemplate {
    if queries.contains_key("list-type") {
        return ResponseTemplate::new(200).set_body_string("<ListBucketResult></ListBucketResult>");
    }
    match request.method.as_str() {
        "POST" if queries.contains_key("uploads") => {
            state.next_upload += 1;
            let id = format!("upload-{}", state.next_upload);
            state.uploads.insert(id.clone(), HashMap::new());
            ResponseTemplate::new(200).set_body_string(format!(
                "<InitiateMultipartUploadResult><UploadId>{id}</UploadId></InitiateMultipartUploadResult>"
            ))
        }
        "POST" if queries.contains_key("uploadId") => {
            if state.fail_complete {
                return ResponseTemplate::new(200)
                    .set_body_string("<Error><Code>InternalError</Code><Message>boom</Message></Error>");
            }
            let parts = state.uploads.remove(&queries["uploadId"]).unwrap_or_default();
            let mut numbers = parts.keys().copied().collect::<Vec<_>>();
            numbers.sort_unstable();
            let body = numbers.into_iter().flat_map(|number| parts[&number].clone()).collect();
            state.objects.insert(key.to_owned(), body);
            ResponseTemplate::new(200).set_body_string("<CompleteMultipartUploadResult/>")
        }
        "PUT" if queries.contains_key("uploadId") => {
            if state.fail_parts {
                return ResponseTemplate::new(500).set_body_string("<Error><Code>SlowDown</Code></Error>");
            }
            let number = queries["partNumber"].parse().unwrap();
            state
                .uploads
                .entry(queries["uploadId"].clone())
                .or_default()
                .insert(number, request.body.clone());
            ResponseTemplate::new(200).append_header("ETag", format!("\"part-{number}\""))
        }
        "PUT" => {
            state.objects.insert(key.to_owned(), request.body.clone());
            ResponseTemplate::new(200).append_header("ETag", "\"object\"")
        }
        "HEAD" => state.objects.get(key).map_or_else(
            || ResponseTemplate::new(404),
            |bytes| ResponseTemplate::new(200).insert_header("content-length", bytes.len().to_string()),
        ),
        "GET" => state.objects.get(key).map_or_else(
            || ResponseTemplate::new(404).set_body_string("<Error><Code>NoSuchKey</Code></Error>"),
            |bytes| {
                request.headers.get("range").map_or_else(
                    || ResponseTemplate::new(200).set_body_bytes(bytes.clone()),
                    |range| {
                        let (start, end) = parse_range(range.to_str().unwrap(), bytes.len());
                        ResponseTemplate::new(206)
                            .append_header("Content-Range", format!("bytes {start}-{}/{}", end - 1, bytes.len()))
                            .set_body_bytes(bytes[start..end].to_vec())
                    },
                )
            },
        ),
        "DELETE" if queries.contains_key("uploadId") => {
            state.uploads.remove(&queries["uploadId"]);
            ResponseTemplate::new(204)
        }
        "DELETE" => {
            state.objects.remove(key);
            ResponseTemplate::new(204)
        }
        other => ResponseTemplate::new(400).set_body_string(format!("unexpected {other}")),
    }
}

fn parse_range(header: &str, len: usize) -> (usize, usize) {
    let spec = header.strip_prefix("bytes=").unwrap();
    let (start, end) = spec.split_once('-').unwrap();
    (start.parse().unwrap(), end.parse::<usize>().unwrap().min(len - 1) + 1)
}

fn settings(endpoint: String) -> S3Settings {
    S3Settings {
        endpoint,
        bucket: "bucket".to_owned(),
        prefix: String::new(),
        region: "us-east-1".to_owned(),
        path_style: true,
        request_timeout: Duration::from_secs(5),
        max_retries: 2,
        multipart_threshold: 64 << 20,
        part_size: 8 << 20,
        upload_concurrency: 2,
    }
}

fn credentials() -> S3Credentials {
    S3Credentials {
        access_key_id: "test".to_owned(),
        secret_access_key: "test".to_owned(),
        session_token: Some("token".to_owned()),
    }
}

async fn mounted() -> (MockServer, MockS3) {
    let server = MockServer::start().await;
    let mock = MockS3::new();
    Mock::given(any()).respond_with(mock.clone()).mount(&server).await;
    (server, mock)
}

fn storage(server: &MockServer, overrides: impl FnOnce(&mut S3Settings)) -> (BlobStorage, tempfile::TempDir) {
    let mut settings = settings(server.uri());
    overrides(&mut settings);
    let staging = tempfile::tempdir().unwrap();
    let config = S3Config::new(settings).unwrap();
    (
        BlobStorage::s3(config, credentials(), staging.path().to_path_buf()),
        staging,
    )
}

#[tokio::test]
async fn test_s3_storage_round_trips_a_streamed_blob() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});

    storage.health().await.unwrap();
    assert_eq!(storage.name(), "s3");
    assert_eq!(storage.capabilities().durability.as_str(), "object-store");

    let digest = Digest::of(b"package");
    assert!(storage.head(&digest).await.unwrap().is_none());

    let mut write = storage.begin().await.unwrap();
    write.write_chunk(bytes::Bytes::from_static(b"pack")).await.unwrap();
    assert!(write.tail().unwrap().open().is_ok());
    write.flush().await.unwrap();
    write.write_chunk(bytes::Bytes::from_static(b"age")).await.unwrap();
    write.commit(&digest).await.unwrap();

    assert_eq!(storage.head(&digest).await.unwrap().unwrap().bytes, 7);
    assert_eq!(storage.read_bytes(&digest, 64).await.unwrap(), b"package");
    let ranged = storage
        .open(&digest, Some(1..5))
        .await
        .unwrap()
        .collect(64)
        .await
        .unwrap();
    assert_eq!(ranged, b"acka");
    assert!(storage.verify(&digest).await.unwrap());

    let lease = storage.materialize(&digest).await.unwrap();
    assert_eq!(std::fs::read(lease.path()).unwrap(), b"package");
    drop(lease);

    assert_eq!(
        storage
            .present(vec![digest.clone(), Digest::of(b"absent")])
            .await
            .unwrap(),
        std::collections::HashSet::from([digest.clone()])
    );
    assert!(storage.delete(&digest).await.unwrap());
    assert!(!storage.delete(&digest).await.unwrap());
}

#[tokio::test]
async fn test_s3_put_bytes_uses_multipart_above_the_threshold() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |settings| {
        settings.multipart_threshold = 4;
        settings.part_size = 5;
    });
    let payload = b"multipart streaming payload".to_vec();
    let digest = storage.put_bytes(&payload).await.unwrap();
    assert_eq!(digest, Digest::of(&payload));
    assert_eq!(storage.read_bytes(&digest, 1 << 20).await.unwrap(), payload);
    assert!(storage.verify(&digest).await.unwrap());
}

#[tokio::test]
async fn test_s3_staging_exposes_the_local_stage_before_commit() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});

    let staged = storage.stage_bytes(b"payload").await.unwrap();
    assert!(!staged.is_empty());
    assert_eq!(staged.len(), 7);
    assert_eq!(staged.digest(), &Digest::of(b"payload"));
    assert_eq!(
        staged.with_materialized(|path| std::fs::read(path).unwrap()),
        b"payload"
    );
    staged.commit().await.unwrap();
    assert_eq!(
        storage.read_bytes(&Digest::of(b"payload"), 64).await.unwrap(),
        b"payload"
    );
}

#[tokio::test]
async fn test_s3_writes_can_be_aborted_before_commit() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});

    let mut write = storage.begin().await.unwrap();
    write
        .write_chunk(bytes::Bytes::from_static(b"unpublished"))
        .await
        .unwrap();
    write.abort().await.unwrap();

    storage.stage_bytes(b"dropped").await.unwrap().abort().await.unwrap();
    assert!(storage.head(&Digest::of(b"dropped")).await.unwrap().is_none());
}

#[tokio::test]
async fn test_s3_missing_blob_reports_not_found() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});
    let digest = Digest::of(b"missing");
    assert_eq!(
        storage.open(&digest, None).await.map(drop).unwrap_err().kind(),
        BlobErrorKind::NotFound
    );
    // A ranged open first HEADs the object; a missing one is not found before any range check.
    assert_eq!(
        storage.open(&digest, Some(0..4)).await.map(drop).unwrap_err().kind(),
        BlobErrorKind::NotFound
    );
    assert_eq!(
        storage.verify(&digest).await.unwrap_err().kind(),
        BlobErrorKind::NotFound
    );
    assert_eq!(
        storage.materialize(&digest).await.unwrap_err().kind(),
        BlobErrorKind::NotFound
    );
    assert!(!storage.delete(&digest).await.unwrap());
}

#[tokio::test]
async fn test_s3_open_rejects_a_range_past_the_object() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});
    let digest = storage.put_bytes(b"short").await.unwrap();
    let error = storage.open(&digest, Some(2..99)).await.map(drop).unwrap_err();
    assert_eq!(error.kind(), BlobErrorKind::InvalidRange);
}

#[tokio::test]
async fn test_s3_commit_rejects_a_digest_mismatch() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});
    let mut write = storage.begin().await.unwrap();
    write.write_chunk(bytes::Bytes::from_static(b"bytes")).await.unwrap();
    let error = write.commit(&Digest::of(b"other")).await.unwrap_err();
    assert_eq!(error.kind(), BlobErrorKind::DigestMismatch);
    assert_eq!(error.context().unwrap().backend, "s3");
}

#[tokio::test]
async fn test_s3_health_retries_a_transient_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("<ListBucketResult/>"))
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |_| {});
    storage.health().await.unwrap();
}

#[tokio::test]
async fn test_s3_read_operations_surface_a_server_error() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500).set_body_string("<Error><Code>InternalError</Code></Error>"))
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |settings| settings.max_retries = 0);
    let digest = Digest::of(b"x");
    assert_eq!(storage.head(&digest).await.unwrap_err().kind(), BlobErrorKind::Io);
    assert_eq!(storage.verify(&digest).await.unwrap_err().kind(), BlobErrorKind::Io);
    assert_eq!(
        storage.materialize(&digest).await.unwrap_err().kind(),
        BlobErrorKind::Io
    );
    // A ranged open HEADs first; a server error there surfaces before the body is fetched.
    assert_eq!(
        storage.open(&digest, Some(0..2)).await.map(drop).unwrap_err().kind(),
        BlobErrorKind::Io
    );
    // Delete HEADs to learn whether the object existed; a server error there surfaces too.
    assert_eq!(storage.delete(&digest).await.unwrap_err().kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_begin_surfaces_a_staging_failure() {
    let (server, _mock) = mounted().await;
    // The staging path is a regular file, so creating the staging directory fails with a
    // not-a-directory error that no user, root included, can bypass.
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("staging");
    std::fs::write(&staging, b"regular file, not a directory").unwrap();
    let config = S3Config::new(settings(server.uri())).unwrap();
    let storage = BlobStorage::s3(config, credentials(), staging);
    assert_eq!(storage.begin().await.map(|_| ()).unwrap_err().kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_commit_surfaces_a_missing_stage() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});
    let staged = storage.stage_bytes(b"vanishes").await.unwrap();
    // Remove the local stage before commit reads it to upload. A missing file fails the read for any
    // user, unlike a permission bit that root ignores, and there is no permission to restore that
    // could race the stage's asynchronous drop.
    std::fs::remove_file(staged.with_materialized(std::path::Path::to_path_buf)).unwrap();
    assert_eq!(staged.commit().await.unwrap_err().kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_delete_surfaces_a_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-length", "5"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(500).set_body_string("<Error><Code>InternalError</Code></Error>"))
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |settings| settings.max_retries = 0);
    assert_eq!(
        storage.delete(&Digest::of(b"x")).await.unwrap_err().kind(),
        BlobErrorKind::Io
    );
}

#[tokio::test]
async fn test_s3_multipart_abort_swallows_a_cleanup_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                "<InitiateMultipartUploadResult><UploadId>u</UploadId></InitiateMultipartUploadResult>",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(500).set_body_string("<Error><Code>InternalError</Code></Error>"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(500).set_body_string("<Error><Code>InternalError</Code></Error>"))
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |settings| {
        settings.multipart_threshold = 2;
        settings.part_size = 4;
        settings.max_retries = 0;
    });
    // The part upload fails and the abort cleanup also fails; the original error still surfaces.
    assert_eq!(
        storage.put_bytes(b"needs multiple parts").await.unwrap_err().kind(),
        BlobErrorKind::Io
    );
}

#[tokio::test]
async fn test_s3_health_reports_a_missing_bucket() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(404).set_body_string("<Error><Code>NoSuchBucket</Code></Error>"))
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |_| {});
    assert_eq!(storage.health().await.unwrap_err().kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_unexpected_status_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(403).set_body_string("<Error><Code>AccessDenied</Code><Message>no</Message></Error>"),
        )
        .mount(&server)
        .await;
    let (storage, _staging) = storage(&server, |_| {});
    let error = storage.health().await.unwrap_err();
    assert_eq!(error.kind(), BlobErrorKind::Io);
    assert!(error.to_string().contains("s3 blob backend health"));
}

#[tokio::test]
async fn test_s3_transport_failure_exhausts_retries() {
    let (storage, _staging) = {
        let mut settings = settings("http://127.0.0.1:1".to_owned());
        settings.max_retries = 1;
        settings.request_timeout = Duration::from_secs(2);
        let staging = tempfile::tempdir().unwrap();
        let config = S3Config::new(settings).unwrap();
        (
            BlobStorage::s3(config, credentials(), staging.path().to_path_buf()),
            staging,
        )
    };
    assert_eq!(storage.health().await.unwrap_err().kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_multipart_aborts_when_a_part_fails() {
    let (server, mock) = mounted().await;
    mock.state.lock().unwrap().fail_parts = true;
    let (storage, _staging) = storage(&server, |settings| {
        settings.multipart_threshold = 2;
        settings.part_size = 4;
        settings.max_retries = 0;
    });
    let error = storage.put_bytes(b"needs multiple parts").await.unwrap_err();
    assert_eq!(error.kind(), BlobErrorKind::Io);
}

#[tokio::test]
async fn test_s3_multipart_surfaces_a_completion_error() {
    let (server, mock) = mounted().await;
    mock.state.lock().unwrap().fail_complete = true;
    let (storage, _staging) = storage(&server, |settings| {
        settings.multipart_threshold = 2;
        settings.part_size = 4;
    });
    let error = storage.put_bytes(b"needs multiple parts").await.unwrap_err();
    assert_eq!(error.kind(), BlobErrorKind::Io);
    let source = std::error::Error::source(&error).unwrap().to_string();
    assert!(source.contains("InternalError"), "{source}");
}

#[tokio::test]
async fn test_s3_client_rejects_a_multipart_response_without_an_upload_id() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("<InitiateMultipartUploadResult/>"))
        .mount(&server)
        .await;
    let client = S3Client::new(S3Config::new(settings(server.uri())).unwrap(), credentials());
    assert!(client.create_multipart("sha256/key").await.is_err());
}

#[tokio::test]
async fn test_s3_client_rejects_a_part_upload_without_an_etag() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let client = S3Client::new(S3Config::new(settings(server.uri())).unwrap(), credentials());
    let error = client
        .upload_part(
            "sha256/key",
            "upload-1",
            1,
            bytes::Bytes::from_static(b"part"),
            &"0".repeat(64),
        )
        .await;
    assert!(error.is_err());
}

#[tokio::test]
async fn test_s3_blocking_facade_is_unsupported() {
    let (server, _mock) = mounted().await;
    let (storage, _staging) = storage(&server, |_| {});
    let blocking: BlobBlocking<'_> = storage.blocking();
    let digest = Digest::of(b"x");
    assert_eq!(blocking.head(&digest).unwrap_err().kind(), BlobErrorKind::Unsupported);
    assert_eq!(blocking.verify(&digest).unwrap_err().kind(), BlobErrorKind::Unsupported);
    assert_eq!(blocking.delete(&digest).unwrap_err().kind(), BlobErrorKind::Unsupported);
    assert_eq!(
        blocking.read_bytes(&digest, 16).unwrap_err().kind(),
        BlobErrorKind::Unsupported
    );
    assert_eq!(
        blocking.materialize(&digest).unwrap_err().kind(),
        BlobErrorKind::Unsupported
    );
    assert_eq!(blocking.put_bytes(b"x").unwrap_err().kind(), BlobErrorKind::Unsupported);
    assert!(blocking.visit(|_| Ok::<(), std::convert::Infallible>(())).is_err());
}
