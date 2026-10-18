mod support;

use engine::{
    Engine,
    actions::{Action, TransferReservePoint, TransferSatiation, TransferVisibility},
};
use nucleus::transfer::{
    AgreementType,
    disclosure::TransferItem,
    exchange::ExchangeRoute,
    transport::{Activity, Direction, Step, Terms},
};
use support::{Person, TransferFixture};

async fn draft(
    engine: &Engine,
    driver: &Person,
    passenger: &Person,
    endpoints: &[String; 2],
    source: Option<String>,
    name: &str,
) -> TransferFixture {
    engine.set_signer(driver.signer.clone()).await.unwrap();
    let mut promise = support::promise(name, "", driver, -1.0);
    promise.reserve_from = Some(TransferReservePoint::Agreed);
    promise.window_start = Some("2026-10-17T12:00:00Z".into());
    promise.window_end = Some("2026-10-17T13:00:00Z".into());
    promise.item = Some(TransferItem {
        title: "Ride seat".into(),
        exchange: Some(ExchangeRoute {
            uid: format!("route-{name}"),
            giver: driver.uid.clone(),
            receiver: passenger.uid.clone(),
        }),
        transport: Some(Terms {
            activity: Activity::Ride,
            direction: Direction::Offer,
            origin: endpoints[0].clone(),
            destination: endpoints[1].clone(),
            count: 1,
            capacity: 2,
        }),
        ..Default::default()
    });
    let action = Action::CreateTransferDraft {
        public_history: None,
        request_id: format!("draft-{name}"),
        creator: Some(driver.uid.clone()),
        slug: Some(name.into()),
        head: "Ride".into(),
        agreement: AgreementType::Full,
        agreement_pct: None,
        satiation: TransferSatiation::None,
        parent: None,
        source,
        visibility: TransferVisibility::Hidden,
        max_proximity: None,
        reserve_default: TransferReservePoint::Agreed,
        require_confirmation: true,
        default_place: None,
        invitees: vec![passenger.uid.clone()],
        promises: vec![promise],
        dependencies: vec![],
    };
    let transfer = engine
        .act(action.clone(), None)
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        engine.act(action, None).await.unwrap().created.as_deref(),
        Some(transfer.as_str())
    );
    engine.set_signer(passenger.signer.clone()).await.unwrap();
    let invitation = store::transfers::invitations_for_transfer(&engine.store.pool, &transfer)
        .await
        .unwrap()
        .into_iter()
        .find(|invitation| invitation.addressed_person_uid == passenger.uid)
        .unwrap();
    let revision = store::transfers::get(&engine.store.pool, &transfer)
        .await
        .unwrap()
        .unwrap()
        .revision as u64;
    engine
        .act(
            Action::AcceptTransferInvitation {
                transfer: Some(transfer.clone()),
                invitation: invitation.uid,
                person: Some(passenger.uid.clone()),
                expected_revision: revision,
                request_id: format!("accept-{name}"),
            },
            None,
        )
        .await
        .unwrap();
    let revision = store::transfers::get(&engine.store.pool, &transfer)
        .await
        .unwrap()
        .unwrap()
        .revision as u64;
    TransferFixture { transfer, revision }
}

async fn level(
    engine: &Engine,
    booking: &TransferFixture,
    person: &Person,
    value: u8,
    id: &str,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    engine
        .act(
            Action::SetTransferAgreementLevel {
                transfer: booking.transfer.clone(),
                expected_revision: booking.revision,
                request_id: id.into(),
                person: Some(person.uid.clone()),
                level: value,
            },
            None,
        )
        .await
}

#[test]
fn competing_bookings_reserve_once_and_signed_journey_progress_releases_before_settlement() {
    support::run_async_test("transport-capacity", || async {
        let clock = nucleus::execution::Execution::new(
            [81; 32],
            chrono::DateTime::parse_from_rfc3339("2026-10-17T12:30:00Z")
                .unwrap()
                .timestamp_millis(),
        )
        .unwrap();
        clock
            .scope(async {
                let engine = support::engine().await;
                let driver = support::person(&engine, "driver").await;
                let first = support::person(&engine, "first").await;
                let second = support::person(&engine, "second").await;
                let outsider = support::person(&engine, "observer").await;
                let mut endpoints = [String::new(), String::new()];
                for (index, endpoint) in endpoints.iter_mut().enumerate() {
                    *endpoint = support::plain(&engine, &format!("address-{index}"), 0.0).await;
                    engine
                        .act(
                            Action::SetPlace {
                                target: endpoint.clone(),
                                lat: -23.5 + index as f64 / 10.0,
                                lon: -46.6,
                                address: None,
                            },
                            None,
                        )
                        .await
                        .unwrap();
                }
                let offer =
                    draft(&engine, &driver, &first, &endpoints, None, "capacity-offer").await;
                let mut bookings = Vec::new();
                for (index, passenger) in [&first, &second, &first].into_iter().enumerate() {
                    let booking = draft(
                        &engine,
                        &driver,
                        passenger,
                        &endpoints,
                        Some(offer.transfer.clone()),
                        &format!("booking-{index}"),
                    )
                    .await;
                    for person in [&driver, passenger] {
                        engine.set_signer(person.signer.clone()).await.unwrap();
                        level(
                            &engine,
                            &booking,
                            person,
                            1,
                            &format!("review-{index}-{}", person.uid),
                        )
                        .await
                        .unwrap();
                    }
                    engine.set_signer(passenger.signer.clone()).await.unwrap();
                    level(
                        &engine,
                        &booking,
                        passenger,
                        2,
                        &format!("accept-seat-{index}"),
                    )
                    .await
                    .unwrap();
                    bookings.push(booking);
                }
                engine.set_signer(driver.signer.clone()).await.unwrap();
                level(&engine, &bookings[0], &driver, 2, "first-seat")
                    .await
                    .unwrap();
                let (left, right) = tokio::join!(
                    level(&engine, &bookings[1], &driver, 2, "last-seat-left"),
                    level(&engine, &bookings[2], &driver, 2, "last-seat-right")
                );
                assert_ne!(left.is_ok(), right.is_ok());
                let capacity = store::transport::capacity(&engine.store.pool, &offer.transfer)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    (capacity.total, capacity.reserved, capacity.available),
                    (2, 2, 0)
                );
                level(&engine, &bookings[0], &driver, 2, "first-seat")
                    .await
                    .unwrap();
                let before: i64 =
                    store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
                        .fetch_one(&engine.store.pool)
                        .await
                        .unwrap();
                let action = |step, id: &str| Action::RecordJourneyProgress {
                    transfer: bookings[0].transfer.clone(),
                    expected_revision: bookings[0].revision,
                    person: driver.uid.clone(),
                    request_id: id.into(),
                    step,
                };
                engine.set_signer(outsider.signer.clone()).await.unwrap();
                assert!(
                    engine
                        .act(action(Step::Departed, "forged-progress"), None)
                        .await
                        .is_err()
                );
                engine.set_signer(driver.signer.clone()).await.unwrap();
                let departed = engine
                    .act(action(Step::Departed, "depart"), None)
                    .await
                    .unwrap();
                assert_eq!(
                    engine
                        .act(action(Step::Departed, "depart"), None)
                        .await
                        .unwrap()
                        .created,
                    departed.created
                );
                let finished = engine
                    .act(action(Step::Finished, "finish"), None)
                    .await
                    .unwrap();
                assert_eq!(
                    engine
                        .act(action(Step::Finished, "finish"), None)
                        .await
                        .unwrap()
                        .created,
                    finished.created
                );
                assert!(
                    engine
                        .act(action(Step::Boarded, "old-step"), None)
                        .await
                        .is_err()
                );
                assert_eq!(
                    store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer_occurrence")
                        .fetch_one(&engine.store.pool)
                        .await
                        .unwrap(),
                    before
                );
                let capacity = store::transport::capacity(&engine.store.pool, &offer.transfer)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!((capacity.reserved, capacity.available), (1, 1));
                assert!(engine.location_sources().await.is_empty());
                let started = &bookings[if left.is_ok() { 1 } else { 2 }];
                engine
                    .act(
                        Action::RecordJourneyProgress {
                            transfer: started.transfer.clone(),
                            expected_revision: started.revision,
                            person: driver.uid.clone(),
                            request_id: "other-departed".into(),
                            step: Step::Departed,
                        },
                        None,
                    )
                    .await
                    .unwrap();
                clock
                    .set_time(
                        chrono::DateTime::parse_from_rfc3339("2026-10-17T13:15:00.001Z")
                            .unwrap()
                            .timestamp_millis(),
                    )
                    .unwrap();
                assert!(
                    !store::transport::journey(&engine.store.pool, &started.transfer)
                        .await
                        .unwrap()
                        .unwrap()
                        .expired
                );
                let unstarted = &bookings[if left.is_ok() { 2 } else { 1 }];
                assert!(
                    store::transport::journey(&engine.store.pool, &unstarted.transfer)
                        .await
                        .unwrap()
                        .unwrap()
                        .expired
                );
                assert_eq!(
                    store::transport::capacity(&engine.store.pool, &offer.transfer)
                        .await
                        .unwrap()
                        .unwrap()
                        .reserved,
                    1
                );
            })
            .await;
    });
}
