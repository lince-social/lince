use super::*;

#[tokio::test]
async fn saved_credentials_survive_restart_without_any_password() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials");
    assert!(
        Store::new(path.clone())
            .key("account")
            .await
            .unwrap()
            .is_none()
    );
    Store::new(path.clone())
        .put("account".into(), Secret("saved token".into()))
        .await
        .unwrap();
    assert_eq!(
        Store::new(path).key("account").await.unwrap().unwrap().0,
        "saved token"
    );
}

#[tokio::test]
async fn independent_accounts_are_not_lost_and_stale_refresh_cannot_replace_a_login() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path().join("credentials"));
    store.put("a".into(), Secret("old".into())).await.unwrap();
    let (a, b) = tokio::join!(
        store.put("a".into(), Secret("new login".into())),
        store.put("b".into(), Secret("other account".into()))
    );
    a.unwrap();
    b.unwrap();
    store
        .refreshed(
            "a".into(),
            Secret("stale refresh".into()),
            Secret("old".into()),
        )
        .await
        .unwrap();
    assert_eq!(store.key("a").await.unwrap().unwrap().0, "new login");
    assert_eq!(store.key("b").await.unwrap().unwrap().0, "other account");
    store
        .refreshed(
            "a".into(),
            Secret("fresh".into()),
            Secret("new login".into()),
        )
        .await
        .unwrap();
    assert_eq!(store.key("a").await.unwrap().unwrap().0, "fresh");
}

#[tokio::test]
async fn invalid_large_replacement_keeps_the_previous_credential() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path().join("credentials"));
    store.put("a".into(), Secret("keep".into())).await.unwrap();
    assert!(
        store
            .put("a".into(), Secret("x".repeat(MAX_BYTES as usize)))
            .await
            .is_err()
    );
    assert_eq!(store.key("a").await.unwrap().unwrap().0, "keep");
}

#[cfg(unix)]
#[tokio::test]
async fn files_are_private_and_symlinked_or_shared_files_are_refused() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(directory.path().join("credentials"));
    store
        .put("a".into(), Secret("do not expose".into()))
        .await
        .unwrap();
    let path = store.path("a", "json");
    assert_eq!(
        fs::metadata(&store.directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let error = store.key("a").await.unwrap_err();
    assert!(!error.contains("do not expose"));
    fs::remove_file(&path).unwrap();
    let outside = directory.path().join("outside");
    fs::write(&outside, "unchanged").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(store.key("a").await.is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "unchanged");
}

#[tokio::test]
async fn refresh_guard_serializes_separate_store_instances() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials");
    let first = Store::new(path.clone()).refresh_guard("a").await.unwrap();
    let another = Store::new(path);
    let waiting = tokio::spawn(async move { another.refresh_guard("a").await });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    assert!(!waiting.is_finished());
    drop(first);
    tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn legacy_import_preserves_newer_logins_and_removal_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("credentials");
    let store = Store::new(path.clone());
    assert!(
        store
            .import_if_missing("a".into(), Secret("legacy".into()))
            .await
            .unwrap()
    );
    store
        .put("a".into(), Secret("new login".into()))
        .await
        .unwrap();
    assert!(
        !store
            .import_if_missing("a".into(), Secret("old legacy".into()))
            .await
            .unwrap()
    );
    assert_eq!(store.key("a").await.unwrap().unwrap().0, "new login");
    store.remove("a").await.unwrap();
    assert!(Store::new(path).key("a").await.unwrap().is_none());
}
