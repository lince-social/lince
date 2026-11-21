// non-lean-yet
// Scope: struct QueryBodies, fn read_queries, fn wait_for, fn view.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use crate::native::{
    app::CellHandle,
    cell_bridge::{CellBridge, CellBridgePlugin, CellMessage, ReceiveCell, records_subscription},
    protein_area::RecordBinding,
    record_binding::{self, RecordBindingPlugin, SyncBindings},
    theme::Typography,
    wake::WakeSignal,
};
use bevy::ecs::change_detection::DetectChanges;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Resource, Default)]
struct QueryBodies(BTreeMap<String, (String, String)>);

fn read_queries(mut messages: MessageReader<CellMessage>, mut observations: ResMut<QueryBodies>) {
    for message in messages.read() {
        let (id, rows) = match message.0.as_ref() {
            cell::ServerMessage::Snapshot { id, rows }
            | cell::ServerMessage::Update { id, rows } => (id, rows),
            cell::ServerMessage::Error { id, message, .. } if id.starts_with("preview-oracle-") => {
                panic!("Private query {id}: {message}");
            }
            _ => continue,
        };
        if id.starts_with("preview-oracle-") {
            assert_eq!(rows.len(), 1, "Each query is scoped to one Record");
            observations.0.insert(
                id.clone(),
                (
                    rows[0]["uid"].as_str().unwrap().into(),
                    rows[0]["body"].as_str().unwrap().into(),
                ),
            );
        }
    }
}

async fn wait_for(app: &mut App, wake: &tokio::sync::Notify, ready: impl Fn(&World) -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    for _ in 0..64 {
        app.update();
        if ready(app.world()) {
            return;
        }
        tokio::time::timeout_at(deadline, wake.notified())
            .await
            .expect("Private Record preview did not settle");
    }
    panic!("Private Record preview exceeded its wake budget");
}

fn view(world: &mut World, root: Entity, uid: &str, source: &str, mode: Mode) -> (Entity, Editor) {
    let parent = world.spawn((Node::default(), ChildOf(root))).id();
    let input = crate::native::sand::text_editor(source, world.resource::<Typography>(), 0);
    let input = world.spawn((input, ChildOf(parent))).id();
    record_binding::attach(
        world,
        input,
        RecordBinding {
            area: parent,
            uid: uid.into(),
            source: Source::Local,
        },
        "body",
        None,
    );
    attach_editor(
        world,
        parent,
        input,
        Context {
            owner: parent,
            source: Source::Local,
        },
    );
    mode.apply(world, parent);
    (parent, *world.get::<Editor>(parent).unwrap())
}

fn contains_span(world: &World, preview: Entity, expected: &str) -> bool {
    let mut pending = vec![preview];
    while let Some(entity) = pending.pop() {
        if world
            .get::<TextSpan>(entity)
            .is_some_and(|text| text.0 == expected)
        {
            return true;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    false
}

#[tokio::test]
async fn local_body_edit_updates_pretty_split_and_live_query_views_without_rebuilding_unrelated_content()
 {
    const ORIGINAL: &str = "## Original café";
    const UNRELATED: &str = "**Unrelated Record**";
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("preview.sqlite").display()
    );
    let engine = Arc::new(engine::Engine::open(&url).await.unwrap());
    let mut uids = Vec::new();
    for (head, body) in [("Edited", ORIGINAL), ("Unrelated", UNRELATED)] {
        let result = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: head.into(),
                    body: body.into(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
        uids.push(result.created.unwrap());
    }
    let [uid, other] = uids.as_slice() else {
        unreachable!()
    };
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let ready = Arc::new(tokio::sync::Notify::new());
    let wake = Arc::clone(&ready);
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((MinimalPlugins, CellBridgePlugin, RecordBindingPlugin))
        .init_resource::<Assets<Font>>()
        .init_resource::<Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .init_resource::<QueryBodies>()
        .insert_resource(CellHandle(runtime))
        .insert_resource(WakeSignal::new(move || wake.notify_one()))
        .add_systems(Update, read_queries.after(ReceiveCell))
        .add_systems(PostUpdate, sync_editors.after(SyncBindings));
    let root = app
        .world_mut()
        .spawn(crate::native::container::BoxRoot)
        .id();
    let split = view(app.world_mut(), root, uid, ORIGINAL, Mode::Split);
    let pretty = view(app.world_mut(), root, uid, ORIGINAL, Mode::Pretty);
    let raw = view(app.world_mut(), root, uid, ORIGINAL, Mode::Raw);
    let unrelated = view(app.world_mut(), root, other, UNRELATED, Mode::Pretty);
    wait_for(&mut app, &ready, |world| {
        [split, pretty, raw, unrelated]
            .iter()
            .all(|(_, view)| record_binding::can_edit(world, view.input))
    })
    .await;

    let sender = app.world().non_send::<CellBridge>().outgoing.clone();
    for (id, target) in [
        ("preview-oracle-list", uid),
        ("preview-oracle-detail", uid),
        ("preview-oracle-other", other),
    ] {
        let cell::ClientMessage::Subscribe { mut protein, .. } = records_subscription() else {
            unreachable!()
        };
        protein.filter = vec![protein::Predicate::UidEq(target.clone())];
        protein.fields = Some(vec!["uid".into(), "body".into()]);
        sender
            .send(cell::ClientMessage::Subscribe {
                id: id.into(),
                protein,
            })
            .await
            .unwrap();
    }
    wait_for(&mut app, &ready, |world| {
        world.resource::<QueryBodies>().0.len() == 3
    })
    .await;
    let unrelated_tick = app
        .world()
        .entity(unrelated.1.preview)
        .get_ref::<Description>()
        .unwrap()
        .last_changed();
    for (source, target, marker) in [
        (split, pretty, "First café revision"),
        (pretty, split, "Second 日本語 revision"),
    ] {
        Mode::Split.apply(app.world_mut(), source.0);
        Mode::Pretty.apply(app.world_mut(), target.0);
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(source.1.input, bevy::input_focus::FocusCause::Pressed);
        let body = format!("## {marker}");
        app.world_mut()
            .get_mut::<EditableText>(source.1.input)
            .unwrap()
            .editor
            .set_text(&body);
        wait_for(&mut app, &ready, |world| {
            let queries = &world.resource::<QueryBodies>().0;
            [source, target, raw].iter().all(|(_, view)| {
                world.get::<EditableText>(view.input).unwrap().value() == body.as_str()
            }) && ["preview-oracle-list", "preview-oracle-detail"]
                .iter()
                .all(|id| {
                    let (observed_uid, observed_body) = &queries[*id];
                    observed_uid == uid && observed_body == &body
                })
                && [source.1.preview, target.1.preview]
                    .iter()
                    .all(|preview| contains_span(world, *preview, marker))
        })
        .await;
        let world = app.world();
        assert_eq!(
            world.get::<Node>(source.1.input).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            world.get::<Node>(target.1.input).unwrap().display,
            Display::None
        );
        assert!(
            [source, target].iter().all(|(_, view)| world
                .get::<Node>(view.preview)
                .unwrap()
                .display
                == Display::Flex)
        );
        assert_eq!(
            world
                .entity(unrelated.1.preview)
                .get_ref::<Description>()
                .unwrap()
                .last_changed(),
            unrelated_tick
        );
        assert_eq!(
            world.resource::<QueryBodies>().0["preview-oracle-other"],
            (other.clone(), UNRELATED.into())
        );
        assert_eq!(engine.doc_text(uid).await.unwrap().1, body);
    }
    drop(sender);
    drop(app);
    engine.store.pool.close().await;
    let reopened = engine::Engine::open(&url).await.unwrap();
    assert_eq!(
        reopened.doc_text(uid).await.unwrap().1,
        "## Second 日本語 revision"
    );
    assert_eq!(reopened.doc_text(other).await.unwrap().1, UNRELATED);
}
