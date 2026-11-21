use chrono::DateTime;
use nucleus::{
    execution::Execution,
    new_uid,
    transfer::{
        disclosure::TransferItem,
        exchange::ExchangeRoute,
        transport::{Activity, Direction, Event, JOURNEY_DOMAIN, Step, Terms},
    },
};
use serde_json::json;
use store::{
    Store, StoreError,
    sqlx::{self, SqlitePool},
    transport::{capacity, journey},
};

const PICKUP_END: &str = "2026-11-21T12:00:00Z";

async fn transfer(pool: &SqlitePool, source: Option<&str>) -> Result<String, StoreError> {
    let uid = new_uid("r");
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
         SELECT ?, 'transfer', uid, 'fixture', 'fixture' FROM record WHERE slug = 'local-organ'",
    )
    .bind(&uid)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO transfer (record_uid, source_uid, revision) VALUES (?, ?, 2)")
        .bind(&uid)
        .bind(source)
        .execute(pool)
        .await?;
    Ok(uid)
}

fn terms(origin: &str, destination: &str, direction: Direction) -> Terms {
    Terms {
        activity: Activity::Ride,
        direction,
        origin: origin.into(),
        destination: destination.into(),
        via: Vec::new(),
        count: 1,
        capacity: 1,
    }
}

async fn promise(
    pool: &SqlitePool,
    transfer: &str,
    uid: &str,
    state: &str,
    item: &TransferItem,
) -> Result<(), StoreError> {
    let item =
        serde_json::to_string(item).map_err(|error| StoreError::Protocol(error.to_string()))?;
    sqlx::query(
        "INSERT INTO promise
            (uid, transfer_uid, delta, state, window_end, item_json, created_at, updated_at)
         VALUES (?, ?, -1, ?, ?, ?, 'fixture', 'fixture')",
    )
    .bind(uid)
    .bind(transfer)
    .bind(state)
    .bind(PICKUP_END)
    .bind(item)
    .execute(pool)
    .await?;
    Ok(())
}

async fn participant(pool: &SqlitePool, transfer: &str, person: &str) -> Result<(), StoreError> {
    let party = new_uid("tp");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&party)
        .bind(transfer)
        .bind(person)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, 2, 'fixture', 2)",
    )
    .bind(new_uid("ta"))
    .bind(transfer)
    .bind(party)
    .execute(pool)
    .await?;
    Ok(())
}

async fn event(
    pool: &SqlitePool,
    transfer: &str,
    actor: &str,
    person: &str,
    revision: u64,
    step: Step,
) -> Result<String, StoreError> {
    let uid = new_uid("f");
    let payload = json!({
        "journey": Event {
            domain: JOURNEY_DOMAIN.into(),
            transfer: transfer.into(),
            revision,
            person: person.into(),
            step,
            request_id: uid.clone(),
        },
    });
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, actor_uid, cause_kind, payload, prev_hash, hash)
         VALUES (?1, ?2, '0', 0, ?3, ?4, 'user_edit', ?5, '', ?1)",
    )
    .bind(&uid)
    .bind(transfer)
    .bind(PICKUP_END)
    .bind(actor)
    .bind(payload.to_string())
    .execute(pool)
    .await?;
    Ok(uid)
}

fn item(terms: Terms, exchange: &str, driver: &str, passenger: &str) -> TransferItem {
    TransferItem {
        exchange: Some(ExchangeRoute {
            uid: exchange.into(),
            giver: driver.into(),
            receiver: passenger.into(),
        }),
        transport: Some(terms),
        ..Default::default()
    }
}

fn clock(at: &str) -> Result<Execution, Box<dyn std::error::Error>> {
    Ok(Execution::new(
        [13; 32],
        DateTime::parse_from_rfc3339(at)?.timestamp_millis(),
    )?)
}

#[tokio::test]
async fn reservations_deduplicate_exchanges_and_keep_adjacent_segments_independent()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let [a, b, c, driver, passenger] = std::array::from_fn(|_| new_uid("r"));
    let offer = transfer(pool, None).await?;
    let mut route = terms(&a, &c, Direction::Offer);
    route.via.push(b.clone());
    promise(
        pool,
        &offer,
        "offer",
        "proposed",
        &item(route, "route", &driver, &passenger),
    )
    .await?;
    let ab = transfer(pool, Some(&offer)).await?;
    let bc = transfer(pool, Some(&offer)).await?;
    let first = item(terms(&a, &b, Direction::Need), "ab", &driver, &passenger);
    promise(pool, &ab, "ab-a", "agreed", &first).await?;
    promise(pool, &ab, "ab-b", "active", &first).await?;
    promise(
        pool,
        &bc,
        "bc",
        "agreed",
        &item(terms(&b, &c, Direction::Need), "bc", &driver, &passenger),
    )
    .await?;
    participant(pool, &ab, &driver).await?;
    participant(pool, &ab, &passenger).await?;

    let clock = clock(PICKUP_END)?;
    let projection = clock
        .scope(capacity(pool, &offer))
        .await?
        .expect("offer capacity");
    let reservations: Vec<_> = projection
        .segments
        .iter()
        .map(|segment| segment.reserved)
        .collect();
    assert_eq!(reservations, [1, 1]);
    let part = clock
        .scope(capacity(pool, &bc))
        .await?
        .expect("booking capacity");
    let [segment] = part.segments.as_slice() else {
        panic!("the B to C booking covers one segment");
    };
    assert_eq!(
        (&segment.origin, &segment.destination, part.available),
        (&b, &c, 0)
    );

    let overlap = transfer(pool, Some(&offer)).await?;
    promise(
        pool,
        &overlap,
        "overlap",
        "agreed",
        &item(
            terms(&a, &c, Direction::Need),
            "overlap",
            &driver,
            &passenger,
        ),
    )
    .await?;
    assert!(matches!(
        clock.scope(capacity(pool, &offer)).await,
        Err(StoreError::Protocol(message)) if message.contains("capacity would be exceeded")
    ));
    sqlx::query("UPDATE promise SET state = 'withdrawn' WHERE uid = 'overlap'")
        .execute(pool)
        .await?;

    event(pool, &ab, &passenger, &passenger, 2, Step::Finished).await?;
    assert!(
        !clock
            .scope(journey(pool, &ab))
            .await?
            .expect("active journey")
            .all_finished
    );
    let released = clock
        .scope(capacity(pool, &offer))
        .await?
        .expect("offer capacity");
    let available: Vec<_> = released
        .segments
        .iter()
        .map(|segment| segment.available)
        .collect();
    assert_eq!(available, [1, 0]);
    Ok(())
}

#[tokio::test]
async fn journey_keeps_current_attributed_progress_after_the_pickup_cutoff()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let [a, b, driver, rider] = std::array::from_fn(|_| new_uid("r"));
    let booking = transfer(pool, None).await?;
    promise(
        pool,
        &booking,
        "booking",
        "agreed",
        &item(terms(&a, &b, Direction::Need), "ride", &driver, &rider),
    )
    .await?;
    participant(pool, &booking, &driver).await?;
    participant(pool, &booking, &rider).await?;
    event(pool, &booking, &rider, &rider, 1, Step::Departed).await?;
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, payload, prev_hash, hash)
         VALUES ('legacy', ?, '0', 0, 'fixture', 'user_edit', 'not JSON', '', 'legacy')",
    )
    .bind(&booking)
    .execute(pool)
    .await?;

    let before = clock("2026-11-21T12:14:59.999Z")?;
    let cutoff = clock("2026-11-21T12:15:00Z")?;
    assert!(
        !before
            .scope(journey(pool, &booking))
            .await?
            .expect("journey before cutoff")
            .capacity_released
    );
    let expired = cutoff
        .scope(journey(pool, &booking))
        .await?
        .expect("unstarted journey");
    assert!(expired.agreed && expired.expired && expired.capacity_released && !expired.started);

    let departed = event(pool, &booking, &driver, &driver, 2, Step::Departed).await?;
    event(pool, &booking, &driver, &driver, 2, Step::Boarded).await?;
    let started = cutoff
        .scope(journey(pool, &booking))
        .await?
        .expect("started journey");
    assert!(started.started && !started.expired && !started.capacity_released);
    let progress = started
        .participants
        .iter()
        .find(|progress| progress.person == driver)
        .expect("driver progress");
    assert_eq!(
        (progress.step, progress.fact.as_deref()),
        (Some(Step::Departed), Some(departed.as_str()))
    );

    event(pool, &booking, &driver, &rider, 2, Step::Finished).await?;
    assert!(matches!(
        cutoff.scope(journey(pool, &booking)).await,
        Err(StoreError::Protocol(message)) if message.contains("attribution differs")
    ));
    Ok(())
}
