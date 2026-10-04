use nucleus::sand_package::{self as model, Identity, Kind, License, Manifest, Package};
use sqlx::Row;

fn package() -> Package {
    let mut package = Package {
        format: model::FORMAT.into(),
        manifest: Manifest {
            identity: Identity {
                origin: nucleus::new_uid("r"),
                id: nucleus::new_uid("r"),
                version: 1,
            },
            name: "Shared Castle".into(),
            kind: Kind::Castle,
            author: nucleus::new_uid("r"),
            execution: "future.runtime.v1".into(),
            permissions: vec!["camera".into()],
            licenses: vec![License {
                name: "Example".into(),
                text: "Keep author attribution".into(),
            }],
            credits: vec!["Example Author".into()],
            key_id: "key1".into(),
            public_key: "key".into(),
        },
        payload: "Preserved opaque content".into(),
        digest: String::new(),
        signature: "signature".into(),
    };
    package.digest = package.content_digest().unwrap();
    package
}

#[tokio::test]
async fn received_packages_survive_reopening_and_stay_private_without_executable_records() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("library.db").display());
    let store = store::Store::open_durable(&url).await.unwrap();
    let package = package();
    let identity = package.manifest.identity.clone();
    assert!(
        store::sand_packages::save(&store.pool, &package, false, Some(&identity.origin))
            .await
            .unwrap()
    );
    store.pool.close().await;
    let reopened = store::Store::open_existing_durable(&url).await.unwrap();
    let stored = store::sand_packages::get(&reopened.pool, &identity, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.package, package);
    assert!(!stored.public);
    assert!(!stored.origin_verified);
    assert!(
        store::sand_packages::get(&reopened.pool, &identity, true)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::records::get(&reopened.pool, &identity.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn immutable_package_versions_keep_publication_choices_on_repeated_receipt() {
    let store = store::Store::open_memory().await.unwrap();
    let original = package();
    let identity = original.manifest.identity.clone();
    assert!(
        store::sand_packages::save(&store.pool, &original, true, None)
            .await
            .unwrap()
    );
    store::sand_packages::set_public(&store.pool, &identity, true)
        .await
        .unwrap();
    assert!(
        store::sand_packages::save(&store.pool, &original, false, Some(&identity.origin))
            .await
            .unwrap()
    );
    let stored = store::sand_packages::get(&store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.public);
    assert!(stored.origin_verified);
    let mut different = original.clone();
    different.manifest.author = nucleus::new_uid("r");
    different.digest = different.content_digest().unwrap();
    assert!(
        !store::sand_packages::save(&store.pool, &different, true, None)
            .await
            .unwrap()
    );
    assert_eq!(
        store::sand_packages::get(&store.pool, &identity, false)
            .await
            .unwrap()
            .unwrap()
            .package,
        original
    );
    different.payload = "x".repeat(model::MAX_BYTES + 1);
    assert!(
        store::sand_packages::save(&store.pool, &different, true, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn catalogue_pages_use_metadata_and_public_index_instead_of_loading_payloads() {
    let store = store::Store::open_memory().await.unwrap();
    for index in 0..model::PAGE_SIZE + 2 {
        let mut package = package();
        package.payload = "x".repeat(200_000);
        package.digest = package.content_digest().unwrap();
        assert!(
            store::sand_packages::save(&store.pool, &package, true, None)
                .await
                .unwrap()
        );
        if index < model::PAGE_SIZE + 1 {
            store::sand_packages::set_public(&store.pool, &package.manifest.identity, true)
                .await
                .unwrap();
        }
    }
    let entries = store::sand_packages::list(&store.pool, true, 0)
        .await
        .unwrap();
    assert_eq!(entries.len(), model::PAGE_SIZE as usize + 1);
    let encoded = serde_json::to_string(&entries).unwrap();
    assert!(encoded.len() < 32_000);
    assert!(!encoded.contains("payload"));
    assert!(!encoded.contains("xxxxxxxx"));
    let plan = sqlx::query("EXPLAIN QUERY PLAN SELECT manifest, public, origin_verified FROM sand_package WHERE public = 1 ORDER BY origin, id, version DESC LIMIT 33 OFFSET 0")
        .fetch_all(&store.pool).await.unwrap();
    assert!(plan.iter().any(|row| {
        row.get::<String, _>("detail")
            .contains("sand_package_public")
    }));
    assert!(
        !plan
            .iter()
            .any(|row| row.get::<String, _>("detail").contains("TEMP B-TREE"))
    );
}
