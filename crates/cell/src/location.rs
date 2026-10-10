use crate::{CellRuntime, ClientMessage, ServerMessage};

struct LoginTask(tokio::task::JoinHandle<()>);

impl Drop for LoginTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::actions::Action;
    use engine::wire::{PeerAddr, Wire};
    use nucleus::location::{Command, Settings, SourceKind};
    use std::sync::Arc;

    fn runtime(engine: Arc<engine::Engine>) -> CellRuntime {
        CellRuntime {
            speech: None,
            commands: Default::default(),
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(crate::LaneHub::new()),
            wire: Arc::new(tokio::sync::RwLock::new(None)),
            information: None,
            fiote: None,
        }
    }

    fn address(wire: &Wire) -> PeerAddr {
        PeerAddr::new(wire.node_id()).with_ip_addr(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            wire.endpoint()
                .bound_sockets()
                .into_iter()
                .next()
                .unwrap()
                .port(),
        )))
    }

    #[tokio::test]
    async fn live_login_admits_an_outside_location_view_without_a_contact_or_record_access() {
        let root = tempfile::tempdir().unwrap();
        let authority = runtime(Arc::new(engine::Engine::open_memory().await.unwrap()));
        let observer = runtime(Arc::new(engine::Engine::open_memory().await.unwrap()));
        let role = store::auth::ensure_role(&authority.store.pool, "location-controller")
            .await
            .unwrap();
        for action in ["read", "update"] {
            let permission =
                store::auth::ensure_permission(&authority.store.pool, "record", action)
                    .await
                    .unwrap();
            store::auth::grant(&authority.store.pool, role, permission)
                .await
                .unwrap();
        }
        let hash = utils::auth::hash_password("location test password").unwrap();
        let controller = store::auth::create_person_login(
            &authority.store.pool,
            "Controller",
            "controller",
            &hash,
            role,
        )
        .await
        .unwrap();
        let reader = store::auth::ensure_role(&authority.store.pool, "location-observer")
            .await
            .unwrap();
        let recipient = store::auth::create_person_login(
            &authority.store.pool,
            "Observer",
            "observer",
            &hash,
            reader,
        )
        .await
        .unwrap();
        let authority_task = LoginTask(
            authority
                .start_loopback_peer(&root.path().join("authority.key"))
                .await
                .unwrap(),
        );
        let observer_task = LoginTask(
            observer
                .start_loopback_peer(&root.path().join("observer.key"))
                .await
                .unwrap(),
        );
        let authority_wire = authority.wire.read().await.clone().unwrap();
        let observer_wire = observer.wire.read().await.clone().unwrap();
        let node = authority_wire.node_id().to_string();
        observer_wire.remember_addr(address(&authority_wire));
        authority_wire.remember_addr(address(&observer_wire));
        let record = authority
            .engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Private ride context".into(),
                    body: "Private terms".into(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let cell = store::cells::local(&authority.store.pool)
            .await
            .unwrap()
            .unwrap();
        authority
            .engine
            .act(
                Action::Location {
                    request: Command::Configure {
                        settings: Settings {
                            record_uid: record.clone(),
                            controller_uid: controller.clone(),
                            source_cell_uid: cell.uid,
                            source_node_id: node.clone(),
                            source_kind: SourceKind::Manual,
                            duration_seconds: 3600,
                            recipients: vec![recipient.clone()],
                            transfer_uid: None,
                        },
                    },
                },
                None,
            )
            .await
            .unwrap();
        authority
            .engine
            .act(
                Action::Location {
                    request: Command::Start {
                        person: controller.clone(),
                        record_uid: record.clone(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        authority
            .engine
            .act(
                Action::Location {
                    request: Command::Approve {
                        person: controller.clone(),
                        record_uid: record.clone(),
                    },
                },
                None,
            )
            .await
            .unwrap();
        let lease = authority.engine.location_sources().await.remove(0);
        authority
            .engine
            .act(
                Action::Location {
                    request: Command::Publish {
                        person: controller,
                        record_uid: record.clone(),
                        session_uid: lease.session_uid,
                        fix: nucleus::location::Fix {
                            sequence: 1,
                            latitude: -23.5505,
                            longitude: -46.6333,
                            accuracy_metres: None,
                            captured_at_ms: chrono::Utc::now().timestamp_millis(),
                        },
                    },
                },
                None,
            )
            .await
            .unwrap();
        let request = Command::Observe {
            person: recipient.clone(),
            record_uid: record.clone(),
            node_id: node.clone(),
        };
        assert!(
            observer
                .engine
                .act(
                    Action::Location {
                        request: request.clone()
                    },
                    None
                )
                .await
                .is_err()
        );
        assert!(
            observer
                .authenticate_location_device(
                    &node,
                    &record,
                    "",
                    None,
                    "observer".into(),
                    "incorrect".into()
                )
                .await
                .is_err()
        );
        assert_eq!(
            observer
                .authenticate_location_device(
                    &node,
                    &record,
                    "",
                    None,
                    "observer".into(),
                    "location test password".into()
                )
                .await
                .unwrap(),
            recipient
        );
        let view = observer
            .engine
            .act(
                Action::Location {
                    request: request.clone(),
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(view["fix"].is_object());
        assert!(view.get("settings").is_none());
        assert!(view.get("people").is_none());
        assert!(view.get("body").is_none());
        assert!(
            !authority
                .engine
                .may_read_record(Some(&recipient), &record)
                .await
                .unwrap()
        );
        assert!(
            store::organs::contact_by_node_id(
                &authority.store.pool,
                &observer_wire.node_id().to_string()
            )
            .await
            .unwrap()
            .is_none()
        );
        let organ = store::organs::local(&observer.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        store::organs::add_contact(&authority.store.pool, &organ, None, "Observer Organ", "", 2)
            .await
            .unwrap();
        store::organs::set_trust(&authority.store.pool, &organ, "known")
            .await
            .unwrap();
        store::organs::set_node_id(
            &authority.store.pool,
            &organ,
            Some(&observer_wire.node_id().to_string()),
        )
        .await
        .unwrap();
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                upper: Some(nucleus::visibility::Bound {
                    value: 3,
                    inclusive: false,
                }),
                ..Default::default()
            }],
            exclude: vec![],
        };
        authority
            .engine
            .visibility_request(
                nucleus::visibility::Command::Save {
                    record_uid: record.clone(),
                    data: nucleus::visibility::Data::LiveLocation,
                    policy: policy.clone(),
                    expected_revision: 0,
                },
                None,
            )
            .await
            .unwrap();
        let organ_request = Command::Observe {
            person: String::new(),
            record_uid: record.clone(),
            node_id: node.clone(),
        };
        let projected = observer
            .engine
            .location_request(organ_request.clone(), None)
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(projected["fix"].is_object());
        assert!(projected["session_uid"].is_null());
        assert!(projected.get("settings").is_none());
        store::organs::set_proximity(&authority.store.pool, &organ, 3)
            .await
            .unwrap();
        assert!(
            observer
                .engine
                .location_request(organ_request.clone(), None)
                .await
                .is_err()
        );
        store::organs::set_proximity(&authority.store.pool, &organ, 2)
            .await
            .unwrap();
        let mut excluded = policy;
        excluded.exclude.push(nucleus::visibility::Condition {
            organs: vec![organ],
            ..Default::default()
        });
        authority
            .engine
            .visibility_request(
                nucleus::visibility::Command::Save {
                    record_uid: record.clone(),
                    data: nucleus::visibility::Data::LiveLocation,
                    policy: excluded,
                    expected_revision: 1,
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            observer
                .engine
                .location_request(organ_request, None)
                .await
                .is_err()
        );
        let denied = observer
            .engine
            .location_request(request.clone(), None)
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(denied["fix"].is_null());
        authority
            .engine
            .visibility_request(
                nucleus::visibility::Command::Save {
                    record_uid: record.clone(),
                    data: nucleus::visibility::Data::LiveLocation,
                    policy: Default::default(),
                    expected_revision: 2,
                },
                None,
            )
            .await
            .unwrap();
        let mut connection = authority.store.pool.acquire().await.unwrap();
        store::sqlx::query("UPDATE person_device SET revoked=1,revision=revision+1 WHERE person_uid=? AND node_id=?")
            .bind(&recipient).bind(observer_wire.node_id().to_string()).execute(&mut *connection).await.unwrap();
        drop(connection);
        assert!(
            observer
                .engine
                .act(Action::Location { request }, None)
                .await
                .is_err()
        );
        drop(authority_task);
        drop(observer_task);
    }
}

impl CellRuntime {
    pub async fn authenticate_location_device(
        &self,
        node: &str,
        record: &str,
        local_person: &str,
        expected_person: Option<&str>,
        username: String,
        password: String,
    ) -> Result<String, String> {
        if username.len() > 256 || password.len() > engine::private_password::MAX_PASSWORD_BYTES {
            return Err("Location login exceeds its limit".into());
        }
        let node = if node.is_empty() {
            self.engine
                .location_authority(record)
                .await
                .map_err(|error| error.to_string())?
        } else {
            node.into()
        };
        let wire = self
            .wire
            .read()
            .await
            .clone()
            .ok_or("The device connection is unavailable")?;
        if node == wire.node_id().to_string() {
            let password = engine::private_password::PasswordInput::new(password.into_bytes())
                .map_err(|error| error.to_string())?;
            let login = self
                .engine
                .login_password(&username, password, Some(&node))
                .await
                .map_err(|error| error.to_string())?;
            let person = login.person_uid().to_string();
            if expected_person.is_some_and(|expected| !expected.is_empty() && expected != person) {
                return Err("Sign in as the selected location Person".into());
            }
            self.engine
                .bind_location_observer(
                    &node,
                    if local_person.is_empty() {
                        &person
                    } else {
                        local_person
                    },
                    &person,
                )
                .map_err(|error| error.to_string())?;
            return Ok(person);
        }
        let endpoint = node.parse().map_err(|_| "Invalid location endpoint")?;
        let connection = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wire.endpoint().connect(
                engine::wire::PeerAddr::new(endpoint),
                engine::wire::ALPN_LIVE,
            ),
        )
        .await
        .map_err(|_| "Location login timed out")?
        .map_err(|_| "Location authority is unavailable")?;
        let (outgoing, requests) = tokio::sync::mpsc::channel(4);
        let (responses, mut incoming) = tokio::sync::mpsc::channel(8);
        outgoing
            .send(ClientMessage::LiveLogin { username, password })
            .await
            .map_err(|_| "Location login closed")?;
        let task = LoginTask(tokio::spawn(async move {
            if let Err(message) =
                crate::live_client::drive(connection, requests, responses.clone(), || {}).await
            {
                let _ = responses
                    .send(ServerMessage::Error {
                        id: "location-login".into(),
                        message,
                        code: None,
                    })
                    .await;
            }
        }));
        let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
            while let Some(message) = incoming.recv().await {
                match message {
                    ServerMessage::SessionAuthenticated { person, .. } => return Ok(person),
                    ServerMessage::Error { message, .. }
                    | ServerMessage::LiveLoginError { message } => return Err(message),
                    _ => {}
                }
            }
            Err("Location login closed".into())
        })
        .await
        .map_err(|_| "Location login timed out".to_string())
        .and_then(|result| result);
        drop(task);
        let person: String = result?;
        if expected_person.is_some_and(|expected| !expected.is_empty() && expected != person) {
            return Err("Sign in as the selected location Person".into());
        }
        self.engine
            .bind_location_observer(
                &node,
                if local_person.is_empty() {
                    &person
                } else {
                    local_person
                },
                &person,
            )
            .map_err(|error| error.to_string())?;
        Ok(person)
    }
}
