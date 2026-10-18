use super::*;
use nucleus::visibility::{Command as VisibilityCommand, Condition, Data, Policy};

async fn prepare(f: &Fixture, radius: Option<u32>) -> Invitation {
    let response = f
        .engine
        .location_request(
            Command::PrepareInvitation {
                person: f.controller.clone(),
                record_uid: f.record.clone(),
                recipient: f.recipient.clone(),
                duration_seconds: 3600,
                radius_metres: radius,
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let invitation: Invitation = serde_json::from_value(response["invitation"].clone()).unwrap();
    assert_eq!(
        Invitation::parse(response["reference"].as_str().unwrap()).unwrap(),
        invitation
    );
    invitation
}

async fn observe(f: &Fixture, person: &str, invitation: Invitation) -> Result<View, EngineError> {
    let response = f
        .engine
        .location_request(
            Command::ObserveInvitation {
                person: person.into(),
                invitation,
            },
            None,
        )
        .await?;
    Ok(serde_json::from_value(response.data.unwrap())?)
}

#[tokio::test]
async fn invitation_scope_never_grants_access_and_current_exclusions_and_precision_win() {
    let f = Fixture::new().await;
    let mut settings = f.settings.clone();
    settings.recipients = vec![f.recipient.clone()];
    let source = f.start(settings).await;
    f.engine
        .location_accept_fix(
            &source.settings.source_node_id,
            &f.record,
            &source.session_uid,
            f.fix(1),
        )
        .await
        .unwrap();
    let before: (i64, i64) =
        store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&f.engine.store.pool)
            .await
            .unwrap();
    let invitation = prepare(&f, Some(1000)).await;
    let exact = f
        .engine
        .location_view(&f.recipient, &f.record)
        .await
        .unwrap();
    let coarse = observe(&f, &f.recipient, invitation.clone()).await.unwrap();
    assert_eq!(coarse.radius_metres, Some(1000));
    assert_eq!(
        coarse.expires_at_ms,
        Some(invitation.grant.as_ref().unwrap().expires_at_ms)
    );
    assert!(coarse.fix.is_some());
    assert_ne!(
        coarse.fix.as_ref().unwrap().latitude,
        exact.fix.as_ref().unwrap().latitude
    );
    assert!(coarse.fix.as_ref().unwrap().accuracy_metres.is_none());
    assert!(coarse.session_uid.is_none() && coarse.next_sequence.is_none());
    assert!(observe(&f, &f.stranger, invitation.clone()).await.is_err());
    for field in 0..4 {
        let mut altered = invitation.clone();
        let grant = altered.grant.as_mut().unwrap();
        match field {
            0 => grant.recipient = f.stranger.clone(),
            1 => grant.radius_metres = None,
            2 => grant.expires_at_ms += 1,
            _ => grant.uid = nucleus::new_uid("li"),
        }
        let person = grant.recipient.clone();
        assert!(observe(&f, &person, altered).await.is_err());
    }
    let after: (i64, i64) =
        store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&f.engine.store.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let organ = store::organs::local(&f.engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    for (revision, exclude) in [(0, false), (1, true), (2, false)] {
        f.engine
            .visibility_request(
                VisibilityCommand::Save {
                    record_uid: f.record.clone(),
                    data: Data::LiveLocation,
                    policy: Policy {
                        include: vec![Condition {
                            organs: vec![organ.clone()],
                            persons: vec![f.recipient.clone()],
                            radius_metres: Some(2000),
                            ..Default::default()
                        }],
                        exclude: if exclude {
                            vec![Condition {
                                persons: vec![f.recipient.clone()],
                                ..Default::default()
                            }]
                        } else {
                            vec![]
                        },
                    },
                    expected_revision: revision,
                },
                None,
            )
            .await
            .unwrap();
        let view = observe(&f, &f.recipient, invitation.clone()).await.unwrap();
        if exclude {
            assert!(view.fix.is_none());
        } else {
            assert_eq!(view.radius_metres, Some(2000));
            assert!(view.fix.is_some());
        }
    }
    f.engine
        .visibility_request(
            VisibilityCommand::Save {
                record_uid: f.record.clone(),
                data: Data::LiveLocation,
                policy: Policy {
                    include: vec![Condition {
                        organs: vec![organ.clone()],
                        ..Default::default()
                    }],
                    exclude: vec![],
                },
                expected_revision: 3,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        f.engine
            .location_organ_view(&organ, &f.record)
            .await
            .unwrap()["fix"]
            .is_object()
    );
    assert!(observe(&f, &f.stranger, invitation.clone()).await.is_err());
    assert_eq!(
        observe(&f, &f.recipient, invitation.clone())
            .await
            .unwrap()
            .radius_metres,
        Some(1000)
    );
    f.engine
        .visibility_request(
            VisibilityCommand::Save {
                record_uid: f.record.clone(),
                data: Data::Record,
                policy: Policy {
                    include: vec![],
                    exclude: vec![Condition {
                        persons: vec![f.recipient.clone()],
                        ..Default::default()
                    }],
                },
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        observe(&f, &f.recipient, invitation)
            .await
            .unwrap()
            .fix
            .is_none()
    );
}

#[tokio::test]
async fn invitations_require_authenticated_recipient_and_do_not_renew_or_survive_stop() {
    let f = Fixture::new().await;
    let mut settings = f.settings.clone();
    settings.recipients = vec![f.recipient.clone()];
    let source = f.start(settings).await;
    f.engine
        .location_accept_fix(
            &source.settings.source_node_id,
            &f.record,
            &source.session_uid,
            f.fix(1),
        )
        .await
        .unwrap();
    let invitation = prepare(&f, None).await;
    let uid = invitation.grant.as_ref().unwrap().uid.clone();
    let deadline = f.engine.location.state.lock().await.invitations[&uid].deadline;
    let organ = store::organs::local(&f.engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let node = "b".repeat(64);
    let request = PeerRequest::Control {
        command: Command::ReadInvitation {
            person: f.recipient.clone(),
            invitation: invitation.clone(),
        },
    };
    assert!(
        f.engine
            .location_peer(&organ, &node, request.clone())
            .await
            .is_err()
    );
    let mut conn = f.engine.store.pool.acquire().await.unwrap();
    let credential = store::session_access::password_on(&mut conn, "recipient")
        .await
        .unwrap()
        .unwrap();
    store::session_access::register_device_on(&mut conn, credential.authentication(), &node)
        .await
        .unwrap();
    drop(conn);
    let view: View = serde_json::from_value(
        f.engine
            .location_peer(&organ, &node, request)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(view.fix.is_some());
    observe(&f, &f.recipient, invitation.clone()).await.unwrap();
    assert_eq!(
        f.engine.location.state.lock().await.invitations[&uid].deadline,
        deadline
    );
    f.engine
        .location
        .state
        .lock()
        .await
        .invitations
        .get_mut(&uid)
        .unwrap()
        .deadline = Instant::now() - Duration::from_millis(1);
    assert!(observe(&f, &f.recipient, invitation).await.is_err());
    let next = prepare(&f, None).await;
    f.engine.location_stop_record(&f.record).await;
    assert!(observe(&f, &f.recipient, next).await.is_err());
    assert!(f.engine.location.state.lock().await.invitations.is_empty());
    let saved = serde_json::to_value(
        store::location::settings(&f.engine.store.pool, &f.record)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(
        saved.get("grant").is_none()
            && saved.get("fix").is_none()
            && saved.get("session_uid").is_none()
    );
}

#[tokio::test]
async fn invitation_lifetime_is_clipped_and_per_record_quota_is_reclaimed_on_expiry() {
    let f = Fixture::new().await;
    f.start(f.settings.clone()).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    f.engine
        .location
        .state
        .lock()
        .await
        .sessions
        .get_mut(&f.record)
        .unwrap()
        .deadline = deadline;
    for _ in 0..MAX_RECIPIENTS {
        prepare(&f, None).await;
    }
    let state = f.engine.location.state.lock().await;
    assert_eq!(state.invitations.len(), MAX_RECIPIENTS);
    assert!(
        state
            .invitations
            .values()
            .all(|lease| lease.deadline <= deadline)
    );
    drop(state);
    let command = Command::PrepareInvitation {
        person: f.controller.clone(),
        record_uid: f.record.clone(),
        recipient: f.recipient.clone(),
        duration_seconds: 3600,
        radius_metres: None,
    };
    assert!(f.engine.location_request(command, None).await.is_err());
    f.engine
        .location
        .state
        .lock()
        .await
        .invitations
        .values_mut()
        .next()
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    prepare(&f, None).await;
    assert_eq!(
        f.engine.location.state.lock().await.invitations.len(),
        MAX_RECIPIENTS
    );
    let template = prepare_template(&f).await;
    let mut state = f.engine.location.state.lock().await;
    state.invitations.clear();
    for _ in 0..256 {
        let mut invitation = template.clone();
        invitation.record_uid = nucleus::new_uid("r");
        invitation.grant.as_mut().unwrap().uid = nucleus::new_uid("li");
        state.invitations.insert(
            invitation.grant.as_ref().unwrap().uid.clone(),
            super::super::invitations::Lease {
                invitation,
                deadline,
            },
        );
    }
    drop(state);
    assert!(
        f.engine
            .location_request(
                Command::PrepareInvitation {
                    person: f.controller.clone(),
                    record_uid: f.record.clone(),
                    recipient: f.recipient.clone(),
                    duration_seconds: 60,
                    radius_metres: None
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(f.engine.location.state.lock().await.invitations.len(), 256);
}

async fn prepare_template(f: &Fixture) -> Invitation {
    f.engine
        .location
        .state
        .lock()
        .await
        .invitations
        .values()
        .next()
        .unwrap()
        .invitation
        .clone()
}

#[tokio::test]
async fn process_restart_restores_settings_but_never_invitation_tokens_or_positions() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("location.sqlite").display()
    );
    let f = Fixture::from_engine(Engine::open(&url).await.unwrap()).await;
    let mut settings = f.settings.clone();
    settings.recipients = vec![f.recipient.clone()];
    let source = f.start(settings.clone()).await;
    f.engine
        .location_accept_fix(
            &source.settings.source_node_id,
            &f.record,
            &source.session_uid,
            f.fix(1),
        )
        .await
        .unwrap();
    let invitation = prepare(&f, Some(1000)).await;
    let recipient = f.recipient.clone();
    let record = f.record.clone();
    let network = f._network.clone();
    drop(f);
    let reopened = Engine::open(&url).await.unwrap();
    reopened.attach_location_network(network.clone());
    assert_eq!(
        store::location::settings(&reopened.store.pool, &record)
            .await
            .unwrap(),
        Some(settings)
    );
    assert!(reopened.location_sources().await.is_empty());
    assert!(reopened.location.state.lock().await.sessions.is_empty());
    assert!(
        reopened
            .location_request(
                Command::ObserveInvitation {
                    person: recipient.clone(),
                    invitation
                },
                None
            )
            .await
            .is_err()
    );
    let view = reopened.location_view(&recipient, &record).await.unwrap();
    assert_eq!(view.status, Status::Stopped);
    assert!(view.fix.is_none());
}
