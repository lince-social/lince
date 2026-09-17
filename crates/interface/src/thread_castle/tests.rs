use super::*;
use serde_json::json;

fn fixture() -> (World, Entity, Value) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let area = world.spawn_empty().id();
    let parent = world.spawn(Node::default()).id();
    let data = json!({"uid":"record", "threads":[
        {"uid":"thread-a", "head":"A", "messages_limit":50, "messages_has_more":true, "messages":[{"uid":"message-a", "author":"Alice", "body":"# Heading\n\n**Rich** message\n\n```mermaid\ngraph LR\n A --> B\n```"}]},
        {"uid":"thread-b", "head":"B", "messages_limit":50, "messages_has_more":false, "messages":[{"uid":"message-b", "author":"Bob", "body":"Second"}]}
    ]});
    populate(
        &mut world,
        parent,
        RecordBinding {
            area,
            uid: "record".into(),
            source: Source::Local,
        },
        &data,
    );
    (world, parent, data)
}

#[cfg_attr(test, test)]
fn switching_threads_keeps_drafts_and_rich_message_editors() {
    let (mut world, castle, mut data) = fixture();
    let a = world.get::<ThreadCastle>(castle).unwrap().pages["thread-a"];
    let b = world.get::<ThreadCastle>(castle).unwrap().pages["thread-b"];
    assert_eq!(world.get::<Node>(a).unwrap().display, Display::Flex);
    assert_eq!(world.get::<Node>(b).unwrap().display, Display::None);
    let input = world
        .query::<&ThreadForm>()
        .iter(&world)
        .find(|form| form.thread.as_deref() == Some("thread-a"))
        .unwrap()
        .input;
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Unsent draft");
    let message = world.get::<Page>(a).unwrap().messages["message-a"];
    assert!(
        world
            .query::<&crate::description::Description>()
            .iter(&world)
            .any(|description| description.source.starts_with("# Heading"))
    );
    Switch("thread-b".into()).apply(&mut world, castle);
    assert_eq!(world.get::<Node>(a).unwrap().display, Display::None);
    assert_eq!(world.get::<Node>(b).unwrap().display, Display::Flex);
    Switch("thread-a".into()).apply(&mut world, castle);
    data["threads"][0]["messages"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"uid":"older", "body":"Older"}));
    refresh(&mut world, castle, &data);
    assert_eq!(world.get::<Page>(a).unwrap().messages["message-a"], message);
    assert_eq!(
        world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Unsent draft"
    );
    let viewport = world.get::<Page>(a).unwrap().viewport;
    assert!(world.get::<Viewport>(viewport).unwrap().prepend);
    data["threads"][0]["messages"] = json!([]);
    refresh(&mut world, castle, &data);
    assert!(world.get_entity(message).is_err());
    data["threads"].as_array_mut().unwrap().remove(0);
    refresh(&mut world, castle, &data);
    assert_eq!(
        world.get::<ThreadCastle>(castle).unwrap().active.as_deref(),
        Some("thread-b")
    );
    assert_eq!(world.get::<Node>(b).unwrap().display, Display::Flex);
}

#[cfg_attr(test, test)]
fn history_preserves_scroll_anchor_and_new_messages_follow_only_at_bottom() {
    let mut app = App::new();
    app.add_systems(Update, anchor_scroll);
    let entity = app
        .world_mut()
        .spawn((
            Viewport {
                page: Entity::PLACEHOLDER,
                height: 0.0,
                initialized: false,
                prepend: false,
            },
            ScrollPosition::default(),
            ComputedNode {
                size: Vec2::new(300.0, 100.0),
                content_size: Vec2::new(300.0, 500.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(entity).unwrap().0.y,
        400.0
    );
    app.world_mut()
        .get_mut::<ScrollPosition>(entity)
        .unwrap()
        .0
        .y = 20.0;
    app.world_mut().get_mut::<Viewport>(entity).unwrap().prepend = true;
    app.world_mut()
        .get_mut::<ComputedNode>(entity)
        .unwrap()
        .content_size
        .y = 800.0;
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(entity).unwrap().0.y,
        320.0
    );
    app.world_mut()
        .get_mut::<ComputedNode>(entity)
        .unwrap()
        .content_size
        .y = 900.0;
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(entity).unwrap().0.y,
        320.0
    );
    app.world_mut()
        .get_mut::<ScrollPosition>(entity)
        .unwrap()
        .0
        .y = 800.0;
    app.world_mut()
        .get_mut::<ComputedNode>(entity)
        .unwrap()
        .content_size
        .y = 1000.0;
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(entity).unwrap().0.y,
        900.0
    );
}

#[cfg_attr(test, test)]
fn deletion_errors_keep_the_message_and_send_results_keep_newer_drafts() {
    let (mut world, castle, _) = fixture();
    let page = world.get::<ThreadCastle>(castle).unwrap().pages["thread-a"];
    let message = world.get::<Page>(page).unwrap().messages["message-a"];
    AskDelete(true).apply(&mut world, message);
    let confirmation = world.get::<Message>(message).unwrap().confirmation;
    assert_eq!(
        world.get::<Node>(confirmation).unwrap().display,
        Display::Flex
    );
    world.get_mut::<Message>(message).unwrap().pending = true;
    assert!(finished(
        &mut world,
        message,
        Some("Permission denied".into())
    ));
    assert!(world.get_entity(message).is_ok());
    let status = world.get::<Message>(message).unwrap().status;
    assert!(
        world
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("Permission denied")
    );
    let (form, input) = world
        .query::<(Entity, &ThreadForm)>()
        .iter(&world)
        .find(|(_, form)| form.thread.as_deref() == Some("thread-a"))
        .map(|(entity, form)| (entity, form.input))
        .unwrap();
    world.get_mut::<ThreadForm>(form).unwrap().pending = Some("Sent".into());
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("New draft");
    finished(&mut world, form, None);
    assert_eq!(
        world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "New draft"
    );
}

crate::laboratory_cases! {
    switching_threads_keeps_drafts_and_rich_message_editors,
    history_preserves_scroll_anchor_and_new_messages_follow_only_at_bottom,
    deletion_errors_keep_the_message_and_send_results_keep_newer_drafts,
}
