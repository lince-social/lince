use super::*;

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

#[test]
fn person_drafts_are_separate_and_passwords_never_reach_disk() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = fixture();
    world.insert_resource(Mobile::new(directory.path().into()));
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person("r_person_one".into());
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
    {
        let mut state = world.resource_mut::<Mobile>();
        state.organ = Some("organ".into());
        state.identity = person.clone();
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
    identity_ready(&mut world, "organ".into(), false, person);
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
