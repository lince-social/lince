mod support;

use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use store::{
    record_move::offers::{self, Bundle, Preview},
    records::NewRecord,
};

async fn pair() -> (Engine, Engine, String, String) {
    let a = Engine::open_memory().await.unwrap();
    let b = Engine::open_memory().await.unwrap();
    let ao = store::organs::local(&a.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bo = store::organs::local(&b.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    support::karma::authorize(&a).await;
    support::karma::authorize(&b).await;
    a.adopt_introduction(&b.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    store::organs::set_trust(&a.store.pool, &bo, "known")
        .await
        .unwrap();
    store::organs::set_trust(&b.store.pool, &ao, "known")
        .await
        .unwrap();
    a.adopt_roster(&b.roster_of(&bo).await.unwrap().unwrap())
        .await
        .unwrap();
    b.adopt_roster(&a.roster_of(&ao).await.unwrap().unwrap())
        .await
        .unwrap();
    store::organs::set_sync_policy(&a.store.pool, &bo, true, true)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, &ao, true, true)
        .await
        .unwrap();
    (a, b, ao, bo)
}
async fn record(engine: &Engine, slug: &str) -> String {
    store::records::create(
        &engine.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "Move this text",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}
async fn offer(a: &Engine, root: &str, peer: &str) -> (String, Preview, Bundle) {
    let (preview, bundle) = a.preview_record_move(root, peer).await.unwrap();
    let uid = a
        .offer_record_move(root, peer, Some(&preview.hash))
        .await
        .unwrap();
    (uid, preview, bundle)
}
async fn accept(a: &Engine, b: &Engine, ao: &str, uid: &str, preview: &Preview) {
    assert_eq!(
        b.receive_move_offer(ao, uid, preview.clone())
            .await
            .unwrap(),
        "offered"
    );
    b.answer_record_move(uid, true).await.unwrap();
    assert!(a.claim_record_move(uid).await.unwrap());
}

#[tokio::test]
async fn only_explicit_acceptance_and_durable_receipt_release_the_source() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "deed").await;
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    b.receive_move_offer(&ao, &uid, preview.clone())
        .await
        .unwrap();
    store::organs::advance_peer_acked_seq(&a.store.pool, &bo, i64::MAX)
        .await
        .unwrap();
    a.drain_outbox(|_, _, _| async { engine::sync::Delivery::Sent })
        .await
        .unwrap();
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::records::get(&b.store.pool, &root)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        b.receive_move_bundle(&ao, &uid, bundle.clone())
            .await
            .is_err()
    );
    assert!(a.finish_record_move(&uid, &preview.hash).await.is_err());
    accept(&a, &b, &ao, &uid, &preview).await;
    let receipt = b
        .receive_move_bundle(&ao, &uid, bundle.clone())
        .await
        .unwrap();
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        b.receive_move_bundle(&ao, &uid, bundle).await.unwrap(),
        receipt
    );
    a.finish_record_move(&uid, &receipt).await.unwrap();
    a.finish_record_move(&uid, &receipt).await.unwrap();
    assert!(offers::payload(&a.store.pool, &uid).await.is_err());
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_none()
    );
    let received = store::records::get(&b.store.pool, &root)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.body, "Move this text");
    assert_eq!(received.organ_uid.as_deref(), Some(ao.as_str()));
    assert!(
        store::records::restore(&a.store.pool, &root, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn assertion_endpoints_and_karma_rules_move_as_one_paused_set() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "root").await;
    let child = record(&a, "child").await;
    let predicate = store::concepts::create(&a.store.pool, "contains", &[])
        .await
        .unwrap();
    let assertion = store::assertions::assert(
        &a.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &root,
            predicate_uid: &predicate,
            object_uid: Some(&child),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let now = nucleus::execution::now();
    let rule = store::recurrence::create(
        &a.store.pool,
        store::recurrence::NewRecurrence {
            record_uid: &child,
            consequences: nucleus::karma::Consequences::capture(store::exact::zero(), None),
            condition: None,
            note: Some("Included Karma"),
            cadence: nucleus::karma::Cadence::every_months(1),
            anchor_at: now,
            request_id: "move-rule",
            actor_uid: None,
        },
        now,
    )
    .await
    .unwrap()
    .rule()
    .uid
    .clone();
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    assert_eq!(preview.records.len(), 2);
    assert_eq!(preview.assertions, 1);
    assert_eq!(preview.karma_rules, 1);
    accept(&a, &b, &ao, &uid, &preview).await;
    let hash = b.receive_move_bundle(&ao, &uid, bundle).await.unwrap();
    a.finish_record_move(&uid, &hash).await.unwrap();
    for record in [&root, &child] {
        assert!(
            store::records::get(&a.store.pool, record)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store::records::get(&b.store.pool, record)
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        store::assertions::get(&b.store.pool, &assertion)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::recurrence::get(&b.store.pool, &rule)
            .await
            .unwrap()
            .unwrap()
            .is_paused()
    );
    assert!(
        store::recurrence::get(&a.store.pool, &rule)
            .await
            .unwrap()
            .unwrap()
            .is_paused()
    );
    assert!(
        store::recurrence::all(&a.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store::recurrence::all(&b.store.pool).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn incoming_karma_reads_and_shared_fields_are_included_in_the_preview() {
    use nucleus::karma::rule_field::{RuleFieldInput, RuleFieldKind};
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "input").await;
    let first = record(&a, "first-output").await;
    let second = record(&a, "second-output").await;
    let text = |source: &str| RuleFieldInput::Text {
        source: source.into(),
    };
    let rule = a
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [text("@input"), text(">0"), text("@first-output")],
                request_id: "move-first-rule".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let field = store::karma_fields::for_rule(&a.store.pool, &rule)
        .await
        .unwrap()
        .into_iter()
        .find(|f| f.kind == RuleFieldKind::Condition)
        .unwrap();
    a.act(
        Action::SaveKarmaRule {
            identity: None,
            rule: None,
            expected_revision: None,
            fields: [
                RuleFieldInput::Reference {
                    uid: field.uid,
                    revision: field.revision,
                },
                text(">0"),
                text("@second-output"),
            ],
            request_id: "move-second-rule".into(),
        },
        None,
    )
    .await
    .unwrap();
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    assert_eq!(
        preview
            .records
            .iter()
            .map(|r| r.uid.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        [root.clone(), first, second].into_iter().collect()
    );
    assert_eq!(preview.karma_rules, 2);
    accept(&a, &b, &ao, &uid, &preview).await;
    let receipt = b.receive_move_bundle(&ao, &uid, bundle).await.unwrap();
    a.finish_record_move(&uid, &receipt).await.unwrap();
    assert!(
        store::recurrence::all(&a.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store::recurrence::all(&b.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|r| r.is_paused())
    );
    assert_eq!(
        store::karma_fields::for_rule(&b.store.pool, &rule)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn typed_karma_definitions_preserve_their_revision_identities() {
    use nucleus::karma::{CapabilitySet, ProgramAst, ProgramSchema, Slug, TimestampMs};
    let (a, b, ao, bo) = pair().await;
    let program = a
        .act(
            Action::CreateKarmaProgram {
                request_id: "move-program".into(),
                program: ProgramAst {
                    schema: ProgramSchema::V1,
                    slug: Slug::new("move-program").unwrap(),
                    purpose: "Move this program".into(),
                    tags: Default::default(),
                    parameters: Default::default(),
                    nodes: Default::default(),
                    outputs: Default::default(),
                    required_capabilities: CapabilitySet::default(),
                },
                owner_person_uid: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let frequency = a
        .act(
            Action::CreateKarmaFrequency {
                request_id: "move-frequency".into(),
                frequency: nucleus::karma::simple_frequency::frequency_from_cadence(
                    Slug::new("move-frequency").unwrap(),
                    "Move this frequency".into(),
                    &nucleus::karma::Cadence::every_months(1),
                    TimestampMs::parse_canonical("2026-09-01T12:00:00.000Z").unwrap(),
                )
                .unwrap(),
                owner_person_uid: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let old_program = store::karma::programs::get_handle(&a.store.pool, &program)
        .await
        .unwrap()
        .unwrap();
    let old_frequency = store::karma::frequencies::get_handle(&a.store.pool, &frequency)
        .await
        .unwrap()
        .unwrap();
    for root in [&program, &frequency] {
        let (uid, preview, bundle) = offer(&a, root, &bo).await;
        accept(&a, &b, &ao, &uid, &preview).await;
        let receipt = b.receive_move_bundle(&ao, &uid, bundle).await.unwrap();
        a.finish_record_move(&uid, &receipt).await.unwrap();
    }
    let moved_program = store::karma::programs::get_handle(&b.store.pool, &program)
        .await
        .unwrap()
        .unwrap();
    let moved_frequency = store::karma::frequencies::get_handle(&b.store.pool, &frequency)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        old_program.head_revision_hash,
        moved_program.head_revision_hash
    );
    assert_eq!(
        old_frequency.head_revision_hash,
        moved_frequency.head_revision_hash
    );
    assert!(
        store::karma::programs::get_handle(&a.store.pool, &program)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::karma::frequencies::get_handle(&a.store.pool, &frequency)
            .await
            .unwrap()
            .is_none()
    );
    assert_ne!(
        moved_program.status,
        nucleus::karma::DefinitionStatus::Active
    );
    assert_ne!(
        moved_frequency.status,
        nucleus::karma::DefinitionStatus::Active
    );
}

#[tokio::test]
async fn signed_origin_evidence_survives_a_move() {
    let (a, b, ao, bo) = pair().await;
    let created = a
        .act(
            Action::CreateRecord {
                slug: Some("signed".into()),
                kind: RecordKind::Plain,
                head: "Signed".into(),
                body: "Evidence".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    let root = created.created.unwrap();
    let original = created.facts[0].clone();
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    accept(&a, &b, &ao, &uid, &preview).await;
    let hash = b.receive_move_bundle(&ao, &uid, bundle).await.unwrap();
    a.finish_record_move(&uid, &hash).await.unwrap();
    let received = store::facts::get(&b.store.pool, &original.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.hash, original.hash);
    assert_eq!(received.signature, original.signature);
    let origin: (String, String) =
        store::sqlx::query_as("SELECT organ_uid,cell_uid FROM fact_origin WHERE fact_uid=?")
            .bind(&original.uid)
            .fetch_one(&b.store.pool)
            .await
            .unwrap();
    assert_eq!(origin.0, ao);
    assert_eq!(
        origin.1,
        store::cells::local(&a.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid
    );
    assert!(
        engine::trust::verify_fact(&b.store, &received)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn misleading_or_oversized_previews_cannot_commit_a_move() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "reviewed").await;
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    let mut oversized = preview.clone();
    oversized.records = vec![preview.records[0].clone(); offers::MAX_RECORDS + 1];
    assert!(
        b.receive_move_offer(&ao, "too-many", oversized)
            .await
            .is_err()
    );
    let mut misleading = preview;
    misleading.records[0].title = "Different content".into();
    b.receive_move_offer(&ao, &uid, misleading).await.unwrap();
    b.answer_record_move(&uid, true).await.unwrap();
    assert!(a.claim_record_move(&uid).await.unwrap());
    assert!(b.receive_move_bundle(&ao, &uid, bundle).await.is_err());
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::records::get(&b.store.pool, &root)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn destination_identity_conflicts_preserve_both_local_records() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "source-identity").await;
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    store::records::create_with_uid(
        &b.store.pool,
        NewRecord {
            slug: Some("existing-identity"),
            kind: RecordKind::Plain,
            head: "Existing local content",
            body: "Keep this content",
            quantity: store::exact::zero(),
        },
        &root,
    )
    .await
    .unwrap();
    accept(&a, &b, &ao, &uid, &preview).await;
    assert!(b.receive_move_bundle(&ao, &uid, bundle).await.is_err());
    assert_eq!(
        store::records::get(&b.store.pool, &root)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Keep this content"
    );
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        offers::get(&b.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .state,
        "accepted"
    );
}

#[tokio::test]
async fn declining_is_private_and_does_not_reopen_when_the_sender_retries() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "private-refusal").await;
    let (uid, preview, _) = offer(&a, &root, &bo).await;
    b.receive_move_offer(&ao, &uid, preview.clone())
        .await
        .unwrap();
    b.answer_record_move(&uid, false).await.unwrap();
    assert_eq!(
        b.receive_move_offer(&ao, &uid, preview).await.unwrap(),
        "offered"
    );
    assert_eq!(
        offers::get(&b.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .state,
        "declined"
    );
    assert!(
        b.pending_offer_status().await.unwrap()["pending"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["subject_uid"] != uid)
    );
    assert_eq!(
        offers::get(&a.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .state,
        "offered"
    );
}

#[tokio::test]
async fn source_cancellation_and_acceptance_claim_have_one_winner() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "race").await;
    let (uid, preview, _) = offer(&a, &root, &bo).await;
    b.receive_move_offer(&ao, &uid, preview).await.unwrap();
    b.answer_record_move(&uid, true).await.unwrap();
    let (claimed, cancelled) = tokio::join!(a.claim_record_move(&uid), a.cancel_record_move(&uid));
    let claimed = claimed.unwrap();
    assert_ne!(claimed, cancelled.is_ok());
    if claimed {
        assert!(a.cancel_record_move(&uid).await.is_err());
    } else {
        b.receive_move_cancel(&ao, &uid).await.unwrap();
        assert_eq!(
            offers::get(&b.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .state,
            "cancelled"
        );
    }
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn cancellation_before_a_delayed_offer_prevents_reopening_it() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "delayed-offer").await;
    let (uid, preview, _) = offer(&a, &root, &bo).await;
    a.cancel_record_move(&uid).await.unwrap();
    b.receive_move_cancel(&ao, &uid).await.unwrap();
    assert_eq!(
        b.receive_move_offer(&ao, &uid, preview).await.unwrap(),
        "cancelled"
    );
    assert!(offers::get(&b.store.pool, &uid).await.unwrap().is_none());
    assert!(offers::payload(&a.store.pool, &uid).await.is_err());
    assert!(
        b.pending_offer_status().await.unwrap()["pending"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["subject_uid"] != uid)
    );
}

#[tokio::test]
async fn stale_preview_and_dependency_edits_keep_the_source() {
    let (a, _b, _ao, bo) = pair().await;
    let root = record(&a, "edited").await;
    let (preview, _) = a.preview_record_move(&root, &bo).await.unwrap();
    store::records::set_text(&a.store.pool, &root, Some("Changed"), None)
        .await
        .unwrap();
    assert!(
        a.offer_record_move(&root, &bo, Some(&preview.hash))
            .await
            .is_err()
    );
    let (uid, _, _) = offer(&a, &root, &bo).await;
    store::records::set_text(&a.store.pool, &root, Some("Changed again"), None)
        .await
        .unwrap();
    assert!(!a.claim_record_move(&uid).await.unwrap());
    assert_eq!(
        offers::get(&a.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .state,
        "changed"
    );
    a.cancel_record_move(&uid).await.unwrap();
}

#[tokio::test]
async fn permission_changes_and_foreign_payloads_never_delete_the_source() {
    let (a, b, ao, bo) = pair().await;
    let root = record(&a, "guarded").await;
    let (uid, preview, mut bundle) = offer(&a, &root, &bo).await;
    accept(&a, &b, &ao, &uid, &preview).await;
    store::organs::set_sync_policy(&b.store.pool, &ao, true, false)
        .await
        .unwrap();
    assert!(
        b.receive_move_bundle(&ao, &uid, bundle.clone())
            .await
            .is_err()
    );
    store::organs::set_sync_policy(&b.store.pool, &ao, true, true)
        .await
        .unwrap();
    bundle.tables[3].rows[0][0] = store::snapshot::Value::Text(nucleus::new_uid("r"));
    assert!(b.receive_move_bundle(&ao, &uid, bundle).await.is_err());
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::records::get(&b.store.pool, &root)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn offers_for_overlapping_dependency_sets_are_refused() {
    let (a, _b, _ao, bo) = pair().await;
    let root = record(&a, "exclusive").await;
    let (_, preview, _) = offer(&a, &root, &bo).await;
    assert!(
        a.offer_record_move(&root, &bo, Some(&preview.hash))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn account_records_and_mailbox_only_contacts_are_outside_move_limits() {
    let (a, _b, ao, bo) = pair().await;
    assert!(a.preview_record_move(&ao, &bo).await.is_err());
    let root = record(&a, "direct-required").await;
    a.act(
        Action::SetContactDelivery {
            target: bo.clone(),
            mode: "mailbox".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(a.preview_record_move(&root, &bo).await.is_err());
}

#[tokio::test]
async fn a_destination_receipt_survives_restart_before_source_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("destination.db").display());
    let a = Engine::open_memory().await.unwrap();
    let b = Engine::open(&url).await.unwrap();
    support::karma::authorize(&a).await;
    support::karma::authorize(&b).await;
    let ao = store::organs::local(&a.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bo = store::organs::local(&b.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    a.adopt_introduction(&b.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    store::organs::set_trust(&a.store.pool, &bo, "known")
        .await
        .unwrap();
    store::organs::set_trust(&b.store.pool, &ao, "known")
        .await
        .unwrap();
    a.adopt_roster(&b.roster_of(&bo).await.unwrap().unwrap())
        .await
        .unwrap();
    b.adopt_roster(&a.roster_of(&ao).await.unwrap().unwrap())
        .await
        .unwrap();
    store::organs::set_sync_policy(&a.store.pool, &bo, true, true)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, &ao, true, true)
        .await
        .unwrap();
    let root = record(&a, "restart").await;
    let (uid, preview, bundle) = offer(&a, &root, &bo).await;
    accept(&a, &b, &ao, &uid, &preview).await;
    let receipt = b
        .receive_move_bundle(&ao, &uid, bundle.clone())
        .await
        .unwrap();
    b.store.pool.close().await;
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(
        reopened
            .receive_move_offer(&ao, &uid, preview)
            .await
            .unwrap(),
        "received"
    );
    assert_eq!(
        reopened
            .receive_move_bundle(&ao, &uid, bundle)
            .await
            .unwrap(),
        receipt
    );
    a.finish_record_move(&uid, &receipt).await.unwrap();
    assert!(
        store::records::get(&reopened.store.pool, &root)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::records::get(&a.store.pool, &root)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn real_peers_reconnect_missing_device_lists_and_finish_accepted_moves() {
    use engine::{
        trust::Signer,
        wire::{Reach, Wire},
    };
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let a = Arc::new(Engine::open_memory().await.unwrap());
    let b = Arc::new(Engine::open_memory().await.unwrap());
    let ao = store::organs::local(&a.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bo = store::organs::local(&b.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    for (e, o, name) in [(&a, &ao, "source"), (&b, &bo, "destination")] {
        let cell = store::cells::local(&e.store.pool).await.unwrap().unwrap();
        let signer = Signer::generate(o, &engine::roster::cell_key_id(&cell.uid));
        e.set_signer(signer.clone()).await.unwrap();
        e.set_organ_signer(signer).await.unwrap();
        e.set_root_key_path(dir.path().join(format!("{name}.root")));
    }
    let aw = Arc::new(
        Wire::bind(
            a.clone(),
            iroh::SecretKey::from_bytes(&[122; 32]),
            Reach::Local,
        )
        .await
        .unwrap(),
    );
    let bw = Arc::new(
        Wire::bind(
            b.clone(),
            iroh::SecretKey::from_bytes(&[123; 32]),
            Reach::Local,
        )
        .await
        .unwrap(),
    );
    aw.serve_enrolment();
    bw.serve_enrolment();
    a.create_organ_identity().await.unwrap();
    b.create_organ_identity().await.unwrap();
    store::records::set_extension(
        &b.store.pool,
        &bo,
        "lince.discovery",
        &serde_json::json!({"accept_unknown":true}),
    )
    .await
    .unwrap();
    let aserve = {
        let w = aw.clone();
        tokio::spawn(async move { w.serve().await })
    };
    let bserve = {
        let w = bw.clone();
        tokio::spawn(async move { w.serve().await })
    };
    let mut invite = bw.pairing_invite().await.unwrap();
    let port = bw
        .endpoint()
        .bound_sockets()
        .into_iter()
        .next()
        .unwrap()
        .port();
    invite.addrs = vec![format!("127.0.0.1:{port}")];
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        aw.pair_with(&invite, "Recipient").await.unwrap();
        store::organs::set_trust(&b.store.pool, &ao, "known")
            .await
            .unwrap();
        a.adopt_roster(&b.roster_of(&bo).await.unwrap().unwrap())
            .await
            .unwrap();
        b.adopt_roster(&a.roster_of(&ao).await.unwrap().unwrap())
            .await
            .unwrap();
        store::organs::set_sync_policy(&a.store.pool, &bo, true, true)
            .await
            .unwrap();
        store::organs::set_sync_policy(&b.store.pool, &ao, true, true)
            .await
            .unwrap();
        store::sqlx::query("DELETE FROM organ_roster WHERE organ_uid=?")
            .bind(&bo)
            .execute(&a.store.pool)
            .await
            .unwrap();
        a.act(Action::ReconnectContact { target: bo.clone() }, None)
            .await
            .unwrap();
        assert!(a.roster_of(&bo).await.unwrap().is_some());
        a.act(
            Action::SetContactShare {
                target: bo.clone(),
                protein: Some(
                    serde_json::json!({"source":"record","where":[{"slug_eq":"unselected"}]}),
                ),
            },
            None,
        )
        .await
        .unwrap();
        let root = record(&a, "network-move").await;
        let (uid, _, _) = offer(&a, &root, &bo).await;
        store::organs::set_sync_policy(&a.store.pool, &bo, false, true)
            .await
            .unwrap();
        aw.sync_once().await.unwrap();
        assert!(offers::get(&b.store.pool, &uid).await.unwrap().is_none());
        store::organs::set_sync_policy(&a.store.pool, &bo, true, true)
            .await
            .unwrap();
        let root_key = b.root_signer().await.unwrap().unwrap();
        let cells = b.roster_of(&bo).await.unwrap().unwrap().roster.cells;
        let mut revoked = cells.clone();
        for cell in &mut revoked {
            cell.capabilities
                .retain(|cap| cap != engine::roster::CAP_WRITE);
        }
        b.publish_roster(&root_key, revoked).await.unwrap();
        aw.sync_once().await.unwrap();
        assert!(offers::get(&b.store.pool, &uid).await.unwrap().is_none());
        assert!(
            store::records::get(&a.store.pool, &root)
                .await
                .unwrap()
                .is_some()
        );
        b.publish_roster(&root_key, cells).await.unwrap();
        aw.sync_once().await.unwrap();
        assert!(offers::get(&b.store.pool, &uid).await.unwrap().is_some());
        assert!(
            store::records::get(&a.store.pool, &root)
                .await
                .unwrap()
                .is_some()
        );
        b.answer_record_move(&uid, true).await.unwrap();
        aw.sync_once().await.unwrap();
        assert_eq!(
            offers::get(&a.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .state,
            "complete"
        );
        assert!(
            store::records::get(&b.store.pool, &root)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            store::records::get(&a.store.pool, &root)
                .await
                .unwrap()
                .is_none()
        );
    })
    .await
    .unwrap();
    aserve.abort();
    bserve.abort();
    aw.shutdown().await;
    bw.shutdown().await;
}
