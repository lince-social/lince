use super::*;

fn fixture() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<Typography>();
    world.insert_resource(Mobile::new(PathBuf::from("unused-test-directory")));
    world.spawn((Shell, Node::default()));
    world
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
