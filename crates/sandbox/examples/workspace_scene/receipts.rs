//! The operator loses a published reply. A real local Fiote tool session then continues on
//! the same document, and a third desktop process recovers both operators' original receipts.
use super::{Replay, control, fill, inspect, screenshot, selected};
use bevy::prelude::*;
use nucleus::{
    canvas::{
        Request as CanvasRequest,
        scene::operation::{Edit, Effect, Inspection, Operation, Receipt, Request},
    },
    component::composition::Origin,
    karma::DecimalValue,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use store::workspace_scene;

#[derive(Resource)]
struct Client {
    tools: Arc<cell::FioteTools>,
    origin: Origin,
}

pub(super) fn install(app: &mut App, runtime: &tokio::runtime::Runtime) {
    let (cell, tools, origin) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let agent = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Workbench Fiote".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let thread = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Workbench conversation".into(),
                    body: String::new(),
                    quantity: 0.,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let cell = cell::CellRuntime {
            commands: Default::default(),
            lanes: Arc::new(cell::LaneHub::new()),
            store: engine.store.clone(),
            engine,
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let mut tools = cell::FioteTools::default();
        cell.local_session()
            .into_native_tools(cell::FioteContext {
                agent: agent.clone(),
                record: agent.clone(),
                thread: thread.clone(),
            })
            .register(&mut tools);
        (cell, tools, Origin { agent, thread })
    });
    app.insert_resource(lince_sandbox::app::CellHandle(cell))
        .insert_resource(Client {
            tools: Arc::new(tools),
            origin,
        });
}

#[derive(Resource)]
struct Activated(String, std::time::Instant);

pub(super) fn click(world: &mut World, action: &str) {
    world.insert_resource(Activated(action.to_owned(), std::time::Instant::now()));
    super::click(world, action);
}

pub(super) fn observe_action(world: &mut World) {
    if let Some(Activated(action, start)) = world.remove_resource::<Activated>() {
        bevy::log::debug!(target: "engine::scene_edits", action, activation_us = start.elapsed().as_micros(), "Native control applied");
    }
}

async fn call(tools: &cell::FioteTools, workspace: u64, query: Request) -> Value {
    let action = engine::actions::Action::Canvas {
        canvas: None,
        request: CanvasRequest::Scene { workspace, query },
    };
    let arguments = json!({"request_id":nucleus::new_uid("invocation"),"action":serde_json::to_string(&action).unwrap(),"read_ids":[]});
    let start = std::time::Instant::now();
    let reply = tools.run("lince_action", arguments).await;
    bevy::log::debug!(target: "engine::scene_edits", tool_us = start.elapsed().as_micros(), "Local Fiote tool returned");
    reply
}

async fn submit(tools: &cell::FioteTools, workspace: u64, edit: Edit) -> Receipt {
    let reply = call(tools, workspace, Request::Submit { edit }).await;
    assert_eq!(reply["ok"], true, "{reply}");
    serde_json::from_value(reply["result"]["data"]["receipt"].clone()).unwrap()
}

async fn fiote(tools: Arc<cell::FioteTools>, workspace: u64, origin: Origin) -> Value {
    let inspected = call(&tools, workspace, Request::Inspect).await;
    assert_eq!(inspected["ok"], true, "{inspected}");
    let inspection: Inspection =
        serde_json::from_value(inspected["result"]["data"]["inspection"].clone()).unwrap();
    assert!(
        inspection.editable
            && inspection.available.place_copy
            && inspection.available.move_placement
    );
    assert_eq!(inspection.placements.len(), 2);
    let create = Edit {
        request_id: nucleus::new_uid("request"),
        document: inspection.document.clone(),
        expected: inspection.revision,
        operation: Operation::PlaceCopy {
            source: inspection.placements[0].handle.clone(),
        },
    };
    let copy = submit(&tools, workspace, create.clone()).await;
    assert_eq!(copy.fiote.as_ref(), Some(&origin));
    assert_eq!(copy.effect, Effect::Created);
    let denied = call(
        &tools,
        1,
        Request::Submit {
            edit: create.clone(),
        },
    )
    .await;
    assert_eq!(denied["ok"], false);
    let mut stale = create.clone();
    stale.request_id = nucleus::new_uid("request");
    let conflict = call(&tools, workspace, Request::Submit { edit: stale }).await;
    assert_eq!(conflict["ok"], false);
    assert!(
        conflict.to_string().contains("revision conflict"),
        "{conflict}"
    );
    let moved = Edit {
        request_id: nucleus::new_uid("request"),
        expected: copy.revision.clone(),
        operation: Operation::Move {
            target: copy.handle.clone(),
            metres: [3, 4, 0].map(DecimalValue::from_integer),
        },
        ..create.clone()
    };
    let changed = submit(&tools, workspace, moved.clone()).await;
    let retrieved = call(
        &tools,
        workspace,
        Request::Receipt {
            document: create.document.clone(),
            request_id: create.request_id.clone(),
        },
    )
    .await;
    assert_eq!(retrieved["result"]["data"]["receipt"], json!(copy));
    assert_eq!(submit(&tools, workspace, create.clone()).await, copy);
    let undo = Edit {
        request_id: nucleus::new_uid("request"),
        expected: changed.revision.clone(),
        operation: Operation::Undo {
            request_id: moved.request_id.clone(),
        },
        ..create
    };
    let restored = submit(&tools, workspace, undo).await;
    let last = submit(
        &tools,
        workspace,
        Edit {
            request_id: nucleus::new_uid("request"),
            expected: restored.revision.clone(),
            ..moved
        },
    )
    .await;
    assert_eq!(last.handle, copy.handle);
    json!({"copy":copy, "move":changed, "undo":restored, "last":last, "denied":denied, "stale":conflict})
}

#[derive(Resource)]
struct Finishing(u32);

#[derive(Resource)]
struct Pending(Mutex<std::sync::mpsc::Receiver<Value>>);

fn text_with(world: &mut World, prefix: &str) -> String {
    world
        .query::<&Text>()
        .iter(world)
        .find(|t| t.0.starts_with(prefix))
        .unwrap_or_else(|| panic!("Missing native text {prefix}"))
        .0
        .clone()
}

fn file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: &Path, json: &Value) {
    fs::write(path, serde_json::to_vec_pretty(json).unwrap()).unwrap();
}

pub(super) fn replay(world: &mut World, frame: u32, directory: &Path, workspace: u64) {
    let path = directory.join(format!("workspaces/{workspace}/scene.workspace"));
    let disk = || workspace_scene::open(&path).unwrap();
    let latest =
        || workspace_scene::acceptance::receipt(&path, &disk().scene.document, None).unwrap();
    let native = directory.join("interrupted-receipt.json");
    let agent = directory.join("fiote-receipts.json");
    if world.resource::<Replay>().recover {
        match frame {
            36 => click(world, "Accepted edits and measurements"),
            40 => {
                assert_eq!(
                    json!(text_with(world, "Distance from origin: 5.385165 m")),
                    file(&directory.join("reopened-generation.json"))
                );
                click(world, "Recover last accepted receipt");
            }
            44 => {
                let receipt = latest();
                assert!(text_with(world, "Local operator ·").contains(&receipt.edit.request_id));
                assert_eq!(json!(receipt), file(&directory.join("native-undo.json")));
                let original: Receipt = serde_json::from_value(file(&native)).unwrap();
                fill(world, "Request UID", &original.edit.request_id);
                click(world, "Find accepted receipt");
            }
            48 => {
                let original: Receipt = serde_json::from_value(file(&native)).unwrap();
                assert!(
                    text_with(world, "Local operator · Created")
                        .contains(original.handle.slot.as_str())
                );
                let copy: Receipt = serde_json::from_value(file(&agent)["copy"].clone()).unwrap();
                fill(world, "Request UID", &copy.edit.request_id);
                click(world, "Find accepted receipt");
            }
            52 => {
                let copy: Receipt = serde_json::from_value(file(&agent)["copy"].clone()).unwrap();
                let visible = text_with(world, "Fiote ");
                assert!(
                    visible.contains(copy.handle.slot.as_str())
                        && visible.contains(&copy.edit.request_id)
                );
                assert_eq!(disk().scene.slots.len(), 3);
                let e = control(world, "Recover last accepted receipt");
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(e, bevy::input_focus::FocusCause::Navigated);
            }
            60 => {
                let (viewport, _, _, visible) =
                    super::control_layout(world, "Recover last accepted receipt");
                write(
                    &directory.join("receipt-layout.json"),
                    &json!({"viewport":viewport.size().to_array(),"control_min":visible.min.to_array(),"control_max":visible.max.to_array()}),
                );
                screenshot(world, directory.join("recovered-receipt.png"));
            }
            _ => {}
        }
        return;
    }
    if let Some(finishing) = world.get_resource::<Finishing>() {
        match frame - finishing.0 {
            4 => {
                let last: Receipt = serde_json::from_value(file(&agent)["last"].clone()).unwrap();
                let restored = latest();
                assert_eq!(restored.fiote, None);
                assert_eq!(restored.handle, last.handle);
                assert_eq!(
                    disk().scene.slot(&last.handle).unwrap().metres,
                    [DecimalValue::from_integer(0); 3]
                );
                write(&directory.join("native-undo.json"), &json!(restored));
                let linked = disk().scene.canvas.unwrap().handle;
                click(world, &format!("Placement {}", linked.slot.as_str()));
            }
            8 => click(world, "Measure distance from origin"),
            12 => {
                write(
                    &directory.join("reopened-generation.json"),
                    &json!(text_with(world, "Distance from origin: 5.385165 m")),
                );
                screenshot(world, directory.join("accepted-edits.png"));
            }
            _ => {}
        }
        return;
    }
    match frame {
        36 => click(world, "Accepted edits and measurements"),
        40 => click(world, "Publish a copy and interrupt its reply"),
        44 => {
            text_with(world, "Reply interrupted after publication");
            let receipt = latest();
            assert_eq!(receipt.effect, Effect::Created);
            assert_eq!(disk().scene.slots.len(), 3);
            assert_ne!(selected(world), receipt.handle.slot.as_str());
            write(&native, &json!(receipt));
            click(world, "Retry the same request");
        }
        48 => {
            let accepted = latest();
            assert_eq!(json!(accepted), file(&native));
            assert_eq!(selected(world), accepted.handle.slot.as_str());
            assert_eq!(disk().revision, accepted.revision);
            click(world, "Undo the accepted edit");
        }
        52 => {
            let scene = disk().scene;
            assert_eq!(scene.slots.len(), 2);
            click(
                world,
                &format!("Placement {}", scene.canvas.unwrap().handle.slot.as_str()),
            );
        }
        56 => click(world, "Hold a distance measurement"),
        60 => {
            fill(world, "X (m)", "6");
            fill(world, "Z (m)", "8");
            click(world, "Publish placement coordinates");
        }
        64 => {
            assert_eq!(inspect(world, "X (m)"), "6");
            click(world, "Measure distance from origin");
        }
        68 => {
            let measured = text_with(world, "Distance from origin: 10.000000 m");
            write(
                &directory.join("distance.json"),
                &json!({"accepted":measured}),
            );
            click(world, "Finish the held measurement");
        }
        72 => {
            text_with(world, "Discarded the late measurement");
            assert_eq!(
                json!(text_with(world, "Distance from origin: 10.000000 m")),
                file(&directory.join("distance.json"))["accepted"]
            );
            click(world, "Undo the accepted edit");
        }
        76 => {
            assert_eq!(inspect(world, "X (m)"), "5");
            assert_eq!(inspect(world, "Z (m)"), "2");
            let client = world.resource::<Client>();
            let (tools, origin) = (client.tools.clone(), client.origin.clone());
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            tokio::spawn(async move {
                sender.send(fiote(tools, workspace, origin).await).unwrap();
            });
            world.insert_resource(Pending(Mutex::new(receiver)));
        }
        _ if frame > 76 && world.contains_resource::<Pending>() => {
            let next = world.resource::<Pending>().0.lock().unwrap().try_recv();
            match next {
                Ok(report) => {
                    world.remove_resource::<Pending>();
                    let last = latest();
                    assert_eq!(json!(last), report["last"]);
                    assert!(text_with(world, "Fiote ").contains(&last.edit.request_id));
                    assert_eq!(selected(world), last.handle.slot.as_str());
                    assert_eq!(
                        (inspect(world, "X (m)"), inspect(world, "Y (m)")),
                        ("3".into(), "4".into())
                    );
                    write(&agent, &report);
                    click(world, "Undo the accepted edit");
                    world.insert_resource(Finishing(frame));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
                Err(e) => panic!("Local Fiote workflow stopped: {e}"),
            }
        }
        _ => {}
    }
}
