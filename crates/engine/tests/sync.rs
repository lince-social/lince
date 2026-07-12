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
            .unwrap(),
        Some(10.0)
    );
    // authorship survived replication: the facts still name Ana
    assert!(
        applied
            .iter()
            .all(|f| f.actor_uid.as_deref() == Some("ana"))
    );

    // idempotent: re-importing changes nothing (replay safety)
    let again = bruno.import_package(&package).await.unwrap();
    assert!(again.is_empty());
    assert_eq!(
        store::records::quantity(&bruno.store.pool, &apples)
            .await
            .unwrap(),
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
