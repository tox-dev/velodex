use peryx_identity::{IndexAcl, Principal, UserLifecycleChange, UserState};

use crate::users::UserService;

fn service() -> (tempfile::TempDir, UserService) {
    let dir = tempfile::tempdir().unwrap();
    let store = peryx_storage::meta::MetaStore::open(dir.path().join("peryx.redb")).unwrap();
    (dir, UserService::new(store))
}

#[test]
fn test_user_service_runs_the_account_lifecycle() {
    let (_dir, service) = service();
    let user = service.create("Alice").unwrap();

    assert_eq!(service.rename(&user.id, "Alice").unwrap(), user);
    service.rename(&user.id, "ALICE").unwrap();
    let renamed = service.rename(&user.id, "Alice Cooper").unwrap();
    let disabled = service.disable(&user.id).unwrap();
    assert_eq!(service.disable(&user.id).unwrap(), disabled);

    assert_eq!(renamed.id, user.id);
    assert_eq!(disabled.state, UserState::Disabled);
    assert_eq!(service.inspect(&user.id).unwrap(), Some(disabled));
    assert_eq!(service.identify("ALICE COOPER").unwrap(), None);

    let active = service.reactivate(&user.id).unwrap();
    assert_eq!(service.identify("alice cooper").unwrap(), Some(active));
    assert_eq!(
        service.events(&user.id).unwrap()[4].change,
        UserLifecycleChange::Reactivated
    );
}

#[test]
fn test_user_disable_does_not_change_legacy_upload_token_identity() {
    let (_dir, service) = service();
    let user = service.create("Alice").unwrap();
    let acl = IndexAcl::upload_token("s3cret");
    let before = acl.identify(Some("Basic YWxpY2U6czNjcmV0"), 0).principal;

    service.disable(&user.id).unwrap();

    assert_eq!(
        before,
        Principal::Named {
            subject: "upload_token".to_owned(),
        }
    );
    assert_eq!(acl.identify(Some("Basic YWxpY2U6czNjcmV0"), 0).principal, before);
}
