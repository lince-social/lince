use std::time::Duration;

use nucleus::location::{Fix, SourceKind, Status};

use crate::theme::Typography;

use super::{render::position_text, requests::queue, *};

#[test]
fn saved_location_reply_discards_a_stale_person_selection() {
    use nucleus::vocabulary::SavedLocationCommand;

    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    let root = world.spawn_empty().id();
    let previous = nucleus::new_uid("r");
    let current = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let panel = mount_observer(&mut world, root, &record, Some(&previous));
    let person_input = {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.saved_pending = Some((
            "old-request".into(),
            SavedLocationCommand::Inspect { record },
            Instant::now(),
        ));
        state
            .fields
            .iter()
            .find_map(|(field, entity)| matches!(field, Field::Person).then_some(*entity))
            .expect("the observer has a Person input")
    };
    world
        .get_mut::<bevy::text::EditableText>(person_input)
        .unwrap()
        .editor
        .set_text(&current);

    assert!(receive_ref(&mut world, "old-request", Err("stale reply")));
    let state = world.get::<Panel>(panel).unwrap();
    assert_eq!(state.person, current);
    assert!(state.saved_pending.is_none());
    assert!(state.saved_location.is_none());
    assert_ne!(state.notice, "stale reply");
}

#[test]
fn linking_and_selecting_saved_addresses_send_identities_without_live_acquisition() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    world.init_resource::<Messages<SavedLocationRequest>>();
    let parent = world.spawn_empty().id();
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, None, None);
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.fields.clear();
        state.address = "@street4".into();
    }
    apply(&mut world, panel, Intent::BindAddress(String::new()));
    let mut cursor = world
        .resource::<Messages<SavedLocationRequest>>()
        .get_cursor();
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<SavedLocationRequest>>())
        .cloned()
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(
        matches!(&requests[0].command, nucleus::vocabulary::SavedLocationCommand::Bind { record: uid, address } if uid == &record && address == "@street4")
    );
    let assertion = nucleus::new_uid("a");
    let candidate = nucleus::vocabulary::LocationCandidate {
        assertion_uid: Some(assertion.clone()),
        record_uid: nucleus::new_uid("r"),
        head: "Street4".into(),
        slug: Some("street4".into()),
        latitude: 1.0,
        longitude: 2.0,
        radius_metres: None,
    };
    let location = nucleus::vocabulary::SavedLocation {
        record_uid: record.clone(),
        candidates: vec![candidate.clone()],
        selected: Some(candidate),
        explicit: true,
        unavailable: false,
        revision: 4,
    };
    assert!(receive(
        &mut world,
        &requests[0].id,
        Ok(serde_json::to_value(location).unwrap())
    ));
    apply(
        &mut world,
        panel,
        Intent::SelectSaved(Some(assertion.clone())),
    );
    let next: Vec<_> = cursor
        .read(world.resource::<Messages<SavedLocationRequest>>())
        .cloned()
        .collect();
    assert!(
        matches!(&next[0].command, nucleus::vocabulary::SavedLocationCommand::Select { assertion: Some(uid), expected_revision: 4, .. } if uid == &assertion)
    );
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(world.get::<Panel>(panel).unwrap().settings.is_none());
}

#[test]
fn loading_a_location_reference_never_starts_viewing_or_tracking() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let panel = mount_observer(&mut world, parent, "", None);
    let invitation = nucleus::location::Invitation {
        record_uid: nucleus::new_uid("r"),
        authority_node_id: "a".repeat(64),
        grant: None,
    };
    world.get_mut::<Panel>(panel).unwrap().reference = invitation.reference().unwrap();
    world.get_mut::<Panel>(panel).unwrap().fields.clear();
    apply(&mut world, panel, Intent::LoadReference);
    let state = world.get::<Panel>(panel).unwrap();
    assert_eq!(state.record, invitation.record_uid);
    assert_eq!(state.node, invitation.authority_node_id);
    assert!(!state.watching);
    assert!(state.settings.is_none());
    assert!(world.resource::<Messages<Request>>().is_empty());
}

#[test]
fn setting_a_saved_place_never_requests_live_acquisition() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<SavePlaceRequest>>();
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, None, None);
    assert!(world.get::<Panel>(panel).unwrap().context.is_none());
    assert!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .fields
            .iter()
            .any(|(field, _)| matches!(field, Field::PlaceLatitude))
    );
    world.resource_mut::<Messages<Request>>().clear();
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.pending = None;
        state.place_latitude = "-23.5".into();
        state.place_longitude = "-46.6".into();
        state.fields.clear();
    }
    apply(&mut world, panel, Intent::SetPlace);
    let requests: Vec<_> = world
        .resource_mut::<Messages<SavePlaceRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].record, record);
    assert_eq!(requests[0].latitude, -23.5);
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(world.get::<Panel>(panel).unwrap().settings.is_none());
}

fn view() -> View {
    View {
        record_uid: nucleus::new_uid("r"),
        status: Status::Live,
        session_uid: Some(nucleus::new_uid("location")),
        expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 60_000),
        source_kind: Some(SourceKind::Device),
        fix: Some(Fix {
            sequence: 1,
            latitude: -23.5505,
            longitude: -46.6333,
            accuracy_metres: Some(8.0),
            captured_at_ms: chrono::Utc::now().timestamp_millis(),
            capture_age_ms: 0,
        }),
        age_ms: Some(0),
        remaining_millis: Some(60_000),
        next_sequence: Some(2),
        radius_metres: None,
    }
}

#[test]
fn approximate_live_location_reports_area_size_without_point_or_device_accuracy() {
    let mut area = view();
    area.radius_metres = Some(2500);
    let text = position_text(Some(&(area, Stamp::now())));
    assert!(text.contains("radius 2500 m"));
    assert!(!text.contains("-23.5505"));
    assert!(!text.contains("-46.6333"));
    assert!(!text.contains('±'));
}

#[test]
fn live_recipient_edits_keep_showing_the_effective_audience_until_stopped() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    let parent = world.spawn_empty().id();
    let person = nucleus::new_uid("r");
    let recipient = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, Some(&person), None);
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.settings = Some(Settings {
            record_uid: record,
            controller_uid: person,
            source_cell_uid: nucleus::new_uid("r"),
            source_node_id: "a".repeat(64),
            source_kind: SourceKind::Device,
            duration_seconds: DEFAULT_DURATION_SECONDS,
            recipients: vec![recipient.clone()],
            transfer_uid: None,
        });
        state.view = Some((view(), Stamp::now()));
    }
    apply(&mut world, panel, Intent::Recipient(recipient.clone()));
    assert_eq!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .settings
            .as_ref()
            .unwrap()
            .recipients,
        vec![recipient]
    );
    assert!(world.get::<Panel>(panel).unwrap().notice.contains("Stop"));
}

#[test]
fn completed_view_polls_wait_before_requesting_another_fix() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let panel = mount(
        &mut world,
        parent,
        &nucleus::new_uid("r"),
        Some(&nucleus::new_uid("r")),
        None,
    );
    {
        let mut state = world.entity_mut(panel).take::<Panel>().unwrap();
        let command = Command::View {
            person: state.person.clone(),
            record_uid: state.record.clone(),
        };
        queue(&mut world, &mut state, command, Vec::new());
        world.entity_mut(panel).insert(state);
    }
    let id = world
        .get::<Panel>(panel)
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .id
        .clone();
    assert!(receive(
        &mut world,
        &id,
        Ok(serde_json::to_value(view()).unwrap())
    ));
    assert!(world.get::<Panel>(panel).unwrap().next_poll > Instant::now() + Duration::from_secs(2));
}

#[test]
fn saving_requires_a_fresh_fix_and_an_explicit_request() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<SavePlaceRequest>>();
    let parent = world.spawn_empty().id();
    let person = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, Some(&person), None);
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.settings = Some(Settings {
            record_uid: record.clone(),
            controller_uid: person,
            source_cell_uid: nucleus::new_uid("r"),
            source_node_id: "a".repeat(64),
            source_kind: SourceKind::Device,
            duration_seconds: DEFAULT_DURATION_SECONDS,
            recipients: Vec::new(),
            transfer_uid: None,
        });
        state.view = Some((view(), Stamp::now() - Duration::from_secs(61)));
    }
    apply(&mut world, panel, Intent::SavePlace);
    assert!(world.resource::<Messages<SavePlaceRequest>>().is_empty());
    world.get_mut::<Panel>(panel).unwrap().view = Some((view(), Stamp::now()));
    apply(&mut world, panel, Intent::SavePlace);
    let requests: Vec<_> = world
        .resource_mut::<Messages<SavePlaceRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].record, record);
    assert_eq!(requests[0].latitude, -23.5505);
    assert!(receive(
        &mut world,
        &requests[0].id,
        Ok(serde_json::Value::Null)
    ));
    assert!(world.get::<Panel>(panel).unwrap().notice.contains("Saved"));
}

#[test]
fn device_authentication_keeps_the_confirmed_observer_identity() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    let parent = world.spawn_empty().id();
    let panel = mount_observer(&mut world, parent, "", None);
    world.get_mut::<Panel>(panel).unwrap().authentication = Some(("login".into(), Instant::now()));
    let person = nucleus::new_uid("r");
    assert!(authentication_reply(
        &mut world,
        "login",
        Ok(person.clone())
    ));
    assert_eq!(world.get::<Panel>(panel).unwrap().person, person);
    assert!(world.get::<Panel>(panel).unwrap().watching);
}

#[test]
fn expired_positions_disappear_using_local_elapsed_time() {
    let old = (view(), Stamp::now() - Duration::from_secs(61));
    let text = position_text(Some(&old));
    assert!(!text.contains("23.5505"));
    assert!(text.contains("expired"));
}

#[test]
fn display_uses_shared_elapsed_clock_instead_of_authority_or_capture_wall_time() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TIME: AtomicU64 = AtomicU64::new(1_000);
    let mut world = World::new();
    world.insert_resource(ElapsedClock(|| TIME.load(Ordering::SeqCst)));
    let stamp = Stamp::for_world(&world);
    let mut current = view();
    current.expires_at_ms = Some(chrono::Utc::now().timestamp_millis() - 86_400_000);
    current.fix.as_mut().unwrap().captured_at_ms += 86_400_000;
    current.remaining_millis = Some(3_600_000);
    assert!(position_text(Some(&(current.clone(), stamp))).contains("23.5505"));
    TIME.store(91_000, Ordering::SeqCst);
    assert!(!position_text(Some(&(current, stamp))).contains("23.5505"));
}

pub(super) fn handover_panel() -> (World, Entity, View, Choice) {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let person = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, Some(&person), None);
    let current = Choice {
        uid: nucleus::new_uid("r"),
        label: "Desktop".into(),
        node_id: Some("a".repeat(64)),
    };
    let replacement = Choice {
        uid: nucleus::new_uid("r"),
        label: "Phone".into(),
        node_id: Some("b".repeat(64)),
    };
    let settings = Settings {
        record_uid: record.clone(),
        controller_uid: person,
        source_cell_uid: current.uid.clone(),
        source_node_id: "a".repeat(64),
        source_kind: SourceKind::Device,
        duration_seconds: DEFAULT_DURATION_SECONDS,
        recipients: vec![],
        transfer_uid: None,
    };
    let mut active = view();
    active.record_uid = record;
    let mut state = world.get_mut::<Panel>(panel).unwrap();
    state.context = Some(Context {
        authority_node_id: "a".repeat(64),
        settings: Some(settings.clone()),
        people: vec![],
        devices: vec![current.clone(), replacement.clone()],
        current_cell_uid: current.uid,
        current_node_id: "a".repeat(64),
        view: active.clone(),
    });
    state.settings = Some(settings);
    state.view = Some((active.clone(), Stamp::now()));
    state.fields.clear();
    (world, panel, active, replacement)
}

#[test]
fn handover_requires_an_unchanged_review_and_never_queues_source_approval() {
    let (mut world, panel, active, replacement) = handover_panel();
    apply(&mut world, panel, Intent::Device(replacement.clone()));
    apply(&mut world, panel, Intent::ReviewHandover);
    assert!(world.resource::<Messages<Request>>().is_empty());
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.minutes = "120".into();
        state.fields.clear();
    }
    apply(&mut world, panel, Intent::ConfirmHandover);
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(world.get::<Panel>(panel).unwrap().handover.is_none());
    apply(&mut world, panel, Intent::ReviewHandover);
    apply(&mut world, panel, Intent::ConfirmHandover);
    let requests = world
        .resource_mut::<Messages<Request>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(
        matches!(&requests[0].command,Command::Handover {settings,expected_session_uid} if settings.source_cell_uid == replacement.uid && settings.duration_seconds == 7200 && Some(expected_session_uid) == active.session_uid.as_ref())
    );
    let state = world.get::<Panel>(panel).unwrap();
    assert!(state.view.is_none());
    assert_eq!(state.pending.as_ref().unwrap().then.len(), 1);
    assert!(matches!(
        state.pending.as_ref().unwrap().then[0],
        Command::Context { .. }
    ));
}

#[test]
fn handover_replies_cannot_restore_a_changed_private_draft_or_viewer() {
    for viewer_changed in [false, true] {
        let (mut world, panel, mut reply, replacement) = handover_panel();
        apply(&mut world, panel, Intent::Device(replacement));
        apply(&mut world, panel, Intent::ReviewHandover);
        apply(&mut world, panel, Intent::ConfirmHandover);
        let request = world
            .resource_mut::<Messages<Request>>()
            .drain()
            .next()
            .unwrap();
        if viewer_changed {
            actor_changed(&mut world, &nucleus::new_uid("r"));
        } else {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.minutes = "121".into();
            state.fields.clear();
        }
        reply.session_uid = Some(nucleus::new_uid("location"));
        reply.status = Status::AwaitingApproval;
        reply.fix = None;
        let handled = receive(
            &mut world,
            &request.id,
            Ok(serde_json::to_value(reply).unwrap()),
        );
        assert_eq!(handled, !viewer_changed);
        let state = world.get::<Panel>(panel).unwrap();
        assert!(state.view.is_none());
        assert!(state.pending.is_none());
        assert!(state.handover.is_none());
        assert!(world.resource::<Messages<Request>>().is_empty());
        if viewer_changed {
            assert!(state.settings.is_none());
            assert!(state.context.is_none());
        }
    }
}

#[test]
fn stopping_hides_coordinates_and_ignores_earlier_view_replies() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let panel = mount(
        &mut world,
        parent,
        &nucleus::new_uid("r"),
        Some(&nucleus::new_uid("r")),
        None,
    );
    let current = view();
    {
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.view = Some((current.clone(), Stamp::now()));
        state.pending = Some(Pending {
            id: "old-view".into(),
            command: Command::View {
                person: state.person.clone(),
                record_uid: state.record.clone(),
            },
            then: Vec::new(),
            sent: Stamp::now(),
            save_place: false,
            scope: None,
        });
    }
    apply(&mut world, panel, Intent::Stop);
    assert!(world.get::<Panel>(panel).unwrap().view.is_none());
    assert!(!receive(
        &mut world,
        "old-view",
        Ok(serde_json::to_value(current).unwrap())
    ));
    assert!(world.get::<Panel>(panel).unwrap().view.is_none());
}
