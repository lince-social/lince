//! Sync (blueprint XV): a fact package moves between two Cells, authorship
//! survives, import is idempotent, and deltas commute.

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::RecordKind;

async fn cell() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn plain(e: &Engine, slug: &str, q: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind: RecordKind::Plain,
            head: slug.into(),
            body: String::new(),
            quantity: q,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

#[tokio::test]
async fn a_signed_package_crosses_cells_and_stays_verifiable() {
    // Ana's Cell produces a visible, signed donation record
    let ana = cell().await;
    ana.set_signer(Signer::generate("ana", "ed25519:ana:2026-07"))
        .await
        .unwrap();
    let apples = plain(&ana, "ana.apples", 0.0).await;
    ana.append_user(&apples, 12.0).await.unwrap(); // grew 12 apples
    ana.append_user(&apples, -2.0).await.unwrap(); // ate 2 -> 10
    ana.act(
        Action::CreateConcept {
            lingua: store::linguas::LOCAL_UID.into(),
            name: "food".into(),
            parents: Vec::new(),
        },
        None,
    )
    .await
    .unwrap();
    ana.act(
        Action::AssertRecord {
            subject: apples.clone(),
            predicate: "food".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    ana.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.family".into()),
            target: apples.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let package = ana
        .export_package("organ.family", "organ.ana")
        .await
        .unwrap();
    assert_eq!(package.records.len(), 1);
    assert_eq!(package.assertions.len(), 1);
    assert!(package.facts.len() >= 2);

    // Bruno's Cell has Ana's public key and imports the package
    let bruno = cell().await;
    store::sqlx::query("INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)")
        .bind("ana")
        .bind("ed25519:ana:2026-07")
        // pull Ana's published key across
        .bind(
            store::sqlx::query_scalar::<_, String>(
                "SELECT public_key FROM identity_key WHERE actor_uid = 'ana'",
            )
            .fetch_one(&ana.store.pool)
            .await
            .unwrap(),
        )
        .execute(&bruno.store.pool)
        .await
        .unwrap();

    let applied = bruno.import_package(&package).await.unwrap();
    assert!(!applied.is_empty());
    // deltas commute: Bruno's copy reaches the same level Ana has
    assert_eq!(
        store::records::quantity(&bruno.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );
    // authorship survived replication: the facts still name Ana
    assert!(
        applied
            .iter()
            .all(|f| f.actor_uid.as_deref() == Some("ana"))
    );
    assert_eq!(
        store::assertions::concepts_for_record(&bruno.store.pool, &apples)
            .await
            .unwrap()
            .len(),
        1
    );

    // idempotent: re-importing changes nothing (replay safety)
    let again = bruno.import_package(&package).await.unwrap();
    assert!(again.is_empty());
    assert_eq!(
        store::records::quantity(&bruno.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );
}

#[tokio::test]
async fn export_respects_the_visibility_gate() {
    let ana = cell().await;
    let public = plain(&ana, "public.thing", 5.0).await;
    plain(&ana, "private.thing", 9.0).await;
    ana.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.x".into()),
            target: public,
        },
        None,
    )
    .await
    .unwrap();

    let package = ana.export_package("organ.x", "organ.ana").await.unwrap();
    assert_eq!(
        package.records.len(),
        1,
        "private record never leaves the Cell"
    );
    assert_eq!(package.records[0].slug.as_deref(), Some("public.thing"));
}

#[tokio::test]
async fn create_record_stamps_the_local_organ_as_origin() {
    let e = cell().await;
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .unwrap()
        .uid;
    let apple = plain(&e, "apple", 1.0).await;
    let row = store::records::get(&e.store.pool, &apple)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.organ_uid.as_deref(), Some(organ.as_str()));
}

#[tokio::test]
async fn organ_lineage_survives_a_relay_through_a_second_cell() {
    // Ana creates a record (stamped with her own organ), Bruno imports it,
    // then Carla imports FROM Bruno — the record's origin must still say Ana,
    // not Bruno (blueprint: Protein `organ_eq` must resolve the TRUE origin
    // even after relaying, not just the last hop).
    let ana = cell().await;
    let ana_organ = store::organs::ensure_local(&ana.store.pool, "http://cell-ana")
        .await
        .unwrap()
        .uid;
    let apples = plain(&ana, "ana.apples", 5.0).await;
    ana.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.everyone".into()),
            target: apples.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let bruno = cell().await;
    let package_to_bruno = ana
        .export_package("organ.everyone", &ana_organ)
        .await
        .unwrap();
    assert_eq!(
        package_to_bruno.records[0].organ_uid.as_deref(),
        Some(ana_organ.as_str())
    );
    bruno.import_package(&package_to_bruno).await.unwrap();
    bruno
        .act(
            Action::GrantVisibility {
                subject_kind: "organ".into(),
                subject: Some("organ.carla".into()),
                target: apples.clone(),
            },
            None,
        )
        .await
        .unwrap();

    let carla = cell().await;
    let bruno_organ = store::organs::ensure_local(&bruno.store.pool, "http://cell-bruno")
        .await
        .unwrap()
        .uid;
    let package_to_carla = bruno
        .export_package("organ.carla", &bruno_organ)
        .await
        .unwrap();
    assert_eq!(
        package_to_carla.records[0].organ_uid.as_deref(),
        Some(ana_organ.as_str()),
        "relayed record keeps Ana as its true origin, not Bruno"
    );
    carla.import_package(&package_to_carla).await.unwrap();
    let carla_row = store::records::get(&carla.store.pool, &apples)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(carla_row.organ_uid.as_deref(), Some(ana_organ.as_str()));
}

#[tokio::test]
async fn file_sync_writes_and_reimports_records_selected_by_a_protein() {
    use protein::{Predicate, Protein, Source};

    let ana = cell().await;
    let organ = store::organs::ensure_local(&ana.store.pool, "http://cell-ana")
        .await
        .unwrap()
        .uid;
    plain(&ana, "ana.exported", 3.0).await;
    plain(&ana, "ana.excluded", 9.0).await;

    let path = std::env::temp_dir().join(format!("{}.json", nucleus::new_uid("filesync-test")));

    let protein = Protein {
        source: Source::Record,
        filter: vec![Predicate::SlugEq("ana.exported".into())],
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let written = ana.sync_to_disk(&path, &protein).await.unwrap();
    assert_eq!(written, 1, "only the Protein-selected record was written");

    let bruno = cell().await;
    let applied = bruno.sync_from_disk(&path).await.unwrap();
    std::fs::remove_file(&path).ok();

    assert!(!applied.is_empty());
    let imported = store::records::resolve(&bruno.store.pool, "ana.exported")
        .await
        .unwrap()
        .expect("the selected record landed on Bruno's Cell");
    assert_eq!(imported.quantity, store::exact::from_f64(3.0));
    assert_eq!(
        imported.organ_uid.as_deref(),
        Some(organ.as_str()),
        "origin travels with a file-synced record too"
    );
    assert!(
        store::records::resolve(&bruno.store.pool, "ana.excluded")
            .await
            .unwrap()
            .is_none(),
        "the record the Protein did not select never left the disk file"
    );
}
