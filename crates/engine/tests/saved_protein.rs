//! Stage 8b: a saved Protein is a `kind='protein'` record (its AST in the
//! `lince.protein` extension) — the successor to a named SQL view. `SaveProtein`
//! is a full-CRUD upsert so the sand-settings Protein editor can create AND edit.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use serde_json::json;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn ast(e: &Engine, uid: &str) -> serde_json::Value {
    store::records::get_extension(&e.store.pool, uid, "lince.protein")
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn save_protein_creates_updates_deletes_and_reactivates() {
    let e = engine().await;

    // create
    let created = e
        .act(
            Action::SaveProtein {
                slug: "views.stock".into(),
                head: "Stock".into(),
                ast: json!({ "source": "record" }),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let row = store::records::get(&e.store.pool, &created)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.kind, RecordKind::Protein.as_str());
    assert_eq!(row.head, "Stock");
    assert_eq!(ast(&e, &created).await, json!({ "source": "record" }));

    // update by same slug: SAME record (upsert), new head + AST
    let updated = e
        .act(
            Action::SaveProtein {
                slug: "views.stock".into(),
                head: "Stock v2".into(),
                ast: json!({ "source": "record", "limit": 10 }),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        updated, created,
        "upsert must reuse the record, not collide"
    );
    let updated_row = store::records::get(&e.store.pool, &created)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_row.head, "Stock v2");
    assert_eq!(
        ast(&e, &created).await,
        json!({ "source": "record", "limit": 10 })
    );
    // the AST is mirrored into body so a records Protein can list saved Proteins
    // with their query text for the editor.
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&updated_row.body).unwrap(),
        json!({ "source": "record", "limit": 10 })
    );

    // delete = deactivate (append-only: hide, don't erase)
    e.act(
        Action::Deactivate {
            target: "views.stock".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::records::quantity(&e.store.pool, &created)
            .await
            .unwrap().map(|q| q.to_f64()),
        Some(0.0)
    );

    // saving again reactivates the same record
    let back = e
        .act(
            Action::SaveProtein {
                slug: "views.stock".into(),
                head: "Back".into(),
                ast: json!({ "source": "record" }),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(back, created);
    assert_eq!(
        store::records::quantity(&e.store.pool, &created)
            .await
            .unwrap().map(|q| q.to_f64()),
        Some(1.0)
    );
}

#[tokio::test]
async fn save_protein_refuses_to_clobber_a_non_protein_slug() {
    let e = engine().await;
    e.act(
        Action::CreateRecord {
            slug: Some("plain.thing".into()),
            kind: RecordKind::Plain,
            head: "Thing".into(),
            body: String::new(),
            quantity: 0.0,
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        e.act(
            Action::SaveProtein {
                slug: "plain.thing".into(),
                head: "nope".into(),
                ast: json!({ "source": "record" }),
            },
            None,
        )
        .await
        .is_err()
    );
}
