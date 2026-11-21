use chrono::{DateTime, TimeDelta};
use store::{
    Store, StoreError,
    sqlx::{Error, raw_sql},
    transfers::{
        AddressTransferInvitationInput, InvitationCommit, InvitationCommitOutcome,
        InvitationTransitionInput, NewTransferInvitation,
        TransferInvitationStatus::{Accepted, Expired, Pending, Rejected, Withdrawn},
        accept_invitation, accept_transfer_invitation, address_transfer_invitation,
        create_invitation, due_transfer_invitations, expire_invitation, expire_transfer_invitation,
        invitation, invitation_events, pending_invitations_for_person, reject_transfer_invitation,
        reopen_transfer_invitation, withdraw_transfer_invitation,
    },
};

async fn fixture() -> Result<Store, StoreError> {
    let store = Store::open_memory().await?;
    raw_sql(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at) VALUES
             ('transfer', 'transfer', 'organ', 'fixture', 'fixture'),
             ('creator', 'person', 'organ', 'fixture', 'fixture'),
             ('invitee', 'person', 'organ', 'fixture', 'fixture'),
             ('outsider', 'person', 'organ', 'fixture', 'fixture');
         INSERT INTO transfer (record_uid, revision) VALUES ('transfer', 1);
         INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
             VALUES ('creator-party', 'transfer', 'creator', 'creator');
         INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, prev_hash, hash)
             VALUES ('revision-fact', 'transfer', '0', 0, '2026-01-01T00:00:00Z', 'user_edit', '', 'fixture');
         INSERT INTO transfer_revision (transfer_uid, revision, fact_uid, idempotency_key, created_at)
             VALUES ('transfer', 1, 'revision-fact', 'existing-revision', 'fixture');",
    )
    .execute(&store.pool)
    .await?;
    Ok(store)
}

fn committed(commit: InvitationCommit) -> InvitationCommitOutcome {
    let InvitationCommit::Committed(outcome) = commit else {
        panic!("the invitation transition should commit");
    };
    outcome
}

#[tokio::test]
async fn signed_invitation_attempts_preserve_revisions_and_historical_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let store = fixture().await?;
    let pool = &store.pool;
    let start = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let at = |step| start + TimeDelta::seconds(step);
    let sign = |_: &str| Some("signature".into());
    let mut input = AddressTransferInvitationInput {
        transfer_uid: "transfer".into(),
        expected_revision: 1,
        idempotency_key: "existing-revision".into(),
        addressed_person_uid: "invitee".into(),
        invited_by_person_uid: "creator".into(),
        expires_at: Some(at(1)),
    };
    assert!(matches!(
        address_transfer_invitation(pool, input.clone(), at(0), sign).await,
        Err(Error::Protocol(message)) if message.contains("already used by another action")
    ));
    input.idempotency_key = "address".into();
    let addressed = committed(address_transfer_invitation(pool, input, at(0), sign).await?);
    assert_eq!(addressed.revision, Some(2));
    assert_eq!(addressed.invitation.status, Pending);
    assert_eq!(
        addressed.event_fact.prev_hash,
        addressed.revision_fact.expect("address revision").hash
    );
    let uid = &addressed.invitation.uid;
    let transition = |key: &str, revision, actor: Option<&str>| InvitationTransitionInput {
        invitation_uid: uid.clone(),
        expected_revision: revision,
        idempotency_key: key.into(),
        actor_person_uid: actor.map(str::to_owned),
        expires_at: None,
    };
    assert!(due_transfer_invitations(pool, at(0)).await?.is_empty());
    assert_eq!(due_transfer_invitations(pool, at(1)).await?.len(), 1);
    assert!(matches!(
        accept_transfer_invitation(pool, transition("late-accept", 2, Some("invitee")), at(1), sign).await,
        Err(Error::Protocol(message)) if message.contains("expired invitation")
    ));
    let expired = committed(
        expire_transfer_invitation(pool, transition("expire", 2, None), at(1), sign).await?,
    );
    assert_eq!(expired.invitation.status, Expired);
    assert_eq!(expired.revision, None);

    let reopened = committed(
        reopen_transfer_invitation(pool, transition("reopen", 2, Some("creator")), at(2), sign)
            .await?,
    );
    assert_eq!(reopened.invitation.attempt, 2);
    assert_eq!(reopened.revision, Some(3));
    assert!(matches!(
        reject_transfer_invitation(pool, transition("wrong-actor", 3, Some("outsider")), at(3), sign)
            .await,
        Err(Error::Protocol(message)) if message.contains("addressed Person")
    ));
    let reject = transition("reject", 3, Some("invitee"));
    let rejected = committed(reject_transfer_invitation(pool, reject.clone(), at(3), sign).await?);
    assert_eq!(rejected.invitation.status, Rejected);
    assert_eq!(rejected.revision, None);

    committed(
        reopen_transfer_invitation(
            pool,
            transition("reopen-again", 3, Some("creator")),
            at(4),
            sign,
        )
        .await?,
    );
    let withdrawn = committed(
        withdraw_transfer_invitation(
            pool,
            transition("withdraw", 4, Some("creator")),
            at(5),
            sign,
        )
        .await?,
    );
    assert_eq!(withdrawn.invitation.status, Withdrawn);
    assert_eq!(withdrawn.revision, Some(5));
    assert!(matches!(
        reopen_transfer_invitation(pool, transition("stale", 4, Some("creator")), at(6), sign)
            .await?,
        InvitationCommit::Stale {
            current_revision: 5,
            ..
        }
    ));
    committed(
        reopen_transfer_invitation(
            pool,
            transition("final-reopen", 5, Some("creator")),
            at(6),
            sign,
        )
        .await?,
    );
    assert_eq!(
        pending_invitations_for_person(pool, "invitee").await?.len(),
        1
    );
    let accepted = committed(
        accept_transfer_invitation(pool, transition("accept", 6, Some("invitee")), at(7), sign)
            .await?,
    );
    assert_eq!(accepted.revision, Some(7));
    assert_eq!(accepted.invitation.status, Accepted);
    assert_eq!(accepted.invitation.attempt, 4);
    assert_eq!(accepted.party_uid, accepted.invitation.party_uid);
    assert!(accepted.party_uid.is_some());
    assert!(
        pending_invitations_for_person(pool, "invitee")
            .await?
            .is_empty()
    );

    let InvitationCommit::Replayed(old) =
        reject_transfer_invitation(pool, reject, at(8), |_| None).await?
    else {
        panic!("the original rejection should replay");
    };
    assert_eq!(old.event_fact, rejected.event_fact);
    assert_eq!(old.invitation.status, Rejected);
    assert_eq!(old.invitation.attempt, 2);
    assert!(old.party_uid.is_none());
    let current = invitation(pool, uid).await?.expect("current invitation");
    assert_eq!(current.status, Accepted);
    assert_eq!(current.attempt, 4);
    let events = invitation_events(pool, uid).await?;
    assert!(events.iter().map(|event| event.kind.as_str()).eq([
        "addressed",
        "expired",
        "reopened",
        "rejected",
        "reopened",
        "withdrawn",
        "reopened",
        "accepted",
    ]));
    Ok(())
}

#[tokio::test]
async fn legacy_invitation_updates_keep_their_unsigned_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let store = fixture().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();
    let pending = create_invitation(
        pool,
        NewTransferInvitation {
            transfer_uid: "transfer",
            addressed_person_uid: "invitee",
            invited_by_person_uid: "creator",
            expires_at: None,
        },
        now,
    )
    .await?;
    let accepted = accept_invitation(pool, &pending.uid, now)
        .await?
        .expect("pending legacy invitation should accept");
    assert_eq!(accepted.invitation.status, Accepted);
    assert_eq!(
        accepted.invitation.party_uid.as_deref(),
        Some(accepted.party_uid.as_str())
    );
    assert!(expire_invitation(pool, &pending.uid, now).await?.is_none());
    assert!(invitation_events(pool, &pending.uid).await?.is_empty());
    let transfer = store::transfers::get(pool, "transfer")
        .await?
        .expect("Transfer");
    assert_eq!(transfer.revision, 1);
    Ok(())
}
