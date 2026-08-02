//! A Frequency is a slug and a step. What has to hold is that the slug stays
//! the one way to reach it, that a retry does not declare a second one, and
//! that the beats it implies are computed rather than stored.

use chrono::{DateTime, Utc};
use nucleus::karma::CadenceStep;
use store::Store;
use store::frequency::{NewFrequency, all, create, delete, resolve};

async fn store() -> Store {
    let path =
        std::env::temp_dir().join(format!("lince-frequency-{}.db", nucleus::new_uid("test")));
    Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap()
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn daily() -> CadenceStep {
    CadenceStep {
        days: 1,
        ..Default::default()
    }
}

fn new<'a>(slug: &'a str, every: CadenceStep, request_id: &'a str) -> NewFrequency<'a> {
    NewFrequency {
        slug,
        head: "",
        every,
        anchor_at: at("2026-08-01T07:00:00Z"),
        request_id,
        actor_uid: None,
    }
}

#[tokio::test]
async fn a_frequency_is_reached_by_the_name_a_condition_writes() {
    let store = store().await;
    let now = Utc::now();
    let made = create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();

    // `freq(@daily)` writes the slug with an @; resolving must not care.
    for token in ["daily", "@daily"] {
        let found = resolve(&store.pool, token).await.unwrap().unwrap();
        assert_eq!(found.uid, made.uid, "resolving {token}");
    }
    // A head nobody supplied falls back to the slug rather than being blank.
    assert_eq!(made.head, "daily");
}

#[tokio::test]
async fn declaring_the_same_frequency_twice_declares_one() {
    let store = store().await;
    let now = Utc::now();
    let first = create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();
    let retry = create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();
    assert_eq!(first.uid, retry.uid, "a retry returns the first one");
    assert_eq!(all(&store.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn two_frequencies_cannot_answer_to_one_name() {
    let store = store().await;
    let now = Utc::now();
    create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();
    // A different declaration under the same name: a reading that could mean
    // two beats is not a reading.
    let clash = create(
        &store.pool,
        new(
            "daily",
            CadenceStep {
                weeks: 1,
                ..Default::default()
            },
            "req-2",
        ),
        now,
    )
    .await;
    assert!(clash.is_err(), "the second declaration is refused");
}

#[tokio::test]
async fn a_beat_that_never_comes_is_refused() {
    let store = store().await;
    let now = Utc::now();
    let empty = create(
        &store.pool,
        new("never", CadenceStep::default(), "req-1"),
        now,
    )
    .await;
    assert!(
        empty.is_err(),
        "a step advancing nothing is not a frequency"
    );

    // The slug is typed into an expression, so it may not hold anything the
    // lexer would read as an operator.
    let operator = create(&store.pool, new("a+b", daily(), "req-2"), now).await;
    assert!(operator.is_err(), "a name an expression cannot spell");
}

#[tokio::test]
async fn the_beats_are_computed_from_the_step_not_stored() {
    let store = store().await;
    let now = Utc::now();
    let made = create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();

    // Nothing wrote a beat anywhere; the cadence enumerates them on demand,
    // which is the same call that draws a calendar and fires a rule.
    let beats = made
        .cadence()
        .between(
            made.anchor().unwrap(),
            at("2026-08-01T00:00:00Z"),
            at("2026-08-05T00:00:00Z"),
        )
        .unwrap();
    assert_eq!(beats.len(), 4, "four daily beats in four days");
}

#[tokio::test]
async fn a_frequency_a_rule_still_reads_cannot_be_forgotten() {
    let store = store().await;
    let now = Utc::now();
    let made = create(&store.pool, new("daily", daily(), "req-1"), now)
        .await
        .unwrap();

    // A rule whose condition names the beat. Forgetting it underneath would
    // leave the condition reading nothing, which fires exactly like a reading
    // that is merely false — the rule would quietly stop and say nothing.
    let record = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: Some("rent"),
            kind: nucleus::RecordKind::Plain,
            head: "Rent",
            body: "",
            quantity: nucleus::DecimalValue::from_mantissa(0, 0).unwrap(),
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO recurrence (uid, record_uid, consequences_json, condition_src, gate, carry,
                                 cadence_json, anchor_at, state, revision, created_at, updated_at)
         VALUES ('rule-1', ?, '[]', '-1 * freq(@daily)', '!=0', 'value', '{}', ?, 'active', 1, ?, ?)",
    )
    .bind(&record.uid)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&store.pool)
    .await
    .unwrap();

    let refused = delete(&store.pool, &made.uid).await;
    assert!(
        refused.is_err(),
        "a frequency a rule reads is held in place"
    );
    assert_eq!(
        all(&store.pool).await.unwrap().len(),
        1,
        "it is still there"
    );

    // Once nothing reads it, it goes.
    sqlx::query("DELETE FROM recurrence WHERE uid = 'rule-1'")
        .execute(&store.pool)
        .await
        .unwrap();
    delete(&store.pool, &made.uid).await.unwrap();
    assert!(all(&store.pool).await.unwrap().is_empty());
}
