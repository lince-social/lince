use super::*;
use bevy::input_focus::{FocusCause, InputFocus};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) async fn setup(
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
    let creating = engine.clone();
    let record = tokio::spawn(async move {
        creating
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
            .unwrap()
    })
    .await
    .unwrap();
    let host = Arc::new(
        cell::fiote::Host::open(engine.clone(), root.path().join("settings"))
            .await
            .unwrap(),
    );
    let runtime = cell::CellRuntime {
        speech: None,
        commands: Default::default(),
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
        let requests = [
            (
                "save-live",
                cell::fiote_connection::Request::Save {
                    profile: cell::fiote_connection::Profile {
                        id: "live-test".into(),
                        name: "Existing AI runtime".into(),
                        connection: cell::fiote_connection::Connection::Harness { config },
                    },
                },
            ),
            (
                "connect-live",
                cell::fiote_connection::Request::Select {
                    id: "live-test".into(),
                    api_key: None,
                    password: None,
                },
            ),
        ];
        for (identity, request) in requests {
            bridge
                .outgoing
                .send(cell::ClientMessage::Fiote {
                    id: identity.into(),
                    request: cell::FioteRequest::Connections {
                        record: record.clone(),
                        request,
                    },
                })
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    match bridge.incoming.recv().await.unwrap() {
                        cell::ServerMessage::Fiote { id, status } if id == identity => {
                            if identity == "connect-live" {
                                assert!(status.settings.enabled);
                                assert_eq!(
                                    status.connections.profiles.selected.as_deref(),
                                    Some("live-test")
                                );
                            }
                            break;
                        }
                        cell::ServerMessage::Error { id, message, .. } if id == identity => {
                            panic!("{message}")
                        }
                        _ => {}
                    }
                }
            })
            .await
            .unwrap();
        }
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

pub(super) async fn pump(app: &mut App, ready: impl Fn(&mut World) -> bool) {
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

pub(super) fn composer(world: &mut World) -> Option<(Entity, Entity, String)> {
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
    let load = app
        .world_mut()
        .query::<(
            &crate::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(app.world())
        .find(|(_, node)| node.label() == Some("Load conversation choices · no tokens"))
        .map(|(button, _)| button.clone())
        .unwrap();
    load.actions.run(app.world_mut(), load.target);
    pump(&mut app, |world| {
        world
            .query::<(&crate::dropdown::Dropdown, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .any(|(_, node)| node.label() == Some("Model"))
    })
    .await;
    for (name, choice) in [
        ("Model", "6.1 Sol"),
        ("Reasoning effort", "Medium"),
        ("Reasoning effort", "Low"),
        ("Fast mode", "Off"),
        ("Fast mode", "On"),
    ] {
        let (toggle, menu) = app
            .world_mut()
            .query::<(
                Entity,
                &crate::dropdown::Dropdown,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(app.world())
            .find(|(_, _, node)| node.label() == Some(name))
            .map(|(entity, dropdown, _)| (entity, dropdown.menu))
            .unwrap();
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: toggle });
        let option = app
            .world_mut()
            .query::<(
                &crate::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
                &ChildOf,
            )>()
            .iter(app.world())
            .find(|(_, node, parent)| parent.parent() == menu && node.label() == Some(choice))
            .map(|(button, _, _)| button.clone())
            .unwrap();
        option.actions.run(app.world_mut(), option.target);
        pump(&mut app, |world| {
            !world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "Updating conversation settings…")
                && world
                    .query::<(
                        &crate::dropdown::Dropdown,
                        &bevy::a11y::AccessibilityNode,
                        &Children,
                    )>()
                    .iter(world)
                    .filter(|(_, node, _)| node.label() == Some(name))
                    .any(|(_, _, children)| {
                        children.iter().any(|child| {
                            world
                                .get::<Text>(child)
                                .is_some_and(|text| text.0 == choice)
                        })
                    })
        })
        .await;
    }
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

#[tokio::test]
#[ignore = "Requires an authenticated ACP harness and consumes a live model turn"]
async fn installed_agent_analyzes_files_dropped_into_the_native_composer() {
    let (mut app, _engine, host, _directory, _record) = setup(true).await;
    app.init_resource::<Assets<Image>>()
        .add_message::<bevy::window::FileDragAndDrop>();
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
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, FocusCause::Navigated);
    let window = app.world_mut().spawn(Window::default()).id();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fiote");
    for name in [
        "note.txt",
        "table.csv",
        "document.pdf",
        "photo.png",
        "video.mp4",
    ] {
        app.world_mut()
            .write_message(bevy::window::FileDragAndDrop::DroppedFile {
                window,
                path_buf: fixtures.join(name),
            });
    }
    pump(&mut app, |world| {
        crate::message_content::contents(world, form).is_ok_and(|parts| parts.len() == 5)
    })
    .await;
    app.world_mut().get_mut::<EditableText>(input).unwrap().editor.set_text("Analyze these attachments without using tools or changing files or Records. Reply only with a JSON object: text_marker is the marker phrase in note.txt; csv_total is the numeric sum of the value column in table.csv; image_color is the color of the square in photo.png; pdf_marker is the marker phrase in document.pdf; video_understood is true only if you can actually see the video content. Do not infer video content from the image.");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    pump(&mut app, |world| {
        world
            .query::<&crate::description::Description>()
            .iter(world)
            .any(|description| {
                serde_json::from_str::<serde_json::Value>(description.source.trim())
                    .is_ok_and(|value| value["csv_total"] == 22)
            })
    })
    .await;
    let result = app
        .world_mut()
        .query::<&crate::description::Description>()
        .iter(app.world())
        .find_map(|description| {
            serde_json::from_str::<serde_json::Value>(description.source.trim())
                .ok()
                .filter(|value| value["csv_total"] == 22)
        })
        .unwrap();
    assert!(
        result["text_marker"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("seven blue birds")
    );
    assert_eq!(
        result["image_color"].as_str().unwrap().to_lowercase(),
        "red"
    );
    assert!(
        result["pdf_marker"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("seven blue birds")
    );
    assert!(
        crate::message_content::contents(app.world(), form)
            .unwrap()
            .is_empty()
    );
    println!("Live native attachment analysis: {result}");
    host.stop_all().await;
}

#[tokio::test]
#[ignore = "Requires an authenticated ACP harness and consumes a live model turn"]
async fn installed_agent_sets_up_actual_native_habits_workspace() {
    fn contains(component: &nucleus::canvas::Component, kind: &str, text: Option<&str>) -> bool {
        match component {
            nucleus::canvas::Component::Native {
                kind: actual,
                settings,
                ..
            } => actual == kind && text.is_none_or(|text| settings["text"] == text),
            nucleus::canvas::Component::Builtin {
                state: nucleus::component::ComponentState::Text { text: actual },
            } => kind == "text" && text.is_none_or(|text| actual == text),
            nucleus::canvas::Component::Composition { composition } => composition
                .parts
                .iter()
                .any(|part| contains(&part.component, kind, text)),
            _ => false,
        }
    }
    let (mut app, engine, host, _directory, record) = setup(true).await;
    app.init_resource::<Assets<Image>>().add_plugins((
        crate::canvas_host::Plugin,
        crate::component_push::ComponentPushPlugin,
    ));
    let root = app
        .world_mut()
        .query_filtered::<Entity, (
            With<crate::container::BoxRoot>,
            With<crate::workspace::Workspaces>,
        )>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(root)
        .insert(crate::canvas::CanvasView::default());
    app.update();
    if composer(app.world_mut()).is_none() {
        let castle = app
            .world_mut()
            .query_filtered::<Entity, With<ThreadCastle>>()
            .single(app.world())
            .unwrap();
        controls::Add.apply(app.world_mut(), castle);
    }
    pump(&mut app, |world| composer(world).is_some()).await;
    let (_, input, _) = composer(app.world_mut()).unwrap();
    app.world_mut().get_mut::<EditableText>(input).unwrap().editor.set_text("This is an isolated native canvas acceptance test. Use the normal Lince MCP tools to inspect the connected canvas and supported component registry. Create a workspace named Fiote acceptance habits. Add one reusable composition containing a Todo component and a text component; the text must say Walk daily. Use valid registry settings and fresh request IDs/revisions. Preserve existing workspaces and components. Do not edit files or other Records. Briefly confirm the actual applied result.");
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
        let snapshot = crate::canvas_host::capture(world, root).unwrap();
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.name == "Fiote acceptance habits");
        workspace.is_some_and(|workspace| {
            let placements = snapshot
                .placements
                .iter()
                .filter(|placement| placement.workspace == workspace.id)
                .collect::<Vec<_>>();
            placements
                .iter()
                .any(|placement| contains(&placement.component, "todo", None))
                && placements
                    .iter()
                    .any(|placement| contains(&placement.component, "text", Some("Walk daily")))
        })
    })
    .await;
    let snapshot = crate::canvas_host::capture(app.world_mut(), root).unwrap();
    let (generated, composition) = app
        .world_mut()
        .query::<(Entity, &crate::canvas_host::composition::Generated)>()
        .iter(app.world())
        .find(|(_, generated)| {
            generated
                .composition
                .parts
                .iter()
                .any(|part| contains(&part.component, "todo", None))
                && generated
                    .composition
                    .parts
                    .iter()
                    .any(|part| contains(&part.component, "text", Some("Walk daily")))
        })
        .map(|(entity, generated)| (entity, generated.composition.clone()))
        .unwrap();
    assert_eq!(composition.origin.as_ref().unwrap().agent, record);
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(generated)
            .unwrap()
            .immunity,
        crate::area_effects::Immunity::Isolation
    );
    let stopping = tokio::spawn(async move { host.stop_all().await });
    pump(&mut app, |_| stopping.is_finished()).await;
    stopping.await.unwrap();
    let save = app
        .world_mut()
        .query::<(
            &crate::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(app.world())
        .find(|(button, node)| button.target == generated && node.label() == Some("Save component"))
        .map(|(button, _)| button.clone())
        .unwrap();
    save.actions.run(app.world_mut(), save.target);
    pump(&mut app, |world| {
        let status = world
            .get::<crate::canvas_host::composition::Generated>(generated)
            .unwrap()
            .status;
        world
            .get::<Text>(status)
            .is_some_and(|text| text.0.starts_with("Saved component "))
    })
    .await;
    let status = app
        .world()
        .get::<crate::canvas_host::composition::Generated>(generated)
        .unwrap()
        .status;
    let saved = app
        .world()
        .get::<Text>(status)
        .unwrap()
        .0
        .split_whitespace()
        .last()
        .unwrap()
        .to_owned();
    let saved = store::records::get(&engine.store.pool, &saved)
        .await
        .unwrap()
        .unwrap();
    let document = nucleus::canvas::Document::decode(&saved.body).unwrap();
    let nucleus::canvas::Component::Composition {
        composition: restored,
    } = document.component
    else {
        panic!("Saved Fiote output is not a composition");
    };
    assert_eq!(restored.origin, composition.origin);
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.name == "Fiote acceptance habits")
        .unwrap()
        .id;
    let reopened = crate::canvas_host::spawn(
        app.world_mut(),
        root,
        workspace,
        bevy::math::DVec2::ONE,
        &nucleus::canvas::Component::Composition {
            composition: restored,
        },
    )
    .unwrap();
    assert_ne!(
        app.world()
            .get::<crate::canvas_host::Identity>(generated)
            .unwrap()
            .0,
        app.world()
            .get::<crate::canvas_host::Identity>(reopened)
            .unwrap()
            .0
    );
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(reopened)
            .unwrap()
            .immunity,
        crate::area_effects::Immunity::Isolation
    );
    println!(
        "Live canvas result: {} workspaces, {} placements",
        snapshot.workspaces.len(),
        snapshot.placements.len()
    );
}
