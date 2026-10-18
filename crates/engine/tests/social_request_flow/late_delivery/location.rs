use super::*;
use nucleus::location::{Command as Location, Invitation, Settings, SourceKind};

struct LocalLocation;

#[async_trait::async_trait]
impl engine::location::Network for LocalLocation {
    fn node_id(&self) -> String {
        iroh::SecretKey::from_bytes(&[238; 32]).public().to_string()
    }
    async fn location_request(
        &self,
        _: &str,
        _: nucleus::location::PeerRequest,
    ) -> Result<Value, EngineError> {
        Err(EngineError::Consequence(
            "This local invitation needs no location transport".into(),
        ))
    }
}

#[tokio::test]
async fn location_invitation_uses_ordinary_private_delivery_without_sending_coordinates() {
    let fixture = accepted().await;
    let network: Arc<dyn engine::location::Network> = Arc::new(LocalLocation);
    fixture.sender.attach_location_network(network.clone());
    let role = store::auth::ensure_role(&fixture.sender.store.pool, "invitation-controller")
        .await
        .unwrap();
    for action in ["read", "update"] {
        let permission = store::auth::ensure_permission(&fixture.sender.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::grant(&fixture.sender.store.pool, role, permission)
            .await
            .unwrap();
    }
    let controller = store::auth::create_person_login(
        &fixture.sender.store.pool,
        "Driver",
        "invitation.driver",
        "hash",
        role,
    )
    .await
    .unwrap();
    let recipient = store::auth::create_person_login(
        &fixture.sender.store.pool,
        "Passenger",
        "invitation.passenger",
        "hash",
        role,
    )
    .await
    .unwrap();
    let record = fixture
        .sender
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Private ride position".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let cell = store::cells::local(&fixture.sender.store.pool)
        .await
        .unwrap()
        .unwrap();
    let settings = Settings {
        record_uid: record.clone(),
        controller_uid: controller.clone(),
        source_cell_uid: cell.uid,
        source_node_id: network.node_id(),
        source_kind: SourceKind::Manual,
        duration_seconds: 3600,
        recipients: vec![recipient.clone()],
        transfer_uid: None,
    };
    for request in [
        Location::Configure { settings },
        Location::Start {
            person: controller.clone(),
            record_uid: record.clone(),
        },
    ] {
        fixture
            .sender
            .location_request(request, None)
            .await
            .unwrap();
    }
    let epoch = fixture
        .sender
        .location_pending_sources()
        .await
        .into_iter()
        .find(|source| source.settings.record_uid == record)
        .unwrap()
        .session_uid;
    fixture
        .sender
        .location_request(
            Location::Approve {
                person: controller.clone(),
                record_uid: record.clone(),
                expected_session_uid: epoch,
            },
            None,
        )
        .await
        .unwrap();
    let context = fixture
        .sender
        .location_request(
            Location::Context {
                person: controller.clone(),
                record_uid: record.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    fixture
        .sender
        .location_request(
            Location::Publish {
                person: controller.clone(),
                record_uid: record.clone(),
                session_uid: context["view"]["session_uid"].as_str().unwrap().into(),
                fix: nucleus::location::Fix {
                    sequence: 1,
                    latitude: -23.5505,
                    longitude: -46.6333,
                    accuracy_metres: Some(8.0),
                    captured_at_ms: nucleus::execution::now().timestamp_millis(),
                    capture_age_ms: 0,
                },
            },
            None,
        )
        .await
        .unwrap();
    let response = fixture
        .sender
        .location_request(
            Location::PrepareInvitation {
                person: controller.clone(),
                record_uid: record,
                recipient,
                duration_seconds: 600,
                radius_metres: Some(1000),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let invitation: Invitation = serde_json::from_value(response["invitation"].clone()).unwrap();
    let sent = fixture
        .sender
        .location_request(
            Location::SendInvitation {
                person: controller,
                conversation: fixture.conversation.clone(),
                invitation: invitation.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let message = sent["invitation_message"]["message"].as_str().unwrap();
    for _ in 0..2 {
        exchange(&fixture.sender, &fixture.receiver).await;
    }
    let received = command(&fixture.receiver, Command::Requests { after: None }).await;
    let body = received["requests"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|request| request["messages"].as_array().unwrap())
        .filter_map(|message| message["body"].as_str())
        .find(|body| body.contains("lince-location:1:"))
        .unwrap();
    assert_eq!(
        Invitation::parse(body.lines().last().unwrap()).unwrap(),
        invitation
    );
    for private in [
        "-23.5505",
        "-46.6333",
        "accuracy_metres",
        "session_uid",
        "latitude",
        "longitude",
    ] {
        assert!(!body.contains(private));
    }
    let status = command(
        &fixture.sender,
        Command::PrivateDeliveryStatus {
            message: message.into(),
        },
    )
    .await;
    assert!(status["private_delivery"].is_object());
    let sender = command(&fixture.sender, Command::Requests { after: None }).await;
    assert_eq!(
        sender["requests"][0]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["body"]
                .as_str()
                .is_some_and(|body| body.contains("lince-location:1:")))
            .count(),
        1
    );
}
