use super::*;

fn actions() -> Actions {
    Actions::new(
        Signer::from_bytes("actor", "key", [7; 32]),
        "session".into(),
        "challenge".into(),
    )
}

fn edit() -> Action {
    Action::EditRecordText {
        target: "record".into(),
        head: None,
        body: Some("draft".into()),
    }
}

fn signed(message: ClientMessage) -> (String, u64) {
    let ClientMessage::SignedAct { id, sequence, .. } = message else {
        panic!("The live Action was not signed");
    };
    (id, sequence)
}

#[test]
fn repeated_reply_ids_get_distinct_signed_identities_and_restore_errors_and_successes() {
    let mut actions = actions();
    let (first, first_sequence) = signed(actions.prepare("autosave".into(), edit()).unwrap());
    assert_ne!(first, "autosave");
    assert_eq!(first_sequence, 1);
    let mut refused = ServerMessage::Error {
        id: first.clone(),
        message: "permission denied".into(),
        code: Some("forbidden".into()),
    };
    actions.restore(&mut refused);
    assert!(
        matches!(refused, ServerMessage::Error { id, message, code } if id == "autosave" && message == "permission denied" && code.as_deref() == Some("forbidden"))
    );
    let (next, next_sequence) = signed(actions.prepare("autosave".into(), edit()).unwrap());
    assert_ne!(next, first);
    assert_eq!(next_sequence, 2);
    let mut saved = ServerMessage::ActionOk {
        id: next,
        created: Some("created".into()),
        facts: 3,
        warnings: vec!["warning".into()],
        data: Some(serde_json::json!({ "saved": true })),
    };
    actions.restore(&mut saved);
    assert!(
        matches!(saved, ServerMessage::ActionOk { id, created, facts: 3, warnings, data } if id == "autosave" && created.as_deref() == Some("created") && warnings == ["warning"] && data == Some(serde_json::json!({ "saved": true })))
    );
    assert!(actions.replies.is_empty());
    let (third, sequence) = signed(actions.prepare("autosave".into(), edit()).unwrap());
    assert_ne!(third, first);
    assert_eq!(sequence, 3);
}

#[test]
fn backpressure_and_invalid_ids_never_create_sequence_gaps_or_unbounded_pending_state() {
    let mut actions = actions();
    for id in [String::new(), "\n".into(), "x".repeat(513)] {
        assert!(
            matches!(actions.prepare(id, edit()), Err(ServerMessage::Error { code, .. }) if code.as_deref() == Some("live_action_id_invalid"))
        );
    }
    assert_eq!(actions.sequence, 0);
    for index in 0..MAX_PENDING {
        let (_, sequence) = signed(actions.prepare(format!("edit-{index}"), edit()).unwrap());
        assert_eq!(sequence, index as u64 + 1);
    }
    assert!(
        matches!(actions.prepare("overflow".into(), edit()), Err(ServerMessage::Error { id, code, .. }) if id == "overflow" && code.as_deref() == Some("live_action_busy"))
    );
    assert_eq!(actions.replies.len(), MAX_PENDING);
    let (wire, original) = actions.replies.first_key_value().unwrap();
    let original = original.clone();
    let mut refused = ServerMessage::Error {
        id: wire.clone(),
        message: "refused".into(),
        code: None,
    };
    actions.restore(&mut refused);
    assert!(matches!(refused, ServerMessage::Error { id, .. } if id == original));
    let (_, sequence) = signed(actions.prepare("retry".into(), edit()).unwrap());
    assert_eq!(sequence, MAX_PENDING as u64 + 1);
    assert_eq!(actions.replies.len(), MAX_PENDING);
}

#[test]
fn unrelated_updates_and_duplicate_replies_cannot_consume_another_action_mapping() {
    let mut actions = actions();
    let (wire, _) = signed(actions.prepare("autosave".into(), edit()).unwrap());
    let mut snapshot = ServerMessage::Snapshot {
        id: wire.clone(),
        rows: Vec::new(),
    };
    actions.restore(&mut snapshot);
    assert!(matches!(snapshot, ServerMessage::Snapshot { id, .. } if id == wire));
    assert_eq!(actions.replies.len(), 1);
    let mut response = ServerMessage::Error {
        id: wire.clone(),
        message: "refused".into(),
        code: None,
    };
    actions.restore(&mut response);
    assert!(matches!(response, ServerMessage::Error { id, .. } if id == "autosave"));
    let mut duplicate = ServerMessage::Error {
        id: wire.clone(),
        message: "duplicate".into(),
        code: None,
    };
    actions.restore(&mut duplicate);
    assert!(matches!(duplicate, ServerMessage::Error { id, .. } if id == wire));
    actions.sequence = u64::MAX;
    assert!(
        matches!(actions.prepare("exhausted".into(), edit()), Err(ServerMessage::Error { code, .. }) if code.as_deref() == Some("live_action_sequence_exhausted"))
    );
    assert!(actions.replies.is_empty());
}
