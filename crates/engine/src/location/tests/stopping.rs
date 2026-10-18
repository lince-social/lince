use super::*;

async fn transfer(f: &Fixture) -> String {
    let transfer = store::transfers::create(
        &f.engine.store.pool,
        store::transfers::NewTransfer {
            slug: None,
            head: "Independent journey lifecycle",
            agreement_type: "all",
            agreement_pct: None,
            satiation: None,
            source_uid: None,
            reserve_default: None,
            require_confirmation: false,
        },
    )
    .await
    .unwrap();
    store::sqlx::query("UPDATE record SET quantity_mantissa='1' WHERE uid=?")
        .bind(&transfer)
        .execute(&f.engine.store.pool)
        .await
        .unwrap();
    transfer
}

async fn unrelated(f: &Fixture) -> String {
    let record = f
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Independent manual sharing".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut settings = f.settings.clone();
    settings.record_uid = record.clone();
    settings.source_kind = SourceKind::Manual;
    for command in [
        Command::Configure { settings },
        Command::Start {
            person: f.controller.clone(),
            record_uid: record.clone(),
        },
    ] {
        f.engine.location_request(command, None).await.unwrap();
    }
    f.engine
        .location_request(
            Command::Approve {
                person: f.controller.clone(),
                record_uid: record.clone(),
                expected_session_uid: approval_epoch(&f.engine, &record).await,
            },
            None,
        )
        .await
        .unwrap();
    record
}

#[tokio::test]
async fn terminal_paths_invalidate_old_epochs_and_only_end_their_own_sharing_scope() {
    for reason in [
        "manual",
        "expiry",
        "lease-loss",
        "permission",
        "device-revoked",
        "capture-loss",
        "transfer-ended",
        "participant-finish",
    ] {
        let f = Fixture::new().await;
        let transfer = transfer(&f).await;
        let mut settings = f.settings.clone();
        settings.transfer_uid = Some(transfer.clone());
        let lease = f.start(settings.clone()).await;
        f.engine
            .location_accept_fix(
                &lease.settings.source_node_id,
                &f.record,
                &lease.session_uid,
                f.fix(1),
            )
            .await
            .unwrap();
        let other = unrelated(&f).await;
        match reason {
            "manual" => {
                f.engine
                    .location_request(
                        Command::Stop {
                            person: f.controller.clone(),
                            record_uid: f.record.clone(),
                        },
                        None,
                    )
                    .await
                    .unwrap();
            }
            "expiry" => {
                f.engine
                    .location
                    .state
                    .lock()
                    .await
                    .sessions
                    .get_mut(&f.record)
                    .unwrap()
                    .deadline = Instant::now() - Duration::from_secs(1);
                f.engine.location_tick().await.unwrap();
            }
            "lease-loss" => {
                let mut state = f.engine.location.state.lock().await;
                state
                    .sessions
                    .get_mut(&f.record)
                    .unwrap()
                    .challenges
                    .back_mut()
                    .unwrap()
                    .1 = Instant::now() - Duration::from_secs(31);
                state.sources.get_mut(&f.record).unwrap().renewed =
                    Instant::now() - Duration::from_secs(31);
                drop(state);
                f.engine.location_tick().await.unwrap();
            }
            "permission" => {
                let role = store::auth::ensure_role(&f.engine.store.pool, "revoked-location-write")
                    .await
                    .unwrap();
                store::auth::set_user_role(&f.engine.store.pool, &f.controller, role)
                    .await
                    .unwrap();
                f.engine.location_tick().await.unwrap();
            }
            "device-revoked" => {
                let mut connection = f.engine.store.pool.acquire().await.unwrap();
                let credential = store::session_access::password_on(&mut connection, "controller")
                    .await
                    .unwrap()
                    .unwrap();
                store::session_access::register_device_on(
                    &mut connection,
                    credential.authentication(),
                    &lease.settings.source_node_id,
                )
                .await
                .unwrap();
                drop(connection);
                store::sqlx::query("UPDATE person_device SET revoked=1,revision=revision+1 WHERE person_uid=? AND node_id=?").bind(&f.controller).bind(&lease.settings.source_node_id).execute(&f.engine.store.pool).await.unwrap();
                f.engine.location_tick().await.unwrap();
            }
            "capture-loss" => f.engine.location_device_unavailable().await,
            "transfer-ended" => {
                store::sqlx::query("UPDATE record SET quantity_mantissa='0' WHERE uid=?")
                    .bind(&transfer)
                    .execute(&f.engine.store.pool)
                    .await
                    .unwrap();
                f.engine
                    .location_stop_ended_transfer(&transfer)
                    .await
                    .unwrap();
            }
            "participant-finish" => {
                assert!(
                    f.engine
                        .stop_transfer_location(&f.controller, &transfer, Some(&f.stranger))
                        .await
                        .is_err()
                );
                assert!(
                    f.engine
                        .location_view(&f.controller, &f.record)
                        .await
                        .unwrap()
                        .fix
                        .is_some()
                );
                f.engine
                    .stop_transfer_location(&f.controller, &transfer, Some(&f.controller))
                    .await
                    .unwrap();
                assert!(
                    !store::location::transfer_ended(&f.engine.store.pool, &transfer)
                        .await
                        .unwrap()
                );
            }
            _ => unreachable!(),
        }
        assert!(
            f.engine
                .location_view(&f.controller, &f.record)
                .await
                .unwrap()
                .fix
                .is_none(),
            "{reason}"
        );
        assert!(
            !f.engine
                .location_sources()
                .await
                .iter()
                .any(|source| source.settings.record_uid == f.record),
            "{reason}"
        );
        assert!(
            f.engine
                .location_accept_challenged_fix(
                    &lease.settings.source_node_id,
                    &f.record,
                    &lease.session_uid,
                    &lease.challenge_uid,
                    f.fix(2)
                )
                .await
                .is_err(),
            "{reason}"
        );
        assert!(
            f.engine
                .location_approve(
                    &f.record,
                    &lease.session_uid,
                    &f.controller,
                    &lease.settings.source_node_id
                )
                .await
                .is_err(),
            "{reason}"
        );
        f.engine
            .location_stop_record_if(&f.record, Some(&lease.session_uid))
            .await;
        f.engine
            .location_stop_record_if(&f.record, Some(&lease.session_uid))
            .await;
        if !matches!(reason, "permission" | "device-revoked") {
            assert!(
                f.engine
                    .location_sources()
                    .await
                    .iter()
                    .any(|source| source.settings.record_uid == other),
                "{reason}"
            );
        }
        assert_eq!(
            store::location::settings(&f.engine.store.pool, &f.record)
                .await
                .unwrap(),
            Some(settings)
        );
        let restarted = Engine::new(f.engine.store.clone()).await.unwrap();
        restarted.attach_location_network(f._network.clone());
        assert!(restarted.location_sources().await.is_empty());
        assert!(
            restarted
                .location_view(&f.controller, &f.record)
                .await
                .unwrap()
                .fix
                .is_none()
        );
    }
}

#[tokio::test]
async fn delayed_source_loss_and_old_cleanup_cannot_stop_an_approved_successor() {
    let f = Fixture::new().await;
    let previous = f.start(f.settings.clone()).await;
    f.engine.location_stop_record(&f.record).await;
    f.engine
        .location_request(
            Command::Start {
                person: f.controller.clone(),
                record_uid: f.record.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        f.engine
            .location_request(
                Command::Approve {
                    person: f.controller.clone(),
                    record_uid: f.record.clone(),
                    expected_session_uid: previous.session_uid.clone()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(f.engine.location_sources().await.is_empty());
    f.engine
        .location_request(
            Command::Approve {
                person: f.controller.clone(),
                record_uid: f.record.clone(),
                expected_session_uid: approval_epoch(&f.engine, &f.record).await,
            },
            None,
        )
        .await
        .unwrap();
    let current = f.engine.location_sources().await.remove(0);
    f.engine
        .publish_device_fix_for(std::slice::from_ref(&previous), f.fix(999))
        .await
        .unwrap();
    assert!(
        f.engine
            .location_view(&f.controller, &f.record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    f.engine
        .location_device_unavailable_for(std::slice::from_ref(&previous))
        .await;
    assert_eq!(
        f.engine.location_sources().await[0].session_uid,
        current.session_uid
    );
    f.engine
        .location_accept_fix(
            &current.settings.source_node_id,
            &f.record,
            &current.session_uid,
            f.fix(1),
        )
        .await
        .unwrap();
    let organ = store::organs::local(&f.engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    f.engine
        .location_peer(
            &organ,
            &previous.settings.source_node_id,
            PeerRequest::Unavailable {
                record_uid: f.record.clone(),
                session_uid: previous.session_uid.clone(),
            },
        )
        .await
        .unwrap();
    f.engine
        .location_stop_record_if(&f.record, Some(&previous.session_uid))
        .await;
    assert!(
        f.engine
            .location_view(&f.controller, &f.record)
            .await
            .unwrap()
            .fix
            .is_some()
    );
    assert_eq!(
        f.engine.location_sources().await[0].session_uid,
        current.session_uid
    );
}
