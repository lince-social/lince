use engine::{
    Engine,
    actions::Action,
    area_transition::{RecordChanges, TransitionPreview},
};

async fn fixture() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Area work".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "working".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    (engine, uid)
}

async fn preview(engine: &Engine, uid: &str, changes: RecordChanges) -> TransitionPreview {
    serde_json::from_value(
        engine
            .act(
                Action::PreviewAreaTransition {
                    target: uid.into(),
                    changes,
                    constraints: Default::default(),
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap()
}

fn change(quantity: &str, add: bool) -> RecordChanges {
    RecordChanges {
        quantity: Some(quantity.into()),
        assert: if add { vec!["working".into()] } else { vec![] },
        retract: if add { vec![] } else { vec!["working".into()] },
    }
}

async fn quantity(engine: &Engine, uid: &str) -> String {
    store::records::quantity(&engine.store.pool, uid)
        .await
        .unwrap()
        .unwrap()
        .to_string()
}

async fn assertions(engine: &Engine, uid: &str) -> usize {
    store::assertions::list_active(&engine.store.pool)
        .await
        .unwrap()
        .iter()
        .filter(|assertion| assertion.subject_uid == uid && assertion.role == "ordinary")
        .count()
}

#[tokio::test]
async fn preview_is_read_only_and_entry_and_exit_change_exact_quantity_and_assertions_together() {
    let (engine, uid) = fixture().await;
    let entry = preview(&engine, &uid, change("-3.125", true)).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    assert_eq!(assertions(&engine, &uid).await, 0);
    let outcome = engine
        .act(
            Action::ApplyAreaTransition {
                request_id: "entry".into(),
                preview: entry,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(outcome.facts.len(), 1);
    assert_eq!(quantity(&engine, &uid).await, "-3.125");
    assert_eq!(assertions(&engine, &uid).await, 1);
    let exit = preview(&engine, &uid, change("1", false)).await;
    engine
        .act(
            Action::ApplyAreaTransition {
                request_id: "exit".into(),
                preview: exit,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(quantity(&engine, &uid).await, "1.000");
    assert_eq!(assertions(&engine, &uid).await, 0);
}

#[tokio::test]
async fn duplicate_requests_do_not_repeat_or_overwrite_later_edits() {
    let (engine, uid) = fixture().await;
    let preview = preview(&engine, &uid, change("-3", true)).await;
    let action = Action::ApplyAreaTransition {
        request_id: "same-crossing".into(),
        preview,
    };
    let (first, second) = tokio::join!(
        engine.act(action.clone(), None),
        engine.act(action.clone(), None)
    );
    assert_eq!(first.unwrap().facts.len() + second.unwrap().facts.len(), 1);
    assert_eq!(assertions(&engine, &uid).await, 1);
    engine
        .act(
            Action::SetQuantityExact {
                target: uid.clone(),
                amount: "99".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(action.clone(), None)
            .await
            .unwrap()
            .facts
            .is_empty()
    );
    assert_eq!(quantity(&engine, &uid).await, "99");
    let mut changed = action;
    if let Action::ApplyAreaTransition {
        ref mut preview, ..
    } = changed
    {
        preview.changes.quantity = Some("8".into());
    }
    assert_eq!(
        engine.act(changed, None).await.unwrap_err().code(),
        Some("area_transition_invalid")
    );
}

#[tokio::test]
async fn stale_quantity_or_assertions_refuse_the_entire_change() {
    let (engine, uid) = fixture().await;
    let planned = preview(&engine, &uid, change("-3", true)).await;
    engine
        .act(
            Action::SetQuantityExact {
                target: uid.clone(),
                amount: "5".into(),
            },
            None,
        )
        .await
        .unwrap();
    let action = Action::ApplyAreaTransition {
        request_id: "stale-quantity".into(),
        preview: planned,
    };
    assert_eq!(
        engine.act(action, None).await.unwrap_err().code(),
        Some("area_transition_stale")
    );
    assert_eq!(assertions(&engine, &uid).await, 0);
    let planned = preview(&engine, &uid, change("-3", true)).await;
    engine
        .act(
            Action::AssertRecord {
                subject: uid.clone(),
                predicate: "working".into(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let action = Action::ApplyAreaTransition {
        request_id: "stale-assertions".into(),
        preview: planned,
    };
    assert_eq!(
        engine.act(action, None).await.unwrap_err().code(),
        Some("area_transition_stale")
    );
    assert_eq!(quantity(&engine, &uid).await, "5");
    assert_eq!(assertions(&engine, &uid).await, 1);
}

#[tokio::test]
async fn permissions_and_read_filters_are_checked_for_preview_and_apply() {
    let (engine, uid) = fixture().await;
    let planned = preview(&engine, &uid, change("-3", true)).await;
    let role = store::auth::ensure_role(&engine.store.pool, "area-viewer")
        .await
        .unwrap();
    let actor =
        store::auth::create_person_login(&engine.store.pool, "Reader", "reader", "hash", role)
            .await
            .unwrap();
    let preview = Action::PreviewAreaTransition {
        target: uid.clone(),
        changes: change("-3", true),
        constraints: Default::default(),
    };
    let apply = Action::ApplyAreaTransition {
        request_id: "forbidden".into(),
        preview: planned,
    };
    for action in [preview.clone(), apply.clone()] {
        assert_eq!(
            engine
                .act(action, Some(actor.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
    }
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .set_read_filter(&actor, Some(&protein::Predicate::KindEq("organ".into())))
        .await
        .unwrap();
    for action in [preview, apply] {
        assert!(
            engine
                .act(action, Some(actor.clone()))
                .await
                .unwrap_err()
                .to_string()
                .contains("outside what this login may see")
        );
    }
    assert_eq!(quantity(&engine, &uid).await, "0");
    assert_eq!(assertions(&engine, &uid).await, 0);
}

#[tokio::test]
async fn malformed_or_conflicting_changes_never_mutate_records() {
    let (engine, uid) = fixture().await;
    for changes in [
        RecordChanges::default(),
        RecordChanges {
            quantity: Some("NaN".into()),
            ..Default::default()
        },
        RecordChanges {
            assert: vec!["working".into()],
            retract: vec!["working".into()],
            ..Default::default()
        },
        RecordChanges {
            assert: vec!["missing".into()],
            quantity: Some("-3".into()),
            ..Default::default()
        },
    ] {
        assert!(
            engine
                .act(
                    Action::PreviewAreaTransition {
                        target: uid.clone(),
                        changes,
                        constraints: Default::default()
                    },
                    None
                )
                .await
                .is_err()
        );
    }
    assert_eq!(quantity(&engine, &uid).await, "0");
    assert_eq!(assertions(&engine, &uid).await, 0);
}

#[tokio::test]
async fn overlap_constraints_resolve_names_before_checking_for_conflicts() {
    let (engine, uid) = fixture().await;
    let predicate = store::concepts::resolve(&engine.store.pool, "working")
        .await
        .unwrap()
        .unwrap();
    let result = engine
        .act(
            Action::PreviewAreaTransition {
                target: uid.clone(),
                changes: RecordChanges {
                    retract: vec!["working".into()],
                    ..Default::default()
                },
                constraints: RecordChanges {
                    assert: vec![predicate],
                    ..Default::default()
                },
            },
            None,
        )
        .await;
    assert_eq!(result.unwrap_err().code(), Some("area_transition_invalid"));
    assert_eq!(assertions(&engine, &uid).await, 0);
}
