use super::*;

fn setup() -> (World, Entity) {
    let (mut world, panel, _, _) = super::tests::handover_panel();
    let mut state = world.get_mut::<Panel>(panel).unwrap();
    state.invite_recipient = nucleus::new_uid("r");
    state.invite_conversation = nucleus::new_uid("r");
    state.invite_radius = "1000".into();
    state.fields.clear();
    drop(state);
    draw(&mut world, panel);
    (world, panel)
}

fn activate(world: &mut World, caption: &str) {
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == caption)
        .unwrap()
        .0;
    let entity = world.get::<ChildOf>(label).unwrap().parent();
    assert!(world.get::<Button>(entity).is_some());
    world.trigger(bevy::ui_widgets::Activate { entity });
    world.flush();
}

fn prepared(world: &World, panel: Entity) -> serde_json::Value {
    let state = world.get::<Panel>(panel).unwrap();
    let invitation = nucleus::location::Invitation {
        record_uid: state.record.clone(),
        authority_node_id: state.context.as_ref().unwrap().authority_node_id.clone(),
        grant: Some(nucleus::location::InvitationGrant {
            uid: nucleus::new_uid("li"),
            issuer: state.person.clone(),
            recipient: state.invite_recipient.clone(),
            expires_at_ms: 1_800_000_000_000,
            radius_metres: Some(1000),
        }),
    };
    serde_json::json!({"invitation":invitation,"remaining_millis":60000})
}

#[test]
fn invitation_buttons_require_review_and_send_only_the_unchanged_scope() {
    let (mut world, panel) = setup();
    activate(&mut world, "Review private invitation");
    let request = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .next()
        .unwrap();
    assert!(matches!(request.command, Command::PrepareInvitation { .. }));
    let response = prepared(&world, panel);
    assert!(receive(&mut world, &request.id, Ok(response.clone())));
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .invitation_review
            .is_some()
    );
    activate(&mut world, "Send reviewed private invitation");
    let sent = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 1);
    let state = world.get::<Panel>(panel).unwrap();
    assert!(
        matches!(&sent[0].command,Command::SendInvitation {person,conversation,invitation} if person == &state.person && conversation == &state.invite_conversation && serde_json::to_value(invitation).unwrap()==response["invitation"])
    );
    assert!(state.pending.as_ref().unwrap().then.is_empty());
}

#[test]
fn changed_invitation_reviews_and_delayed_or_mismatched_responses_cannot_send() {
    for field in 0..5 {
        for after_reply in [false, true] {
            let (mut world, panel) = setup();
            activate(&mut world, "Review private invitation");
            let request = world
                .resource_mut::<Messages<Request>>()
                .drain()
                .next()
                .unwrap();
            let response = prepared(&world, panel);
            if after_reply {
                receive(&mut world, &request.id, Ok(response.clone()));
            }
            {
                let mut state = world.get_mut::<Panel>(panel).unwrap();
                match field {
                    0 => state.invite_recipient = nucleus::new_uid("r"),
                    1 => state.invite_conversation = nucleus::new_uid("r"),
                    2 => state.invite_radius = "2000".into(),
                    3 => state.invite_minutes = "120".into(),
                    _ => state.person = nucleus::new_uid("r"),
                }
                state.fields.clear();
            }
            if !after_reply {
                receive(&mut world, &request.id, Ok(response));
            }
            apply(&mut world, panel, Intent::SendInvitation);
            assert!(world.resource::<Messages<Request>>().is_empty());
            assert!(
                world
                    .get::<Panel>(panel)
                    .unwrap()
                    .invitation_review
                    .is_none()
            );
        }
    }
    for changed in [
        "recipient",
        "radius_metres",
        "issuer",
        "authority_node_id",
        "record_uid",
        "remaining_millis",
    ] {
        let (mut world, panel) = setup();
        activate(&mut world, "Review private invitation");
        let request = world
            .resource_mut::<Messages<Request>>()
            .drain()
            .next()
            .unwrap();
        let mut response = prepared(&world, panel);
        match changed {
            "radius_metres" => response["invitation"]["grant"][changed] = serde_json::json!(2000),
            "recipient" | "issuer" => {
                response["invitation"]["grant"][changed] = serde_json::json!(nucleus::new_uid("r"))
            }
            "remaining_millis" => response[changed] = serde_json::json!(86_400_001u64),
            _ => {
                response["invitation"][changed] = serde_json::json!(if changed == "record_uid" {
                    nucleus::new_uid("r")
                } else {
                    "b".repeat(64)
                })
            }
        }
        receive(&mut world, &request.id, Ok(response));
        assert!(
            world
                .get::<Panel>(panel)
                .unwrap()
                .invitation_review
                .is_none()
        );
        assert!(world.resource::<Messages<Request>>().is_empty());
    }
    let (mut world, panel) = setup();
    activate(&mut world, "Review private invitation");
    let request = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .next()
        .unwrap();
    let response = prepared(&world, panel);
    actor_changed(&mut world, &nucleus::new_uid("r"));
    assert!(!receive(&mut world, &request.id, Ok(response)));
    let state = world.get::<Panel>(panel).unwrap();
    assert!(state.invitation_review.is_none() && state.view.is_none() && state.context.is_none());
}

#[test]
fn opening_a_scoped_reference_never_observes_or_downgrades_it_and_expired_review_cannot_send() {
    let (mut world, panel) = setup();
    let response = prepared(&world, panel);
    let invitation: nucleus::location::Invitation =
        serde_json::from_value(response["invitation"].clone()).unwrap();
    let parent = world.spawn_empty().id();
    let observer = mount_observer(
        &mut world,
        parent,
        "",
        Some(&invitation.grant.as_ref().unwrap().recipient),
    );
    {
        let mut state = world.get_mut::<Panel>(observer).unwrap();
        state.authentication = Some(("old-endpoint-login".into(), Instant::now()));
        state.username = "old-login".into();
        state.password = "old-secret".into();
    }
    load_reference(&mut world, observer, &invitation.reference().unwrap()).unwrap();
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(!authentication_reply(
        &mut world,
        "old-endpoint-login",
        Ok(nucleus::new_uid("r"))
    ));
    let state = world.get::<Panel>(observer).unwrap();
    assert!(!state.watching && state.settings.is_none());
    assert!(state.password.is_empty() && state.username.is_empty());
    assert!(matches!(
        observer_command(state).unwrap(),
        Command::ObserveInvitation { .. }
    ));
    world.get_mut::<Panel>(observer).unwrap().record = nucleus::new_uid("r");
    assert!(observer_command(world.get::<Panel>(observer).unwrap()).is_err());
    activate(&mut world, "Review private invitation");
    let request = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .next()
        .unwrap();
    receive(&mut world, &request.id, Ok(response));
    let mut state = world.get_mut::<Panel>(panel).unwrap();
    state.fields.clear();
    let review = state.invitation_review.as_mut().unwrap();
    review.received = review.received - Duration::from_secs(61);
    drop(state);
    apply(&mut world, panel, Intent::SendInvitation);
    assert!(world.resource::<Messages<Request>>().is_empty());
}

#[test]
fn stop_cancels_invitation_review_and_discards_old_responses_without_approval() {
    let (mut world, panel) = setup();
    activate(&mut world, "Review private invitation");
    let request = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .next()
        .unwrap();
    let response = prepared(&world, panel);
    receive(&mut world, &request.id, Ok(response.clone()));
    assert!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .invitation_review
            .is_some()
    );
    activate(&mut world, "Stop");
    let requests = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(matches!(requests[0].command, Command::Stop { .. }));
    let state = world.get::<Panel>(panel).unwrap();
    assert!(state.invitation_review.is_none() && state.view.is_none());
    assert!(state.pending.as_ref().unwrap().then.is_empty());
    assert!(!receive(&mut world, &request.id, Ok(response)));
    apply(&mut world, panel, Intent::SendInvitation);
    assert!(world.resource::<Messages<Request>>().is_empty());
}

#[test]
fn start_renders_pending_approval_and_the_button_binds_its_exact_session() {
    let (mut world, panel) = setup();
    let mut stopped = world
        .get::<Panel>(panel)
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .0
        .clone();
    stopped.remaining_millis = Some(0);
    stopped.elapse(0);
    world.get_mut::<Panel>(panel).unwrap().view = Some((stopped.clone(), Stamp::now()));
    draw(&mut world, panel);
    activate(&mut world, "Start location");
    let configure = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .next()
        .unwrap();
    assert!(matches!(configure.command, Command::Configure { .. }));
    receive(
        &mut world,
        &configure.id,
        Ok(serde_json::to_value(&stopped).unwrap()),
    );
    let requests = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(matches!(requests[0].command, Command::Start { .. }));
    let epoch = nucleus::new_uid("location");
    stopped.status = Status::AwaitingApproval;
    stopped.session_uid = Some(epoch.clone());
    stopped.source_kind = Some(SourceKind::Device);
    stopped.remaining_millis = Some(3_600_000);
    receive(
        &mut world,
        &requests[0].id,
        Ok(serde_json::to_value(stopped).unwrap()),
    );
    assert!(world.resource::<Messages<Request>>().is_empty());
    activate(&mut world, "Approve this device");
    let approvals = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(approvals.len(), 1);
    assert!(
        matches!(&approvals[0].command,Command::Approve {expected_session_uid,..} if expected_session_uid==&epoch)
    );
}
