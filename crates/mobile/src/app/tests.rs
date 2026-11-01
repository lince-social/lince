use super::*;

#[test]
fn confirmation_waits_for_status_reply_and_runs_once() {
    let mut world = fixture();
    world.init_resource::<Intents>();
    world.resource_mut::<Mobile>().deleting =
        Some(Intent::ReplyTo("thread".into(), Some("message".into())));
    world.resource_mut::<Mobile>().pending.insert(
        "status".into(),
        Pending {
            topic: "roster",
            silent: true,
            ..Default::default()
        },
    );
    apply_intent(&mut world, Intent::Confirm).unwrap();
    apply(&mut world);
    assert!(world.resource::<Mobile>().deleting.is_some());
    assert!(world.resource::<Mobile>().confirmed);
    assert!(world.resource::<Mobile>().pending.contains_key("status"));
    assert!(
        !world
            .resource::<Mobile>()
            .drafts
            .contains_key("thread/parent")
    );
    receive_message(
        &mut world,
        ServerMessage::ActionOk {
            id: "status".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(serde_json::json!({
                "has_roster": false,
                "waiting_at_the_door": [],
                "discovery": null
            })),
        },
    );
    apply(&mut world);
    assert_eq!(
        world.resource::<Mobile>().drafts["thread/parent"],
        "message"
    );
    assert_eq!(
        world.resource::<Mobile>().rows["roster"][0]["has_roster"],
        serde_json::json!(false)
    );
    assert!(world.resource::<Mobile>().deleting.is_none());
    assert!(!world.resource::<Mobile>().confirmed);
    world
        .resource_mut::<Mobile>()
        .drafts
        .remove("thread/parent");
    apply(&mut world);
    assert!(
        !world
            .resource::<Mobile>()
            .drafts
            .contains_key("thread/parent")
    );
}

#[test]
fn cancel_stops_deferred_confirmation_and_failed_submission_keeps_review() {
    let mut world = fixture();
    world.init_resource::<Intents>();
    world.resource_mut::<Mobile>().deleting =
        Some(Intent::Act(engine::actions::Action::RosterCreateOrgan));
    world.resource_mut::<Mobile>().pending.insert(
        "status".into(),
        Pending {
            topic: "roster",
            ..Default::default()
        },
    );
    apply_intent(&mut world, Intent::Confirm).unwrap();
    apply_intent(&mut world, Intent::CancelDelete).unwrap();
    assert!(!world.resource::<Mobile>().confirmed);
    assert!(world.resource::<Mobile>().deleting.is_none());
    assert!(world.resource::<Mobile>().pending.contains_key("status"));
    world.resource_mut::<Mobile>().pending.remove("status");
    apply(&mut world);
    assert!(world.resource::<Mobile>().deleting.is_none());
    apply_intent(
        &mut world,
        Intent::Ask(engine::actions::Action::RosterCreateOrgan),
    )
    .unwrap();
    assert!(apply_intent(&mut world, Intent::Confirm).is_err());
    assert!(matches!(
        world.resource::<Mobile>().deleting,
        Some(Intent::Act(engine::actions::Action::RosterCreateOrgan))
    ));
    assert!(!world.resource::<Mobile>().confirmed);
}

#[test]
fn saved_address_replies_reach_the_record_panel_without_starting_live_location() {
    use lince_interface::location as ui;
    let mut world = fixture();
    world.init_resource::<Messages<ui::Request>>();
    let parent = world.spawn_empty().id();
    let record = nucleus::new_uid("r");
    ui::mount(&mut world, parent, &record, None, None);
    let field = world
        .query::<(Entity, &lince_interface::controls::Editor)>()
        .iter(&world)
        .find(|(_, editor)| editor.title.starts_with("Address Record"))
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("@street4");
    fn link(world: &mut World) -> ui::SavedLocationRequest {
        let button = world
            .query::<(Entity, &Children, &bevy::ui_widgets::Button)>()
            .iter(world)
            .find(|(_, children, _)| {
                children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|text| text.0 == "Link address Record")
                })
            })
            .unwrap()
            .0;
        world.trigger(bevy::ui_widgets::Activate { entity: button });
        world.flush();
        world
            .resource_mut::<Messages<ui::SavedLocationRequest>>()
            .drain()
            .next()
            .unwrap()
    }
    let request = link(&mut world);
    assert!(
        matches!(&request.command, nucleus::vocabulary::SavedLocationCommand::Bind { record: target, address } if target == &record && address == "@street4")
    );
    let candidate = nucleus::vocabulary::LocationCandidate {
        assertion_uid: Some(nucleus::new_uid("a")),
        record_uid: nucleus::new_uid("r"),
        head: "Street4".into(),
        slug: Some("street4".into()),
        latitude: -23.5,
        longitude: -46.6,
        radius_metres: Some(3000),
    };
    receive_message(
        &mut world,
        ServerMessage::ActionOk {
            id: request.id,
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(
                serde_json::to_value(nucleus::vocabulary::SavedLocation {
                    record_uid: record,
                    candidates: vec![candidate.clone()],
                    selected: Some(candidate),
                    explicit: true,
                    unavailable: false,
                    revision: 1,
                })
                .unwrap(),
            ),
        },
    );
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0 == "Approximate saved area · radius 3000 m")
    );
    let request = link(&mut world);
    receive_message(
        &mut world,
        ServerMessage::Error {
            id: request.id,
            message: "Address Record is private".into(),
            code: None,
        },
    );
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0 == "Address Record is private")
    );
    assert!(world.resource::<Messages<ui::Request>>().is_empty());
}

#[test]
fn mobile_schedule_initializes_with_bevy_ui_and_text_plugins() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::image::ImagePlugin::default(),
        bevy::text::TextPlugin,
        bevy::ui::UiPlugin,
        MobilePlugin,
    ));
    app.world_mut()
        .schedule_scope(PostUpdate, |world, schedule| {
            schedule.initialize(world).unwrap();
        });
}

fn fixture() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<Typography>();
    world.insert_resource(Mobile::new(PathBuf::from("unused-test-directory")));
    world.spawn((Shell, Node::default()));
    world
}

fn safe_modal(world: &mut World) -> Entity {
    let roots = world
        .query_filtered::<Entity, With<crate::safe_area::InsetModal>>()
        .iter(world)
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1);
    roots[0]
}

fn assert_modal_insets(world: &World, root: Entity, expected: [f32; 4]) {
    let node = world.get::<Node>(root).unwrap();
    assert_eq!(
        [node.left, node.top, node.right, node.bottom],
        expected.map(px)
    );
    assert_eq!(node.padding, UiRect::all(px(18)));
    assert_eq!(node.overflow, Overflow::scroll_y());
    assert!(world.get::<Modal>(root).is_some());
}

#[test]
fn safe_insets_arriving_before_location_and_visibility_mount_are_retained() {
    let mut world = fixture();
    let mut window = Window::default();
    window.resolution.set_scale_factor_override(Some(2.0));
    world.spawn(window);
    world.init_resource::<Messages<lince_interface::visibility::Request>>();
    crate::safe_area::update(&mut world, [8, 60, 12, 40]);
    let record = nucleus::new_uid("r");
    crate::location::open(&mut world, &record, None);
    let location = safe_modal(&mut world);
    assert_modal_insets(&world, location, [4.0, 30.0, 6.0, 20.0]);
    crate::visibility::open(&mut world, &record, nucleus::visibility::Data::LiveLocation);
    assert!(world.get_entity(location).is_err());
    let visibility = safe_modal(&mut world);
    assert_modal_insets(&world, visibility, [4.0, 30.0, 6.0, 20.0]);
}

#[test]
fn safe_insets_update_open_modal_on_rotation_and_scale_without_resetting_scroll_or_edit() {
    let mut world = fixture();
    let mut window = Window::default();
    window.resolution.set_scale_factor_override(Some(2.0));
    let window = world.spawn(window).id();
    world.init_resource::<Messages<lince_interface::visibility::Request>>();
    crate::visibility::open(
        &mut world,
        &nucleus::new_uid("r"),
        nucleus::visibility::Data::Place,
    );
    let root = safe_modal(&mut world);
    assert_modal_insets(&world, root, [0.0; 4]);
    world
        .entity_mut(root)
        .insert(ScrollPosition(Vec2::new(0.0, 71.0)));
    let editor = world
        .spawn((EditableText::new("unfinished address"), ChildOf(root)))
        .id();
    crate::safe_area::update(&mut world, [0, 80, 0, 48]);
    assert_modal_insets(&world, root, [0.0, 40.0, 0.0, 24.0]);
    world
        .get_mut::<Window>(window)
        .unwrap()
        .resolution
        .set_scale_factor_override(Some(1.0));
    crate::safe_area::refresh(&mut world);
    assert_modal_insets(&world, root, [0.0, 80.0, 0.0, 48.0]);
    crate::safe_area::update(&mut world, [30, 0, 70, 0]);
    assert_modal_insets(&world, root, [30.0, 0.0, 70.0, 0.0]);
    assert_eq!(
        world.get::<ScrollPosition>(root).unwrap().0,
        Vec2::new(0.0, 71.0)
    );
    assert_eq!(
        world
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "unfinished address"
    );
}

#[test]
fn person_drafts_are_separate_and_passwords_never_reach_disk() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = fixture();
    world.insert_resource(Mobile::new(directory.path().into()));
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person("r_person_one".into());
    world.resource_mut::<Mobile>().karma = Some(lince_interface::karma::Draft {
        name: "Private automation".into(),
        ..Default::default()
    });
    world
        .resource_mut::<Mobile>()
        .drafts
        .insert("r_one/body".into(), "Private draft".into());
    world
        .resource_mut::<Mobile>()
        .drafts
        .insert("login/password".into(), "secret".into());
    save(&mut world, "organ".into()).unwrap();
    let first = draft_directory(world.resource::<Mobile>());
    let bytes = std::fs::read_to_string(first.join("mobile-drafts.json")).unwrap();
    assert!(!bytes.contains("secret"));
    let saved = crate::storage::read(&first, "organ").unwrap().unwrap();
    assert_eq!(
        saved.karma.as_ref(),
        world.resource::<Mobile>().karma.as_ref()
    );
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person("r_person_two".into());
    let second = draft_directory(world.resource::<Mobile>());
    assert_ne!(first, second);
    assert!(crate::storage::read(&second, "organ").unwrap().is_none());
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person("../../elsewhere".into());
    assert!(
        draft_directory(world.resource::<Mobile>())
            .starts_with(directory.path().join("person-drafts"))
    );
}

#[test]
fn expired_session_keeps_unsaved_drafts_private_until_the_same_person_returns() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = fixture();
    world.insert_resource(Mobile::new(directory.path().into()));
    let person = crate::session::Identity::Person("one".into());
    let karma = lince_interface::karma::Draft {
        name: "Unsaved private automation".into(),
        ..Default::default()
    };
    {
        let mut state = world.resource_mut::<Mobile>();
        state.organ = Some("organ".into());
        state.identity = person.clone();
        state.karma = Some(karma.clone());
        state
            .drafts
            .insert("record/body".into(), "Unsaved private text".into());
        state
            .drafts
            .insert("login/password".into(), "Never retain this".into());
    }
    std::fs::write(
        directory.path().join("person-drafts"),
        "blocks directory creation",
    )
    .unwrap();
    assert!(save(&mut world, "organ".into()).is_err());
    identity_ready(
        &mut world,
        "organ".into(),
        false,
        crate::session::Identity::Locked,
    );
    assert!(world.resource::<Mobile>().drafts.is_empty());
    assert!(world.resource::<Mobile>().karma.is_none());
    assert_eq!(world.resource::<DraftRecovery>().0.len(), 1);
    identity_ready(
        &mut world,
        "organ".into(),
        false,
        crate::session::Identity::Person("two".into()),
    );
    assert!(
        !world
            .resource::<Mobile>()
            .drafts
            .contains_key("record/body")
    );
    assert!(world.resource::<Mobile>().karma.is_none());
    identity_ready(&mut world, "organ".into(), false, person);
    assert_eq!(world.resource::<Mobile>().karma.as_ref(), Some(&karma));
    assert_eq!(
        world.resource::<Mobile>().drafts["record/body"],
        "Unsaved private text"
    );
    assert!(
        !world
            .resource::<Mobile>()
            .drafts
            .contains_key("login/password")
    );
    std::fs::remove_file(directory.path().join("person-drafts")).unwrap();
    world.resource_mut::<Mobile>().save_error = false;
    save(&mut world, "organ".into()).unwrap();
    let restored = crate::storage::read(&draft_directory(world.resource::<Mobile>()), "organ")
        .unwrap()
        .unwrap();
    assert_eq!(restored.drafts["record/body"], "Unsaved private text");
    assert_eq!(restored.karma.as_ref(), Some(&karma));
}

#[test]
fn negative_cards_offer_completion_without_hiding_the_all_records_view() {
    let mut world = fixture();
    world.resource_mut::<Mobile>().rows.insert(
        "records".into(),
        vec![
            serde_json::json!({"uid":"negative","head":"Needs work","quantity":"-2"}),
            serde_json::json!({"uid":"zero","head":"Done","quantity":"0"}),
            serde_json::json!({"uid":"positive","head":"Contribution","quantity":"1"}),
        ],
    );
    render(&mut world);
    let complete: Vec<_> = world
        .query::<&ButtonIntent>()
        .iter(&world)
        .filter_map(|button| match &button.0 {
            Intent::CompleteRecord(uid) => Some(uid.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(complete, ["negative"]);
    assert!(
        world
            .query::<&ButtonIntent>()
            .iter(&world)
            .any(|button| matches!(button.0, Intent::ToggleNegative))
    );
}

#[test]
fn phone_pages_mount_without_desktop_canvas_and_keep_drafts_when_navigating() {
    let mut world = fixture();
    let uid = nucleus::new_uid("r");
    world.resource_mut::<Mobile>().rows.insert(
        "record".into(),
        vec![serde_json::json!({"uid":uid,"head":"Task","body":"Original","quantity":"0"})],
    );
    world
        .resource_mut::<Mobile>()
        .navigation
        .open(Page::Record(uid.clone()));
    render(&mut world);
    let field = world
        .query::<(Entity, &Input)>()
        .iter(&world)
        .find(|(_, input)| input.key == format!("{uid}/body"))
        .unwrap()
        .0;
    assert_eq!(world.get::<Node>(field).unwrap().min_height, px(48));
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("ação 👩‍💻");
    capture(&mut world);
    world
        .resource_mut::<Mobile>()
        .navigation
        .open(Page::Records);
    world.resource_mut::<Mobile>().dirty = true;
    render(&mut world);
    world.resource_mut::<Mobile>().navigation.back();
    world.resource_mut::<Mobile>().dirty = true;
    render(&mut world);
    let (_, input) = world
        .query::<(&Input, &EditableText)>()
        .iter(&world)
        .find(|(input, _)| input.key == format!("{uid}/body"))
        .unwrap();
    assert_eq!(input.value().to_string(), "ação 👩‍💻");
    assert_eq!(
        world
            .query_filtered::<Entity, With<Content>>()
            .iter(&world)
            .count(),
        1
    );
}

#[test]
fn disconnected_mutations_fail_without_losing_edits() {
    let mut world = fixture();
    world
        .resource_mut::<Mobile>()
        .drafts
        .insert("r_one/quantity".into(), "12.5".into());
    assert!(
        apply_intent(
            &mut world,
            Intent::SaveField("r_one".into(), "quantity".into())
        )
        .is_err()
    );
    assert_eq!(world.resource::<Mobile>().drafts["r_one/quantity"], "12.5");
    assert!(world.resource::<Mobile>().pending.is_empty());
}

#[test]
fn receiving_record_updates_does_not_replace_an_unfinished_editor() {
    let mut world = fixture();
    let uid = nucleus::new_uid("r");
    world.resource_mut::<Mobile>().rows.insert(
        "record".into(),
        vec![serde_json::json!({"uid":uid,"body":"Original"})],
    );
    world
        .resource_mut::<Mobile>()
        .navigation
        .open(Page::Record(uid.clone()));
    render(&mut world);
    let field = world
        .query::<(Entity, &Input)>()
        .iter(&world)
        .find(|(_, input)| input.key == format!("{uid}/body"))
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("Still typing");
    capture(&mut world);
    receive_message(
        &mut world,
        ServerMessage::Update {
            id: "mobile/record".into(),
            rows: vec![serde_json::json!({"uid":uid,"body":"Remote"})],
        },
    );
    render(&mut world);
    assert_eq!(
        world.get::<EditableText>(field).unwrap().value(),
        "Still typing"
    );
}

#[test]
fn scroll_returns_with_the_page_and_other_pages_drafts_do_not_block_updates() {
    let mut world = fixture();
    render(&mut world);
    world
        .query_filtered::<&mut ScrollPosition, With<Content>>()
        .single_mut(&mut world)
        .unwrap()
        .0
        .y = 200.0;
    world.resource_mut::<Mobile>().navigation.open(Page::Organ);
    world.resource_mut::<Mobile>().dirty = true;
    render(&mut world);
    world.resource_mut::<Mobile>().navigation.back();
    world.resource_mut::<Mobile>().dirty = true;
    render(&mut world);
    assert_eq!(
        world
            .query_filtered::<&ScrollPosition, With<Content>>()
            .single(&world)
            .unwrap()
            .0
            .y,
        200.0
    );
    world
        .resource_mut::<Mobile>()
        .drafts
        .insert("r_other/body".into(), "unfinished".into());
    receive_message(
        &mut world,
        ServerMessage::Update {
            id: "mobile/records".into(),
            rows: vec![serde_json::json!({"uid":"r_new","head":"Arrived"})],
        },
    );
    assert!(world.resource::<Mobile>().dirty);
}

#[test]
fn remote_head_updates_preserve_a_body_draft_and_its_document_base() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let mut world = fixture();
    let doc = loro::LoroDoc::new();
    doc.get_text("head").insert(0, "Original").unwrap();
    doc.get_text("body").insert(0, "Body").unwrap();
    let original = B64.encode(doc.export(loro::ExportMode::Snapshot).unwrap());
    world.resource_mut::<Mobile>().rows.insert(
        "record".into(),
        vec![serde_json::json!({"uid":"r_one","head":"Original","body":"Body"})],
    );
    world
        .resource_mut::<Mobile>()
        .navigation
        .open(Page::Record("r_one".into()));
    render(&mut world);
    refresh_document(&mut world, "r_one", &original).unwrap();
    let body = world
        .query::<(Entity, &Input)>()
        .iter(&world)
        .find(|(_, input)| input.key == "r_one/body")
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text("My draft");
    capture(&mut world);
    doc.get_text("head")
        .update("Remote title", Default::default())
        .unwrap();
    doc.get_text("body")
        .update("Remote body", Default::default())
        .unwrap();
    let remote = B64.encode(doc.export(loro::ExportMode::Snapshot).unwrap());
    refresh_document(&mut world, "r_one", &remote).unwrap();
    assert_eq!(world.resource::<Mobile>().documents["r_one/body"], original);
    assert_eq!(world.resource::<Mobile>().documents["r_one/head"], remote);
    assert_eq!(world.get::<EditableText>(body).unwrap().value(), "My draft");
    let head = world
        .query::<(&Input, &EditableText)>()
        .iter(&world)
        .find(|(input, _)| input.key == "r_one/head")
        .unwrap()
        .1;
    assert_eq!(head.value(), "Remote title");
}

#[test]
fn configured_transfer_automation_opens_the_shared_mobile_karma_editor() {
    let mut world = fixture();
    let person = nucleus::new_uid("r");
    world.resource_mut::<Mobile>().identity = crate::session::Identity::Person(person);
    let draft = lince_interface::karma::Draft {
        name: "Weekly contribution".into(),
        fields: ["freq(@weekly)", "> 0", "@need: repeat(@me)"].map(|text| {
            lince_interface::karma::FieldDraft {
                text: text.into(),
                linked: None,
            }
        }),
        ..Default::default()
    };
    let _ = apply_intent(&mut world, Intent::OpenKarmaDraft(draft.clone()));
    assert_eq!(world.resource::<Mobile>().navigation.current, Page::Karma);
    assert_eq!(world.resource::<Mobile>().karma.as_ref(), Some(&draft));
    let mut another = draft.clone();
    another.name = "Another contribution".into();
    let result = apply_intent(&mut world, Intent::OpenKarmaDraft(another));
    assert_eq!(world.resource::<Mobile>().karma.as_ref(), Some(&draft));
    assert!(result.is_err());
}
