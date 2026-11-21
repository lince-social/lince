use std::time::Duration;

use chrono::DateTime;
use nucleus::transfer::{
    TransferDependencyScope, TransferDependencyUpstreamKind, TransferRevisionEvidence,
};
use store::{
    Store, StoreError,
    sqlx::{self, Error},
    transfers::{
        DraftPromise, DraftPromiseRevisionInput, NewTransferDraft, PromiseRevisionInput,
        RevisionCommit, TransferDependencyInput, TransferDraftTermsInput, WholeDraftRevisionInput,
        create_draft, dependencies_of, get, publish_whole_draft, revise_promise,
        revise_whole_draft, revision_fact, revision_for_request,
    },
};
use tokio::time::timeout;

async fn fixture() -> Result<(Store, String), StoreError> {
    let store = Store::open_memory().await?;
    let organ: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(&store.pool)
        .await?;
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at) VALUES
             ('creator', 'person', ?1, 'fixture', 'fixture'),
             ('invitee', 'person', ?1, 'fixture', 'fixture'),
             ('stock', 'plain', ?1, 'fixture', 'fixture')",
    )
    .bind(&organ)
    .execute(&store.pool)
    .await?;
    Ok((store, organ))
}

fn draft(key: &str, organ: &str) -> NewTransferDraft {
    NewTransferDraft {
        public_history: None,
        idempotency_key: key.into(),
        slug: None,
        head: key.into(),
        agreement_type: "full".into(),
        agreement_pct: None,
        satiation: None,
        parent_uid: None,
        source_uid: None,
        visibility: "hidden".into(),
        max_proximity: None,
        reserve_default: "proposed".into(),
        require_confirmation: false,
        default_place: None,
        creator_person: "creator".into(),
        invitees: Vec::new(),
        promises: Vec::new(),
        dependencies: Vec::new(),
        organ_uid: Some(organ.into()),
        evidence_action: "create-transfer-draft".into(),
        correction: None,
        authorization_intent_uid: None,
    }
}

fn promise() -> DraftPromise {
    DraftPromise {
        uid: Some("promise".into()),
        record_uid: Some("stock".into()),
        concept_uid: None,
        item: None,
        unit_uid: None,
        person_uid: Some("creator".into()),
        open: false,
        delta: -1.0,
        window_start: None,
        window_end: None,
        location: None,
        condition: None,
        reserve_from: "proposed".into(),
        open_reuse_policy: Default::default(),
    }
}

fn edit(transfer: &str, key: &str) -> WholeDraftRevisionInput {
    WholeDraftRevisionInput {
        transfer_uid: transfer.into(),
        expected_revision: 1,
        idempotency_key: key.into(),
        creator_person_uid: "creator".into(),
        proposal_author_person_uid: "creator".into(),
        terms: TransferDraftTermsInput {
            public_history: None,
            slug: None,
            head: "Revised terms".into(),
            agreement_type: "full".into(),
            agreement_pct: None,
            settlement: "individual".into(),
            visibility: "hidden".into(),
            max_proximity: None,
            satiation: None,
            parent_uid: None,
            source_uid: None,
            reserve_default: "proposed".into(),
            require_confirmation: false,
            default_place: None,
        },
        retained_invitation_uids: Vec::new(),
        promises: Vec::new(),
        dependencies: Vec::new(),
        successor: None,
        authorization_intent_uid: None,
    }
}

fn dependency(upstream: &str) -> TransferDependencyInput {
    TransferDependencyInput {
        uid: None,
        scope: TransferDependencyScope::Transfer,
        promise_uid: None,
        upstream_kind: TransferDependencyUpstreamKind::Transfer,
        upstream_uid: upstream.into(),
        required_state: "settled".into(),
    }
}

#[tokio::test]
async fn draft_and_revision_keep_signed_evidence_and_roll_back_invalid_dependencies()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, organ) = fixture().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let sign = |_: &str| Some("signature".into());
    let upstream = create_draft(pool, draft("upstream", &organ), now, None, None, sign).await?;
    let input = NewTransferDraft {
        invitees: vec!["invitee".into()],
        ..draft("draft", &organ)
    };
    assert!(matches!(
        create_draft(pool, input.clone(), now, None, None, |_| None).await,
        Err(Error::Protocol(message)) if message.contains("Transfer draft requires")
    ));
    let created = create_draft(pool, input.clone(), now, None, None, sign).await?;
    let replay = create_draft(pool, input, now, None, None, |_| None).await?;
    assert!(replay.replayed);
    assert_eq!(replay.fact, created.fact);
    assert_eq!(
        replay.invitation_event_facts,
        created.invitation_event_facts
    );
    assert_eq!(created.fact.delta.to_f64(), 1.0);
    let [invitation_fact] = created.invitation_event_facts.as_slice() else {
        panic!("the draft should have one addressed invitation");
    };
    assert_eq!(invitation_fact.prev_hash, created.fact.hash);

    let mut revision = edit(&created.transfer_uid, "edit");
    revision.retained_invitation_uids = created.invitation_uids;
    revision
        .dependencies
        .push(dependency(&upstream.transfer_uid));
    revision.promises.push(DraftPromiseRevisionInput {
        uid: None,
        item: None,
        source_promise_uid: None,
        record_uid: Some("stock".into()),
        concept_uid: None,
        unit_uid: None,
        person_uid: Some("creator".into()),
        open: false,
        delta: -3.0,
        window_start: None,
        window_end: None,
        location: None,
        condition: None,
        reserve_from: "proposed".into(),
        open_reuse_policy: Default::default(),
    });
    let unsigned = revise_whole_draft(pool, revision.clone(), now, None, |_| None).await;
    assert!(
        matches!(&unsigned, Err(Error::Protocol(message)) if message.contains("Transfer revision requires")),
        "{unsigned:?}"
    );
    assert_eq!(
        get(pool, &created.transfer_uid)
            .await?
            .expect("draft")
            .revision,
        1
    );
    assert!(
        dependencies_of(pool, &created.transfer_uid)
            .await?
            .is_empty()
    );

    let RevisionCommit::Committed {
        revision: 2, fact, ..
    } = revise_whole_draft(pool, revision.clone(), now, None, sign).await?
    else {
        panic!("signed draft revision should commit");
    };
    assert_eq!(fact.prev_hash, invitation_fact.hash);
    assert!(fact.delta.is_zero());
    let signed: TransferRevisionEvidence =
        serde_json::from_str(fact.payload.as_deref().expect("signed revision evidence"))?;
    assert_eq!(signed.previous_revision, Some(1));
    assert_eq!(signed.terms.revision, 2);
    let [invitation] = signed.terms.invitations.as_slice() else {
        panic!("the revision should retain the invitation");
    };
    let [signed_dependency] = signed.terms.dependencies.as_slice() else {
        panic!("the revision should include the dependency");
    };
    let [promise] = signed.terms.promises.as_slice() else {
        panic!("the revision should include the new promise");
    };
    assert_eq!(invitation.status, "pending");
    assert_eq!(signed_dependency.required_state, "kept");
    assert_eq!(promise.delta, -3.0);
    assert_eq!(
        revision_fact(pool, &created.transfer_uid, 2).await?,
        Some(fact.clone())
    );
    assert!(matches!(
        revise_whole_draft(pool, revision.clone(), now, None, |_| None).await?,
        RevisionCommit::Replayed { revision: 2, fact: replay } if replay == fact
    ));
    assert!(matches!(
        publish_whole_draft(pool, revision, now, None, sign).await,
        Err(Error::Protocol(message)) if message.contains("different revision action")
    ));

    let mut cyclic = edit(&upstream.transfer_uid, "cycle");
    cyclic.dependencies.push(dependency(&created.transfer_uid));
    assert!(matches!(
        revise_whole_draft(pool, cyclic, now, None, sign).await,
        Err(Error::Protocol(message)) if message.contains("dependency cycle")
    ));
    assert_eq!(
        get(pool, &upstream.transfer_uid)
            .await?
            .expect("upstream")
            .revision,
        1
    );
    assert!(revision_for_request(pool, "cycle").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn single_promise_revision_and_replay_work_with_one_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, organ) = fixture().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let input = NewTransferDraft {
        promises: vec![promise()],
        ..draft("draft", &organ)
    };
    let created = create_draft(pool, input, now, None, None, |_| Some("signature".into())).await?;
    let edit = PromiseRevisionInput {
        transfer_uid: created.transfer_uid,
        promise_uid: "promise".into(),
        expected_revision: 1,
        idempotency_key: "promise-edit".into(),
        item: None,
        record_uid: "stock".into(),
        person_uid: "creator".into(),
        delta: -2.0,
        window_end: None,
        condition: None,
        reserve_from: "proposed".into(),
    };
    let RevisionCommit::Committed {
        revision: 2, fact, ..
    } = timeout(
        Duration::from_secs(5),
        revise_promise(pool, edit.clone(), now, None, |_| None),
    )
    .await??
    else {
        panic!("the single promise should be revised");
    };
    assert!(fact.signature.is_none());
    assert_eq!(fact.prev_hash, created.fact.hash);
    let signed: TransferRevisionEvidence =
        serde_json::from_str(fact.payload.as_deref().expect("promise revision evidence"))?;
    let [promise] = signed.terms.promises.as_slice() else {
        panic!("the revision should include the edited promise");
    };
    assert_eq!(promise.delta, -2.0);
    let replay = timeout(
        Duration::from_secs(5),
        revise_promise(pool, edit, now, None, |_| None),
    )
    .await??;
    assert!(matches!(replay, RevisionCommit::Replayed { revision: 2, fact: old } if old == fact));
    Ok(())
}
