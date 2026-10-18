use super::*;

#[tokio::test]
async fn authority_renewal_cannot_grant_local_approval_change_scope_or_refresh_an_expired_reply() {
    let fixture = Fixture::new().await;
    let mut lease = fixture.start(fixture.settings.clone()).await;
    lease.approved = false;
    let mut source = Source {
        lease: lease.clone(),
        renewed: Instant::now(),
    };
    let mut approval = lease.clone();
    approval.approved = true;
    assert!(
        source
            .renew(approval.clone(), Instant::now(), false)
            .is_err()
    );
    assert!(!source.lease.approved);
    source
        .renew(approval.clone(), Instant::now(), true)
        .unwrap();
    let mut widened = approval.clone();
    widened.settings.recipients.push(fixture.stranger.clone());
    assert!(source.renew(widened, Instant::now(), false).is_err());
    let accepted = source.renewed;
    source.renew(lease.clone(), Instant::now(), false).unwrap();
    assert!(source.lease.approved);
    assert_eq!(source.renewed, accepted);
    source
        .renew(
            approval.clone(),
            Instant::now() - Duration::from_secs(31),
            false,
        )
        .unwrap();
    assert_eq!(source.renewed, accepted);
    let mut pending = Source {
        lease: lease.clone(),
        renewed: Instant::now(),
    };
    assert!(
        pending
            .renew(approval, Instant::now() - Duration::from_secs(31), true)
            .is_err()
    );
    assert!(!pending.lease.approved);
    assert_eq!(source.lease.settings, lease.settings);
}

#[tokio::test]
async fn challenge_bound_capture_age_survives_clock_offsets_but_rejects_replay_and_old_epochs() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    fixture
        .engine
        .location
        .state
        .lock()
        .await
        .sessions
        .get_mut(&fixture.record)
        .unwrap()
        .approved_at = Some(Instant::now() - Duration::from_secs(10));
    for (index, offset) in [-86_400_000, 0, 86_400_000].into_iter().enumerate() {
        let mut fix = fixture.fix(index as u64 + 1);
        fix.captured_at_ms += offset;
        fix.capture_age_ms = 5_000;
        let clock =
            nucleus::execution::Execution::new([index as u8 + 1; 32], now_ms() + offset).unwrap();
        clock
            .scope(fixture.engine.location_accept_challenged_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                &lease.challenge_uid,
                fix,
            ))
            .await
            .unwrap();
        let view = fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap();
        assert!(view.fix.is_some());
        assert!(view.age_ms.unwrap() >= 5_000 && view.age_ms.unwrap() < 15_000);
        let delayed: View = serde_json::from_value(age_response(
            serde_json::to_value(view).unwrap(),
            Duration::from_secs(61),
        ))
        .unwrap();
        assert!(delayed.fix.is_none());
        assert_eq!(delayed.status, Status::Unavailable);
    }
    for (session, sequence) in [(&lease.session_uid, 1), (&nucleus::new_uid("location"), 4)] {
        assert!(
            fixture
                .engine
                .location_accept_challenged_fix(
                    &lease.settings.source_node_id,
                    &fixture.record,
                    session,
                    &lease.challenge_uid,
                    fixture.fix(sequence)
                )
                .await
                .is_err()
        );
    }
    fixture
        .engine
        .location
        .state
        .lock()
        .await
        .sessions
        .get_mut(&fixture.record)
        .unwrap()
        .challenges
        .iter_mut()
        .for_each(|(_, issued)| *issued = Instant::now() - Duration::from_secs(31));
    assert!(
        fixture
            .engine
            .location_accept_challenged_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                &lease.challenge_uid,
                fixture.fix(4)
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn suspended_or_disconnected_leases_stop_without_restoring_capture_or_approval() {
    for elapsed in [31, 90] {
        let fixture = Fixture::new().await;
        let lease = fixture.start(fixture.settings.clone()).await;
        let mut fix = fixture.fix(1);
        fix.accuracy_metres = Some(15.0);
        fixture
            .engine
            .location_accept_challenged_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                &lease.challenge_uid,
                fix,
            )
            .await
            .unwrap();
        let exact = fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap();
        assert_eq!(exact.fix.as_ref().unwrap().accuracy_metres, Some(15.0));
        assert!(exact.radius_metres.is_none());
        let area = fixture
            .engine
            .location_permitted_view(&fixture.record, &lease.session_uid, true, false, Some(1000))
            .await
            .unwrap();
        assert_eq!(area.radius_metres, Some(1000));
        assert!(area.fix.as_ref().unwrap().accuracy_metres.is_none());
        {
            let mut state = fixture.engine.location.state.lock().await;
            let session = state.sessions.get_mut(&fixture.record).unwrap();
            for (_, issued) in &mut session.challenges {
                *issued = Instant::now() - Duration::from_secs(elapsed);
            }
            session.latest.as_mut().unwrap().received =
                Instant::now() - Duration::from_secs(elapsed);
            state.sources.get_mut(&fixture.record).unwrap().renewed =
                Instant::now() - Duration::from_secs(elapsed);
        }
        fixture.engine.location_tick().await.unwrap();
        assert!(fixture.engine.location_sources().await.is_empty());
        assert!(fixture.engine.location_pending_sources().await.is_empty());
        assert_eq!(
            fixture
                .engine
                .location_view(&fixture.controller, &fixture.record)
                .await
                .unwrap()
                .status,
            Status::Stopped
        );
        assert!(
            fixture
                .engine
                .location_accept_challenged_fix(
                    &lease.settings.source_node_id,
                    &fixture.record,
                    &lease.session_uid,
                    &lease.challenge_uid,
                    fixture.fix(2)
                )
                .await
                .is_err()
        );
        assert!(
            store::location::settings(&fixture.engine.store.pool, &fixture.record)
                .await
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test]
async fn challenges_are_bounded_and_reading_a_view_cannot_renew_them() {
    let fixture = Fixture::new().await;
    let lease = fixture.start(fixture.settings.clone()).await;
    let current = {
        let mut state = fixture.engine.location.state.lock().await;
        let session = state.sessions.get_mut(&fixture.record).unwrap();
        for _ in 0..12 {
            session.renew();
        }
        assert_eq!(session.challenges.len(), 8);
        session.lease.clone()
    };
    assert!(
        fixture
            .engine
            .location_accept_challenged_fix(
                &lease.settings.source_node_id,
                &fixture.record,
                &lease.session_uid,
                &lease.challenge_uid,
                fixture.fix(1)
            )
            .await
            .is_err()
    );
    fixture
        .engine
        .location_accept_challenged_fix(
            &lease.settings.source_node_id,
            &fixture.record,
            &lease.session_uid,
            &current.challenge_uid,
            fixture.fix(1),
        )
        .await
        .unwrap();
    let before = fixture.engine.location.state.lock().await.sessions[&fixture.record]
        .challenges
        .clone();
    for _ in 0..3 {
        fixture
            .engine
            .location_view(&fixture.controller, &fixture.record)
            .await
            .unwrap();
    }
    let after = fixture.engine.location.state.lock().await.sessions[&fixture.record]
        .challenges
        .clone();
    assert!(before == after);
}
