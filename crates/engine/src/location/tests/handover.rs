use super::*;

pub(super) async fn source_race(
    authority: &Engine,
    source: &Engine,
    person: &str,
    record: &str,
    organ: &str,
) {
    let previous = authority.location.state.lock().await.sessions[record]
        .lease
        .clone();
    let phone = previous.settings.clone();
    let mut desktop = phone.clone();
    desktop.source_cell_uid = store::cells::local(&authority.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    desktop.source_node_id = "a".repeat(64);
    let (left, right) = tokio::join!(
        authority.location_request(
            Command::Handover {
                settings: desktop.clone(),
                expected_session_uid: previous.session_uid.clone()
            },
            None
        ),
        authority.location_request(
            Command::Handover {
                settings: phone.clone(),
                expected_session_uid: previous.session_uid.clone()
            },
            None
        ),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let mut current = authority.location.state.lock().await.sessions[record]
        .lease
        .clone();
    assert_ne!(current.session_uid, previous.session_uid);
    assert!(!current.approved);
    assert!(authority.location_sources().await.is_empty());
    assert!(source.location_sources().await.is_empty());
    assert!(
        authority
            .location_view(person, record)
            .await
            .unwrap()
            .fix
            .is_none()
    );
    authority
        .location_stop_record_if(record, Some(&previous.session_uid))
        .await;
    assert_eq!(
        authority.location.state.lock().await.sessions[record]
            .lease
            .session_uid,
        current.session_uid
    );
    assert!(
        authority
            .location_peer(
                organ,
                &phone.source_node_id,
                PeerRequest::ApproveSource {
                    person: person.into(),
                    record_uid: record.into(),
                    session_uid: previous.session_uid.clone()
                }
            )
            .await
            .is_err()
    );
    assert!(
        authority
            .location_peer(
                organ,
                &phone.source_node_id,
                PeerRequest::Publish {
                    record_uid: record.into(),
                    session_uid: previous.session_uid.clone(),
                    challenge_uid: previous.challenge_uid,
                    fix: Fix {
                        sequence: 999,
                        latitude: 0.0,
                        longitude: 0.0,
                        accuracy_metres: None,
                        captured_at_ms: now_ms(),
                        capture_age_ms: 0
                    }
                }
            )
            .await
            .is_err()
    );
    if current.settings.source_node_id != desktop.source_node_id {
        authority
            .location_request(
                Command::Handover {
                    settings: desktop.clone(),
                    expected_session_uid: current.session_uid,
                },
                None,
            )
            .await
            .unwrap();
        current = authority.location.state.lock().await.sessions[record]
            .lease
            .clone();
    }
    authority
        .location_request(
            Command::Approve {
                person: person.into(),
                record_uid: record.into(),
                expected_session_uid: current.session_uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let mut connection = authority.store.pool.acquire().await.unwrap();
    let device = store::session_access::device_on(&mut connection, person, &phone.source_node_id)
        .await
        .unwrap()
        .unwrap();
    store::session_access::compare_and_set_revoked_on(
        &mut connection,
        person,
        &phone.source_node_id,
        device.revision,
        true,
    )
    .await
    .unwrap();
    drop(connection);
    assert!(
        authority
            .location_request(
                Command::Handover {
                    settings: phone,
                    expected_session_uid: current.session_uid.clone()
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        authority.location.state.lock().await.sessions[record]
            .lease
            .session_uid,
        current.session_uid
    );
    assert_eq!(
        store::location::settings(&authority.store.pool, record)
            .await
            .unwrap(),
        Some(desktop)
    );
    let saved = serde_json::to_value(
        store::location::settings(&authority.store.pool, record)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    for field in [
        "approved",
        "fix",
        "latitude",
        "longitude",
        "session_uid",
        "challenge_uid",
    ] {
        assert!(saved.get(field).is_none());
    }
}
