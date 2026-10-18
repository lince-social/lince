use super::*;
use nucleus::trust_analysis::custody::forwarding::SourceMode;
use store::evidence_forwarding as outbox;

async fn enabled() -> Fixture {
    let f = fixture(Store::open_memory().await.unwrap()).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut state = custody::policy_on(&mut tx, &f.owner).await.unwrap();
    state.policy.forwarding.enabled = true;
    state.policy.forwarding.peers = state.policy.readers.clone();
    custody::set_policy_on(
        &mut tx,
        &f.owner,
        &state.policy,
        1,
        f.now,
        |_| Some("audit fixture".into()),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    f
}

async fn queue(f: &Fixture, index: u32, control: bool, series_index: u32) -> (String, Offer) {
    let mut offer = f.offer.clone();
    std::mem::swap(&mut offer.sender, &mut offer.custodian);
    offer.nonce = nucleus::new_uid("cust");
    offer.artifact = nucleus::karma::canonical_hash("lince.test.queued.original.v1", &index).unwrap();
    offer.deliver_by = f.now + 100;
    let series = nucleus::karma::canonical_hash("lince.test.queued.series.v1", &series_index).unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let record = outbox::queue_on(
        &mut tx,
        outbox::Queue {
            owner: &f.owner,
            source: &f.owner,
            mode: SourceMode::Serving,
            policy_version: 2,
            offer: &offer,
            endpoint: "pinned endpoint",
            series: &series,
            control,
            now: f.now,
        },
        |_| Some("audit fixture".into()),
    )
    .await
    .unwrap()
    .unwrap();
    tx.commit().await.unwrap();
    (record, offer)
}

#[tokio::test]
async fn forwarding_queue_prioritizes_controls_caps_eight_jobs_and_never_retries_more_than_three_times()
 {
    let f = enabled().await;
    let mut jobs = Vec::new();
    for index in 0..8 {
        jobs.push(queue(&f, index, false, index).await);
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut ninth = jobs[0].1.clone();
    ninth.nonce = nucleus::new_uid("cust");
    ninth.artifact = nucleus::karma::canonical_hash("lince.test.queued.original.v1", &9).unwrap();
    let series = nucleus::karma::canonical_hash("lince.test.queued.series.v1", &9).unwrap();
    assert!(
        outbox::queue_on(
            &mut tx,
            outbox::Queue {
                owner: &f.owner,
                source: &f.owner,
                mode: SourceMode::Serving,
                policy_version: 2,
                offer: &ninth,
                endpoint: "pinned endpoint",
                series: &series,
                control: false,
                now: f.now
            },
            |_| Some("audit fixture".into())
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    let (control, offer) = queue(&f, 10, true, 0).await;
    let mut tx = f.store.pool.begin().await.unwrap();
    assert_eq!(
        outbox::get_on(&mut tx, &jobs[0].0).await.unwrap().state,
        "cancelled"
    );
    assert_eq!(
        outbox::pending_on(&mut tx, f.now).await.unwrap()[0],
        control
    );
    tx.rollback().await.unwrap();
    for (attempt, elapsed) in [(1, 0), (2, 5), (3, 15)] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            outbox::claim_on(&mut tx, &control, &offer.hash().unwrap(), f.now + elapsed)
                .await
                .unwrap()
        );
        assert!(
            !outbox::claim_on(&mut tx, &control, &offer.hash().unwrap(), f.now + elapsed)
                .await
                .unwrap()
        );
        assert_eq!(
            outbox::get_on(&mut tx, &control).await.unwrap().attempts,
            attempt
        );
        assert!(
            outbox::result_on(
                &mut tx,
                &control,
                &offer.hash().unwrap(),
                Err("disconnected"),
                f.now + elapsed
            )
            .await
            .unwrap()
        );
        tx.commit().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert_eq!(
        outbox::get_on(&mut tx, &control).await.unwrap().state,
        "failed"
    );
    assert!(
        !outbox::claim_on(&mut tx, &control, &offer.hash().unwrap(), f.now + 30)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn forwarding_restart_keeps_attempt_count_and_cancelled_commit_races_keep_the_true_acknowledgment()
 {
    let f = enabled().await;
    let (record, offer) = queue(&f, 1, false, 1).await;
    let hash = offer.hash().unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        outbox::claim_on(&mut tx, &record, &hash, f.now)
            .await
            .unwrap()
    );
    outbox::interrupted_on(&mut tx, f.now).await.unwrap();
    let restarted = outbox::get_on(&mut tx, &record).await.unwrap();
    assert_eq!(restarted.state, "pending");
    assert_eq!(restarted.attempts, 1);
    assert!(
        outbox::claim_on(&mut tx, &record, &hash, f.now)
            .await
            .unwrap()
    );
    outbox::cancel_on(&mut tx, &record, &f.owner, &hash)
        .await
        .unwrap();
    let ack = acknowledgment(&offer, f.now);
    let mut wrong = ack.clone();
    wrong.artifact = nucleus::karma::canonical_hash("lince.test.wrong.ack.v1", &1).unwrap();
    assert!(
        outbox::result_on(&mut tx, &record, &hash, Ok(&wrong), f.now)
            .await
            .is_err()
    );
    assert!(
        outbox::result_on(&mut tx, &record, &hash, Ok(&ack), f.now)
            .await
            .unwrap()
    );
    let saved = outbox::get_on(&mut tx, &record).await.unwrap();
    assert_eq!(saved.state, "stored");
    assert_eq!(saved.acknowledgment, Some(ack));
    assert!(saved.error.unwrap().contains("after cancellation"));
    tx.commit().await.unwrap();
    let (interrupted, offer) = queue(&f, 2, false, 2).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    for _ in 0..3 {
        assert!(
            outbox::claim_on(&mut tx, &interrupted, &offer.hash().unwrap(), f.now)
                .await
                .unwrap()
        );
        outbox::interrupted_on(&mut tx, f.now).await.unwrap();
    }
    assert_eq!(
        outbox::get_on(&mut tx, &interrupted).await.unwrap().state,
        "failed"
    );
    assert!(
        !outbox::claim_on(&mut tx, &interrupted, &offer.hash().unwrap(), f.now)
            .await
            .unwrap()
    );
}
