use engine::{
    Engine,
    actions::Action,
    record_creation::{Assertion, Draft},
};
use serde_json::json;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    store::organs::ensure_local(&engine.store.pool, "http://local")
        .await
        .unwrap();
    engine
}

#[tokio::test]
async fn creation_preserves_exact_quantity_work_and_retries_without_duplicates() {
    let engine = engine().await;
    let draft = Draft {
        head: "A prepared Record".into(),
        body: "Olá 👩‍💻".into(),
        slug: Some("prepared".into()),
        quantity: "9007199254740993.125".into(),
        work: json!({"start":"2026-09-19","due":"2026-09-20","estimate_min":35,"logs":[{"start":"2026-09-19T10:00:00Z","end":"2026-09-19T10:15:00Z"}]}),
        ..Default::default()
    };
    let uid = engine
        .act(
            Action::CreateRecordDraft {
                draft: draft.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(uid, draft.uid);
    let row = store::records::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.quantity.to_string(), "9007199254740993.125");
    assert_eq!(row.body, draft.body);
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, "work")
            .await
            .unwrap()
            .unwrap(),
        draft.work
    );
    let seq = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    assert_eq!(
        engine
            .act(
                Action::CreateRecordDraft {
                    draft: draft.clone()
                },
                None
            )
            .await
            .unwrap()
            .created,
        Some(uid.clone())
    );
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        seq
    );
    let changed = Draft {
        head: "Different".into(),
        ..draft.clone()
    };
    assert!(
        engine
            .act(Action::CreateRecordDraft { draft: changed }, None)
            .await
            .is_err()
    );
    let duplicate = Draft {
        uid: nucleus::new_uid("r"),
        ..draft
    };
    assert!(
        engine
            .act(
                Action::CreateRecordDraft {
                    draft: duplicate.clone()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        store::records::get(&engine.store.pool, &duplicate.uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn invalid_fields_and_relations_leave_no_record_or_sync_operations() {
    let engine = engine().await;
    for draft in [
        Draft {
            quantity: "not a number".into(),
            ..Default::default()
        },
        Draft {
            work: json!({"start":"not a date"}),
            ..Default::default()
        },
        Draft {
            assertions: vec![Assertion {
                predicate: "missing-assertion".into(),
                object: None,
                quantity: None,
                unit: None,
            }],
            ..Default::default()
        },
        Draft {
            slug: Some("invalid slug".into()),
            ..Default::default()
        },
    ] {
        let seq = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
        assert!(
            engine
                .act(
                    Action::CreateRecordDraft {
                        draft: draft.clone()
                    },
                    None
                )
                .await
                .is_err()
        );
        assert!(
            store::records::get(&engine.store.pool, &draft.uid)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
            seq
        );
    }
}

#[tokio::test]
async fn assertions_are_atomic_with_creation_and_invalid_amount_rolls_everything_back() {
    let engine = engine().await;
    let predicate = store::concepts::create(&engine.store.pool, "planned", &[])
        .await
        .unwrap();
    let mut draft = Draft {
        assertions: vec![Assertion {
            predicate: predicate.clone(),
            object: None,
            quantity: Some("invalid".into()),
            unit: None,
        }],
        ..Default::default()
    };
    let seq = store::sync_ops::max_seq(&engine.store.pool).await.unwrap();
    assert!(
        engine
            .act(
                Action::CreateRecordDraft {
                    draft: draft.clone()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        store::records::get(&engine.store.pool, &draft.uid)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store::sync_ops::max_seq(&engine.store.pool).await.unwrap(),
        seq
    );
    draft.assertions[0].quantity = Some("1.125".into());
    engine
        .act(
            Action::CreateRecordDraft {
                draft: draft.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let assertions = store::assertions::for_subjects(&engine.store.pool, &[draft.uid])
        .await
        .unwrap();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0].predicate_uid, predicate);
    assert_eq!(assertions[0].quantity.unwrap().to_string(), "1.125");
}

#[tokio::test]
async fn creation_requires_permission_even_for_a_retry() {
    let engine = engine().await;
    let role = store::auth::ensure_role(&engine.store.pool, "draft-viewer")
        .await
        .unwrap();
    let actor = store::auth::create_person_login(
        &engine.store.pool,
        "Viewer",
        "draft-viewer",
        "hash",
        role,
    )
    .await
    .unwrap();
    let draft = Draft::default();
    assert!(
        engine
            .act(
                Action::CreateRecordDraft {
                    draft: draft.clone()
                },
                Some(actor.clone())
            )
            .await
            .is_err()
    );
    assert!(
        store::records::get(&engine.store.pool, &draft.uid)
            .await
            .unwrap()
            .is_none()
    );
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "create")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::CreateRecordDraft {
                    draft: draft.clone()
                },
                Some(actor.clone())
            )
            .await
            .is_ok()
    );
    store::auth::revoke(&engine.store.pool, role, permission)
        .await
        .unwrap();
    assert!(
        engine
            .act(Action::CreateRecordDraft { draft }, Some(actor))
            .await
            .is_err()
    );
}
