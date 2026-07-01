use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use crate::config::Config;
use crate::server::build_router;

#[tokio::test]
async fn test_build_router_serves_status() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        ..Config::default()
    };
    let router = build_router(&config).unwrap();

    let response = router
        .oneshot(Request::builder().uri("/+status").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("root/pypi"));
}

#[test]
fn test_build_router_data_dir_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("afile");
    std::fs::write(&file, "x").unwrap();
    let config = Config {
        data_dir: file.join("sub"),
        ..Config::default()
    };
    assert!(build_router(&config).is_err());
}

#[test]
fn test_build_router_rejects_bad_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        upstream_url: "not a url".to_owned(),
        ..Config::default()
    };
    assert!(build_router(&config).is_err());
}
