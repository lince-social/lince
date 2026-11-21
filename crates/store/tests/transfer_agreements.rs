use chrono::{DateTime, Utc};
use nucleus::{
    PromiseState,
    transfer::{OpenPromiseReusePolicy, disclosure::TransferItem},
};
use store::{
    Store, StoreError,
    sqlx::{self, Error},
    transfers::{
        AgreementTransitionCommit, AgreementTransitionInput, CreatedTransferDraft, DraftPromise,
        InvitationCommit, InvitationTransitionInput, NewTransferDraft, OpenPromiseClaimCommit,
        OpenPromiseClaimInput, accept_transfer_invitation, agreement_coalition, agreement_events,
        agreement_readiness_input, claim_open_promise, create_draft, get, item_of, list_all,
        open_claim_pairs_for_source, promises_of, publication_input, transition_agreement,
    },
};

fn sign(_: &str) -> Option<String> {
    Some("signature".into())
}

async fn draft(
    store: &Store,
    now: DateTime<Utc>,
    percentage: Option<i64>,
    open_policy: Option<OpenPromiseReusePolicy>,
) -> Result<CreatedTransferDraft, StoreError> {
    let pool = &store.pool;
    let organ: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at) VALUES
             ('giver', 'person', ?1, 'fixture', 'fixture'),
             ('receiver', 'person', ?1, 'fixture', 'fixture'),
             ('stock', 'plain', ?1, 'fixture', 'fixture')",
    )
    .bind(&organ)
    .execute(pool)
    .await?;
    create_draft(
        pool,
        NewTransferDraft {
            public_history: None,
            idempotency_key: "create".into(),
            slug: None,
            head: "Shared stock".into(),
            agreement_type: if percentage.is_some() {
                "percentage"
            } else {
                "full"
            }
            .into(),
            agreement_pct: percentage,
            satiation: None,
            parent_uid: None,
            source_uid: None,
            visibility: "hidden".into(),
            max_proximity: None,
            reserve_default: "proposed".into(),
            require_confirmation: true,
            default_place: None,
            creator_person: "giver".into(),
            invitees: vec!["receiver".into()],
            promises: vec![DraftPromise {
                uid: Some("offer".into()),
                item: open_policy.map(|_| TransferItem {
                    title: "Stock".into(),
                    ..Default::default()
                }),
                record_uid: Some("stock".into()),
                concept_uid: None,
                unit_uid: None,
                person_uid: Some("giver".into()),
                open: open_policy.is_some(),
                delta: -1.0,
                window_start: None,
                window_end: None,
                location: None,
                condition: None,
                reserve_from: "proposed".into(),
                open_reuse_policy: open_policy.unwrap_or_default(),
            }],
            dependencies: Vec::new(),
            organ_uid: Some(organ),
            evidence_action: "create-transfer-draft".into(),
            correction: None,
            authorization_intent_uid: None,
        },
        now,
        None,
        None,
        sign,
    )
    .await
}

async fn accept(
    store: &Store,
    draft: &CreatedTransferDraft,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let [invitation] = draft.invitation_uids.as_slice() else {
        panic!("the fixture should have one invitation");
    };
    let result = accept_transfer_invitation(
        &store.pool,
        InvitationTransitionInput {
            invitation_uid: invitation.clone(),
            expected_revision: 1,
            idempotency_key: "accept".into(),
            actor_person_uid: Some("receiver".into()),
            expires_at: None,
        },
        now,
        sign,
    )
    .await?;
    assert!(matches!(result, InvitationCommit::Committed(outcome) if outcome.revision == Some(2)));
    Ok(())
}

#[tokio::test]
async fn open_claim_policies_keep_pair_lineage_and_exact_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    for policy in [
        OpenPromiseReusePolicy::Consume,
        OpenPromiseReusePolicy::Duplicate,
    ] {
        let store = Store::open_memory().await?;
        let pool = &store.pool;
        let draft = draft(&store, now, None, Some(policy)).await?;
        let mut claim = OpenPromiseClaimInput {
            transfer_uid: draft.transfer_uid.clone(),
            source_promise_uid: "offer".into(),
            expected_revision: 1,
            idempotency_key: "claim".into(),
            claimant_person_uid: "receiver".into(),
            record_uid: Some("stock".into()),
            concept_uid: None,
            unit_uid: None,
            delta: 1.0,
            window_start: None,
            window_end: None,
            location: None,
            condition: None,
            reserve_from: "proposed".into(),
            authorization_intent_uid: None,
        };
        assert!(matches!(
            claim_open_promise(pool, claim.clone(), now, sign).await,
            Err(Error::Protocol(message)) if message.contains("must be accepted")
        ));
        accept(&store, &draft, now).await?;
        claim.expected_revision = 2;
        let OpenPromiseClaimCommit::Committed(committed) =
            claim_open_promise(pool, claim.clone(), now, sign).await?
        else {
            panic!("the accepted party's claim should commit");
        };
        assert_eq!(committed.revision, 3);
        assert_eq!(
            committed.consumed,
            policy == OpenPromiseReusePolicy::Consume
        );
        assert_eq!(
            committed.proposer_promise_uid == "offer",
            committed.consumed
        );
        let pair = open_claim_pairs_for_source(pool, "offer").await?;
        let [pair] = pair.as_slice() else {
            panic!("the OPEN source should have one claim pair");
        };
        assert_eq!(pair.claimant_promise_uid, committed.promise_uid);
        let source = promises_of(pool, &draft.transfer_uid)
            .await?
            .into_iter()
            .find(|promise| promise.uid == "offer")
            .expect("source promise");
        assert_eq!(
            source.state,
            if committed.consumed {
                PromiseState::Proposed
            } else {
                PromiseState::Open
            }
        );
        let proposer = item_of(pool, &committed.proposer_promise_uid)
            .await?
            .expect("proposer item");
        let claimant = item_of(pool, &committed.promise_uid)
            .await?
            .expect("claimant item");
        assert_eq!(proposer.exchange, claimant.exchange);
        let route = claimant.exchange.expect("signed exchange route");
        assert_eq!(
            (route.giver.as_str(), route.receiver.as_str()),
            ("giver", "receiver")
        );
        assert!(matches!(
            claim_open_promise(pool, claim.clone(), now, |_| None).await?,
            OpenPromiseClaimCommit::Replayed(old) if old.fact == committed.fact
        ));
        claim.reserve_from = "agreed".into();
        assert!(matches!(
            claim_open_promise(pool, claim, now, sign).await,
            Err(Error::Protocol(message)) if message.contains("different refined terms")
        ));
    }
    Ok(())
}

#[tokio::test]
async fn percentage_agreement_freezes_signed_members_and_preserves_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let draft = draft(&store, now, Some(50), None).await?;
    accept(&store, &draft, now).await?;
    let transfer = &draft.transfer_uid;
    let change = |person: &str, level, key: &str| AgreementTransitionInput {
        transfer_uid: transfer.clone(),
        expected_revision: 2,
        idempotency_key: key.into(),
        person_uid: person.into(),
        to_level: level,
        authorization_intent_uid: None,
    };
    let published = publication_input(pool, transfer, 2, "giver", "publish").await?;
    assert_eq!(published.creator_person_uid, "giver");
    assert_eq!(published.terms.visibility, "public");
    let row = get(pool, transfer).await?.expect("Transfer");
    let listed = list_all(pool)
        .await?
        .into_iter()
        .find(|row| row.transfer.record_uid == *transfer)
        .expect("listed Transfer");
    assert_eq!(
        row.require_confirmation,
        listed.transfer.require_confirmation
    );
    assert_eq!(row.revision, listed.transfer.revision);
    assert_eq!(row.active, listed.transfer.active);

    assert!(matches!(
        transition_agreement(pool, change("giver", 2, "skip-level"), now, sign).await,
        Err(Error::Protocol(message)) if message.contains("adjacent level")
    ));
    transition_agreement(pool, change("giver", 1, "giver-review"), now, sign).await?;
    let commit = change("giver", 2, "giver-commit");
    assert!(matches!(
        transition_agreement(pool, commit.clone(), now, |_| None).await,
        Err(Error::Protocol(message)) if message.contains("requires a signing key")
    ));
    assert!(agreement_coalition(pool, transfer, 2).await?.is_none());
    assert_eq!(agreement_events(pool, transfer, Some(2)).await?.len(), 1);
    let AgreementTransitionCommit::Committed(committed) =
        transition_agreement(pool, commit.clone(), now, sign).await?
    else {
        panic!("the signed commitment should succeed");
    };
    let frozen = committed
        .coalition
        .expect("one of two parties reaches 50 percent");
    assert_eq!(frozen.party_uids, [committed.event.party_uid]);
    assert_eq!(frozen.eligible_count, 2);
    assert_eq!(frozen.threshold_pct, 50);
    assert!(matches!(
        transition_agreement(pool, change("receiver", 1, "late-review"), now, sign).await,
        Err(Error::Protocol(message)) if message.contains("frozen without this party")
    ));
    assert!(matches!(
        transition_agreement(pool, commit, now, |_| None).await?,
        AgreementTransitionCommit::Replayed(old) if old.fact == committed.fact
            && old.coalition.is_some()
    ));
    let readiness = agreement_readiness_input(pool, transfer).await?;
    let [promise] = readiness.promises.as_slice() else {
        panic!("the source promise should remain in the agreement");
    };
    assert_eq!(promise.state, "agreed");
    transition_agreement(pool, change("giver", 1, "withdraw-commitment"), now, sign).await?;
    let promises = promises_of(pool, transfer).await?;
    let [promise] = promises.as_slice() else {
        panic!("the source promise should remain after withdrawal");
    };
    assert_eq!(promise.state, PromiseState::Proposed);
    let still_frozen = agreement_coalition(pool, transfer, 2)
        .await?
        .expect("frozen membership");
    assert_eq!(still_frozen.frozen_by_event_uid, frozen.frozen_by_event_uid);
    Ok(())
}
