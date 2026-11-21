use chrono::DateTime;
use store::{
    sqlx,
    transfers::{
        OccurrenceDisputeCommit, OccurrenceDisputeInput, occurrence, occurrence_dispute_events,
        set_occurrence_dispute,
    },
};

mod transfer_occurrence_fixture;

#[tokio::test]
async fn participant_disputes_replay_and_fall_back_to_the_system_dispute()
-> Result<(), Box<dyn std::error::Error>> {
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    let now = DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")?.to_utc();

    let decide = |actor: &str, key: &str, disputed| {
        set_occurrence_dispute(
            pool,
            OccurrenceDisputeInput {
                occurrence_uid: "occurrence".into(),
                idempotency_key: key.into(),
                actor_person_uid: actor.into(),
                disputed,
                authorization_intent_uid: None,
            },
            now,
            |_| Some("signature".into()),
        )
    };
    assert!(matches!(
        decide("outsider", "outsider-asserts", true).await,
        Err(sqlx::Error::Protocol(_))
    ));

    let OccurrenceDisputeCommit::Committed(giver) = decide("giver", "giver-asserts", true).await?
    else {
        panic!("the first decision should be committed");
    };
    let OccurrenceDisputeCommit::Replayed(replay) =
        decide("giver", " giver-asserts ", true).await?
    else {
        panic!("the matching request should replay");
    };
    assert_eq!(replay.event.uid, giver.event.uid);
    assert!(matches!(
        decide("giver", "giver-asserts", false).await,
        Err(sqlx::Error::Protocol(_))
    ));

    decide("receiver", "receiver-asserts", true).await?;
    decide("receiver", "receiver-clears", false).await?;
    let row = occurrence(pool, "occurrence").await?.expect("occurrence");
    assert!(row.disputed);
    assert_eq!(
        row.dispute_fact_uid.as_deref(),
        Some(giver.fact.uid.as_str())
    );

    sqlx::query(
        "UPDATE transfer_occurrence
         SET system_disputed = 1, system_dispute_fact_uid = 'activation-fact',
             system_disputed_at = '2026-01-01T00:00:00Z' WHERE uid = 'occurrence'",
    )
    .execute(pool)
    .await?;
    decide("giver", "giver-clears", false).await?;
    let row = occurrence(pool, "occurrence").await?.expect("occurrence");
    assert!(row.disputed);
    assert_eq!(row.dispute_fact_uid.as_deref(), Some("activation-fact"));
    assert_eq!(row.disputed_at.as_deref(), Some("2026-01-01T00:00:00Z"));

    sqlx::query("UPDATE transfer_occurrence SET system_disputed = 0 WHERE uid = 'occurrence'")
        .execute(pool)
        .await?;
    decide("giver", "giver-clears-again", false).await?;
    let row = occurrence(pool, "occurrence").await?.expect("occurrence");
    assert!(!row.disputed);
    assert!(row.dispute_fact_uid.is_none());
    assert!(row.disputed_at.is_none());
    assert_eq!(
        occurrence_dispute_events(pool, "occurrence").await?.len(),
        5
    );
    Ok(())
}
