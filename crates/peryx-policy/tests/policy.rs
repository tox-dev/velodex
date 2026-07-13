use peryx_policy::{Policy, PolicyAction, PolicyConfig};

#[test]
fn check_size_allows_the_configured_limit() {
    let policy = Policy::compile(
        &PolicyConfig {
            max_file_size_bytes: Some(4),
            ..PolicyConfig::default()
        },
        str::to_owned,
    );

    assert_eq!(policy.check_size(PolicyAction::Upload, "project", 4), Ok(()));
}
