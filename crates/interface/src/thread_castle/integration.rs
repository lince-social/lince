use super::*;
use bevy::input_focus::{FocusCause, InputFocus};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

async fn setup(
    live: bool,
) -> (
    App,
    Arc<engine::Engine>,
    Arc<cell::fiote::Host>,
    tempfile::TempDir,
    String,
) {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let root = tempfile::tempdir().unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateAgent {
                head: "Conversation test".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let host = Arc::new(
        cell::fiote::Host::open(engine.clone(), root.path().join("settings"))
            .await
            .unwrap(),
    );
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: Some(host.clone()),
        information: None,
    };
    let wake = crate::wake::WakeSignal::new(|| {});
    let mut bridge = crate::cell_bridge::connect(runtime.clone(), wake.clone());
    if live {
        let mut config: cell::FioteAgentConfig = serde_json::from_str(
            &std::env::var("LINCE_ACP_TEST_CONFIG").expect("LINCE_ACP_TEST_CONFIG"),
        )
        .unwrap();
        config.directory = root.path().into();
        bridge
            .outgoing
            .send(cell::ClientMessage::Fiote {
                id: "configure-live".into(),
                request: cell::FioteRequest::AgentConfigure {
                    record: record.clone(),
                    config,
                },
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                match bridge.incoming.recv().await.unwrap() {
                    cell::ServerMessage::Fiote { id, status } if id == "configure-live" => {
                        assert!(status.settings.enabled);
                        break;
                    }
                    cell::ServerMessage::Error { id, message, .. } if id == "configure-live" => {
                        panic!("{message}")
                    }
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<InputFocus>()
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(crate::app::CellHandle(runtime))
        .insert_resource(wake)
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::protein_area::ProteinAreaPlugin,
            crate::record_binding::RecordBindingPlugin,
            crate::fiote::session::Plugin,
            ThreadCastlePlugin,
        ));
    drop(bridge);
    let workspace = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    crate::full_record::open(app.world_mut(), workspace, &record, Source::Local).unwrap();
    pump(&mut app, |world| {
        world.query::<&ThreadCastle>().iter(world).next().is_some()
    })
    .await;
    (app, engine, host, root, record)
}

async fn pump(app: &mut App, ready: impl Fn(&mut World) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let world = app.world_mut();
        for (mut node, mut visibility) in world
            .query_filtered::<(&mut ComputedNode, &mut InheritedVisibility), With<EditableText>>()
            .iter_mut(world)
        {
            node.size = Vec2::new(300.0, 40.0);
            *visibility = InheritedVisibility::VISIBLE;
        }
        app.update();
        if ready(app.world_mut()) {
            return;
        }
        if Instant::now() >= deadline {
            let world = app.world_mut();
            let labels: Vec<_> = world
                .query::<&Text>()
                .iter(world)
                .map(|text| text.0.clone())
                .collect();
            let bodies: Vec<_> = world
                .query::<&EditableText>()
                .iter(world)
                .map(|text| text.value().to_string())
                .collect();
            panic!("UI did not reach expected state: labels={labels:?}, bodies={bodies:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn composer(world: &mut World) -> Option<(Entity, Entity, String)> {
    let castle = world.query::<&ThreadCastle>().iter(world).next()?;
    let thread = castle.active.clone()?;
    world
        .query::<(Entity, &ThreadForm)>()
        .iter(world)
        .find(|(_, form)| form.thread.as_ref() == Some(&thread))
        .map(|(entity, form)| (entity, form.input, thread))
}

#[tokio::test]
async fn record_threads_create_numbered_tabs_and_send_from_enter() {
    let (mut app, engine, host, _root, _record) = setup(false).await;
    let castle = app
        .world_mut()
        .query_filtered::<Entity, With<ThreadCastle>>()
        .single(app.world())
        .unwrap();
    assert!(composer(app.world_mut()).is_none());
    controls::Add.apply(app.world_mut(), castle);
    pump(&mut app, |world| composer(world).is_some()).await;
    let (form, input, thread) = composer(app.world_mut()).unwrap();
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":thread}],"fields":["head"]}),
    )
    .unwrap();
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap()[0]["head"],
        "Thread 1"
    );
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Hello from Enter");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    pump(&mut app, |world| {
        world.get::<ThreadForm>(form).unwrap().pending.is_none()
            && world
                .query::<&Page>()
                .iter(world)
                .any(|page| !page.messages.is_empty())
    })
    .await;
    assert!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string()
            .is_empty()
    );
    let tab = app
        .world_mut()
        .query::<(Entity, &TabName)>()
        .iter(app.world())
        .find(|(_, tab)| tab.thread == thread)
        .unwrap()
        .0;
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(tab, FocusCause::Navigated);
    let status = app
        .world()
        .get::<crate::record_binding::TextBinding>(tab)
        .unwrap()
        .status
        .unwrap();
    pump(&mut app, |world| {
        world
            .get::<Text>(status)
            .is_some_and(|text| text.0 == "Saved")
    })
    .await;
    app.world_mut()
        .get_mut::<EditableText>(tab)
        .unwrap()
        .editor
        .set_text("Planning");
    let renamed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            app.update();
            if protein::execute(&engine.store, &query).await.unwrap()[0]["head"] == "Planning" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        renamed.is_ok(),
        "Rename failed: {:?}; edits {:?}",
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect::<Vec<_>>(),
        app.world().get::<EditableText>(tab).unwrap().pending_edits
    );
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep this draft");
    controls::Add.apply(app.world_mut(), castle);
    pump(&mut app, |world| {
        composer(world).is_some_and(|(_, _, active)| active != thread)
    })
    .await;
    let (_, _, second) = composer(app.world_mut()).unwrap();
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":second}],"fields":["head"]}),
    )
    .unwrap();
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap()[0]["head"],
        "Thread 1"
    );
    let page = app.world().get::<ThreadCastle>(castle).unwrap().pages[&second];
    controls::AskDelete.apply(app.world_mut(), page);
    let delete = app
        .world_mut()
        .query::<(Entity, &Text)>()
        .iter(app.world())
        .find(|(_, text)| text.0 == "Delete thread")
        .unwrap()
        .0;
    let button = app.world().get::<ChildOf>(delete).unwrap().parent();
    let action = app
        .world()
        .get::<crate::actions::ActionButton>(button)
        .unwrap();
    let (target, actions) = (action.target, action.actions.clone());
    actions.run(app.world_mut(), target);
    pump(&mut app, |world| {
        !world
            .get::<ThreadCastle>(castle)
            .unwrap()
            .pages
            .contains_key(&second)
    })
    .await;
    assert!(
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep this draft"
    );
    host.stop_all().await;
}

#[tokio::test]
#[ignore = "Requires an installed authenticated ACP agent and consumes a live model turn"]
async fn installed_agent_replies_to_enter_in_record_castle() {
    let (mut app, _engine, host, _root, _record) = setup(true).await;
    if composer(app.world_mut()).is_none() {
        let castle = app
            .world_mut()
            .query_filtered::<Entity, With<ThreadCastle>>()
            .single(app.world())
            .unwrap();
        controls::Add.apply(app.world_mut(), castle);
    }
    pump(&mut app, |world| composer(world).is_some()).await;
    let (form, input, _) = composer(app.world_mut()).unwrap();
    app.world_mut().get_mut::<EditableText>(input).unwrap().editor.set_text("Hello. This is a UI connection test. Reply with exactly: Hello from the real agent. Do not use tools or change files or records.");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    pump(&mut app, |world| {
        world.get::<ThreadForm>(form).unwrap().pending.is_none()
            && world
                .query::<&crate::description::Description>()
                .iter(world)
                .any(|text| text.source.trim() == "Hello from the real agent.")
    })
    .await;
    assert!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string()
            .is_empty()
    );
    host.stop_all().await;
}
