use super::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Resource, Default, Debug)]
struct UiEvents {
    presses: Vec<(Entity, String)>,
    activated: Vec<Entity>,
}

fn provider(runtime: &tokio::runtime::Runtime) -> CellHandle {
    let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
    with_engine(engine)
}

fn persisted_provider(runtime: &tokio::runtime::Runtime, data: &Path) -> CellHandle {
    std::fs::create_dir_all(data).unwrap();
    let url = format!("sqlite://{}", data.join("cell.sqlite").display());
    with_engine(Arc::new(
        runtime.block_on(engine::Engine::open(&url)).unwrap(),
    ))
}

fn with_engine(engine: Arc<engine::Engine>) -> CellHandle {
    CellHandle(cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    })
}

fn until(canvas: &mut Presentation, mut done: impl FnMut(&Presentation) -> bool) -> Frame {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let frame = settle(canvas);
        if done(canvas) {
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "The connected presentation did not settle"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn saved(
    canvas: &mut Presentation,
    data: &Path,
    previous: Option<&Path>,
    workspace_name: &str,
) -> (PathBuf, serde_json::Value) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        settle(canvas);
        let latest = match std::fs::read_dir(data.join("interface.snapshots")) {
            Ok(entries) => entries
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .max(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("Could not inspect ordinary snapshots: {error}"),
        };
        if let Some(path) = latest.filter(|path| Some(path.as_path()) != previous) {
            let bytes = std::fs::read(&path).unwrap();
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            for name in ["sands", "areas", "records"] {
                if let Some(rows) = value[name].as_array_mut() {
                    rows.sort_by_key(|row| row.to_string());
                }
            }
            if value["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|workspace| workspace["id"] == 1 && workspace["name"] == workspace_name)
            {
                if let Some(artifacts) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
                    let artifacts = PathBuf::from(artifacts);
                    std::fs::create_dir_all(&artifacts).unwrap();
                    let name = format!(
                        "autosave-{}-{}.json",
                        data.file_name().unwrap().to_string_lossy(),
                        workspace_name.replace(' ', "-")
                    );
                    std::fs::write(
                        artifacts.join(name),
                        serde_json::to_vec_pretty(&value).unwrap(),
                    )
                    .unwrap();
                }
                return (path, value);
            }
        }
        assert!(
            Instant::now() < deadline,
            "Ordinary autosave did not publish a new snapshot"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assertion_rows(
    runtime: &tokio::runtime::Runtime,
    cell: &CellHandle,
    uid: &str,
) -> Vec<serde_json::Value> {
    let mut draft = native::protein_castle::ProteinDraft::default();
    draft.query = serde_json::json!({"source":"assertion","where":[{"concept_in":"park-preview-property"}],"limit":512});
    let replies = runtime.block_on(
        cell.0
            .local_session()
            .handle(cell::ClientMessage::Subscribe {
                id: "preview-owner-assertions".into(),
                protein: draft.compile().unwrap(),
            }),
    );
    let rows = replies
        .into_iter()
        .find_map(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            cell::ServerMessage::Error { message, .. } => panic!("{message}"),
            _ => None,
        })
        .unwrap();
    assert!(rows.len() < 512);
    let mut rows: Vec<_> = rows
        .into_iter()
        .filter(|row| row["subject"] == uid)
        .collect();
    rows.sort_by_key(|row| row["uid"].to_string());
    rows
}

fn record_rows(runtime: &tokio::runtime::Runtime, cell: &CellHandle) -> Vec<serde_json::Value> {
    let replies = runtime.block_on(
        cell.0
            .local_session()
            .handle(native::cell_bridge::records_subscription()),
    );
    replies
        .into_iter()
        .find_map(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            cell::ServerMessage::Error { message, .. } => panic!("{message}"),
            _ => None,
        })
        .unwrap()
}

fn uids(rows: &[serde_json::Value]) -> std::collections::BTreeSet<String> {
    rows.iter()
        .map(|row| row["uid"].as_str().unwrap().to_owned())
        .collect()
}

fn stop(
    runtime: &tokio::runtime::Runtime,
    canvas: &mut Presentation,
) -> tokio::sync::mpsc::Sender<cell::ClientMessage> {
    let mut bridge = canvas
        .world_mut()
        .non_send_mut::<native::cell_bridge::CellBridge>();
    let sender = bridge.outgoing.clone();
    bridge.incoming.close();
    drop(bridge);
    runtime.block_on(async {
        sender
            .send(native::cell_bridge::records_subscription())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), sender.closed())
            .await
            .unwrap();
    });
    sender
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn connected_cell_autosaves_committed_geometry_and_recovers_during_window_previews() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 700];
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let data = directory.path().join(format!("preview-{mode:?}"));
        let cell = persisted_provider(&runtime, &data);
        let mut canvas = Presentation::new_with_cell(
            size,
            data.join("assets"),
            WakeSignal::new(|| {}),
            Some((cell.clone(), data.clone())),
        )
        .unwrap();
        canvas
            .world_mut()
            .insert_resource(native::notifications::Notifications::new(
                cell::Diagnostics::default(),
            ));
        canvas.world_mut().insert_resource(
            native::workspace::WorkspaceFile::with_settings(
                data.join("interface.json"),
                cell::InterfaceStorage {
                    snapshot_seconds: 1,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        assert!(native::workspace_config::set_physics(
            canvas.world_mut(),
            root,
            1,
            false
        ));
        let center = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center;
        let mut identities = Vec::new();
        for (index, offset) in [-180.0, 180.0].into_iter().enumerate() {
            let surface = SurfaceId(700 + index as u64);
            let identity = canvas
                .open_window(surface, "Private client title", "private.client")
                .unwrap();
            map(&mut canvas, surface, [230, 20, 240, 255]);
            canvas.window_request(
                surface,
                lince_interface::host::WindowRequest {
                    maximized: Some(false),
                    ..Default::default()
                },
            );
            settle(&mut canvas);
            let entity = canvas.windows[&surface].entity;
            canvas
                .world_mut()
                .entity_mut(entity)
                .insert(native::canvas::CanvasItem {
                    position: center + bevy::math::DVec2::new(offset, 0.0),
                    size: Vec2::splat(96.0),
                });
            canvas
                .world_mut()
                .get_mut::<native::layout::LayoutBox>(entity)
                .unwrap()
                .rules = lince_interface::layout::Rules::fixed([96.0; 2]);
            settle(&mut canvas);
            identities.push((surface, identity, entity));
        }
        let (surface, identity, target) = identities[0];
        let (_, anchor_id, _) = identities[1];
        let mut tokens = native::tokens::TokenOverrides::default();
        tokens.0.insert(
            native::tokens::Token::SandBackground,
            native::tokens::TokenValue::Color([12, 34, 56, 255]),
        );
        native::token_style::set_overrides(canvas.world_mut(), target, tokens);
        let note = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            1,
            native::sand_store::SandKind::EditableText,
            "Retained preview note",
            center + bevy::math::DVec2::new(0.0, 250.0),
        );
        let editor = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(note)
            .unwrap()
            .content
            .unwrap();
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text("A draft retained through a provider interruption");
        let mut area = native::area::InfluenceArea::new(
            native::area::AreaShape::Square,
            center + bevy::math::DVec2::splat(1000.0),
            bevy::math::DVec2::splat(100.0),
        );
        area.enabled = false;
        let area = native::area::spawn_area(canvas.world_mut(), root, 1, area).unwrap();
        let area_before = canvas
            .app
            .world()
            .get::<native::area::InfluenceArea>(area)
            .unwrap()
            .clone();
        canvas
            .desktop_action(DesktopAction::SetLayout(mode))
            .unwrap();
        canvas
            .window_launch_description(
                surface,
                serde_json::from_value(
                    serde_json::json!({"program":"test-generic","arguments":["literal argument"]}),
                )
                .unwrap(),
            )
            .unwrap();
        until(&mut canvas, |canvas| {
            identities.iter().all(|(_, id, _)| {
                canvas
                    .records
                    .as_ref()
                    .unwrap()
                    .profile(*id)
                    .is_some_and(|saved| saved.uid.is_some())
            })
        });
        let uid = canvas
            .records
            .as_ref()
            .unwrap()
            .profile(identity)
            .unwrap()
            .uid
            .clone()
            .unwrap();
        runtime.block_on(async {
            cell.0
                .engine
                .act(
                    engine::actions::Action::CreateConcept {
                        lingua: "g_local".into(),
                        name: "park-preview-property".into(),
                        parents: vec![],
                    },
                    None,
                )
                .await
                .unwrap();
            cell.0
                .engine
                .act(
                    engine::actions::Action::AssertRecord {
                        subject: uid.clone(),
                        predicate: "park-preview-property".into(),
                        object: None,
                        quantity: Some("7".into()),
                        unit: None,
                    },
                    None,
                )
                .await
                .unwrap();
        });
        let assertions = assertion_rows(&runtime, &cell, &uid);
        assert!(assertions.iter().any(|row| row["quantity"] == "7"));
        canvas
            .desktop_action(DesktopAction::SelectPlacement {
                workspace: 1,
                item: Some(identity.0),
            })
            .unwrap();
        settle(&mut canvas);
        assert!(native::workspace::rename(
            canvas.world_mut(),
            root,
            "Preview baseline"
        ));
        let (before_path, mut before) = saved(&mut canvas, &data, None, "Preview baseline");
        let original = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        let profiles: Vec<_> = identities
            .iter()
            .map(|(_, id, _)| {
                serde_json::to_value(canvas.records.as_ref().unwrap().profile(*id).unwrap())
                    .unwrap()
            })
            .collect();
        let model = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .clone();
        let operation = if mode == WorkspaceLayout::Free {
            Manipulation::Resize
        } else {
            Manipulation::Move
        };
        assert!(manipulation::begin(
            canvas.world_mut(),
            root,
            target,
            operation,
            [1.0; 2]
        ));
        let slot = (mode == WorkspaceLayout::Tiled).then_some(
            lince_interface::layout::desktop::DropSlot {
                anchor: anchor_id,
                side: lince_interface::host::Direction::Right,
            },
        );
        assert!(manipulation::propose(
            canvas.world_mut(),
            [64.0, 30.0],
            slot
        ));
        let preview = settle(&mut canvas);
        capture(&format!("provider-preview-{mode:?}"), size, &preview);
        colored(&preview, size, mapping(&preview, surface), [0.5; 2]);
        assert!(manipulation::active(canvas.app.world()));
        let old = stop(&runtime, &mut canvas);
        assert!(native::workspace::rename(
            canvas.world_mut(),
            root,
            "Saved during preview"
        ));
        let (_, during) = saved(
            &mut canvas,
            &data,
            Some(&before_path),
            "Saved during preview",
        );
        assert!(manipulation::active(canvas.app.world()));
        before["workspaces"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|workspace| workspace["id"] == 1)
            .unwrap()["name"] = "Saved during preview".into();
        before["canvas_state"]["snapshot"]["workspaces"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|workspace| workspace["id"] == 1)
            .unwrap()["name"] = "Saved during preview".into();
        let revision = before["canvas_state"]["snapshot"]["revision"]
            .as_u64()
            .unwrap();
        before["canvas_state"]["snapshot"]["revision"] = (revision + 1).into();
        assert_eq!(during, before);
        runtime
            .block_on(cell.0.engine.act(
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: Some("Owner title after reconnect".into()),
                    body: None,
                },
                None,
            ))
            .unwrap();
        assert!(native::cell_bridge::reconnect(canvas.world_mut()).unwrap());
        assert!(
            !old.same_channel(
                &canvas
                    .app
                    .world()
                    .non_send::<native::cell_bridge::CellBridge>()
                    .outgoing
            )
        );
        until(&mut canvas, |canvas| {
            canvas
                .app
                .world()
                .get::<native::area::RecordProperties>(target)
                .is_some_and(|properties| properties.0["head"] == "Owner title after reconnect")
        });
        assert!(manipulation::active(canvas.app.world()));
        let item = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        assert_eq!(
            (item.position, item.size),
            (original.position, original.size)
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries[0]
                .desktop,
            model
        );
        for ((_, id, _), expected) in identities.iter().zip(&profiles) {
            assert_eq!(
                &serde_json::to_value(canvas.records.as_ref().unwrap().profile(*id).unwrap())
                    .unwrap(),
                expected
            );
        }
        assert_eq!(
            canvas.app.world().get::<native::area::InfluenceArea>(area),
            Some(&area_before)
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "A draft retained through a provider interruption"
        );
        assert_eq!(assertion_rows(&runtime, &cell, &uid), assertions);
        let note_layout = *canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(note)
            .unwrap();
        let known_records = uids(&record_rows(&runtime, &cell));
        let profile_path = data.join("park-windows.json");
        let profile_bytes = std::fs::read(&profile_path).unwrap();
        let disk: Vec<serde_json::Value> = serde_json::from_slice(&profile_bytes).unwrap();
        for expected in &profiles {
            assert_eq!(
                disk.iter()
                    .find(|row| row["layout"]["id"] == expected["layout"]["id"]),
                Some(expected)
            );
        }
        let old_engine = Arc::downgrade(&cell.0.engine);
        assert!(manipulation::active(canvas.app.world()));
        drop(preview);
        drop(canvas);
        drop(cell);
        runtime.block_on(async {
            let deadline = Instant::now() + Duration::from_secs(15);
            while old_engine.strong_count() != 0 {
                assert!(
                    Instant::now() < deadline,
                    "The old presentation retained its Cell Engine after disposal"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
        assert_eq!(std::fs::read(&profile_path).unwrap(), profile_bytes);
        let cell = persisted_provider(&runtime, &data);
        let rows = record_rows(&runtime, &cell);
        assert_eq!(uids(&rows), known_records);
        assert_eq!(
            rows.iter().find(|row| row["uid"] == uid).unwrap()["head"],
            "Owner title after reconnect"
        );
        assert_eq!(assertion_rows(&runtime, &cell, &uid), assertions);
        let mut reopened = Presentation::new_with_cell(
            size,
            data.join("assets"),
            WakeSignal::new(|| {}),
            Some((cell.clone(), data.clone())),
        )
        .unwrap();
        let frame = settle(&mut reopened);
        let root = reopened.canvas_root().unwrap();
        assert!(!manipulation::active(reopened.app.world()));
        assert!(reopened.windows.is_empty());
        assert!(frame.windows.is_empty());
        assert!(
            frame
                .geometry
                .as_ref()
                .unwrap()
                .items
                .iter()
                .all(|item| !matches!(item.id, ItemId::Application(_)))
        );
        let spaces = reopened
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap();
        assert_eq!(spaces.entries[0].desktop.mode(), mode);
        assert_eq!(spaces.entries[0].name, "Saved during preview");
        assert!(
            spaces.entries[0]
                .desktop
                .items()
                .iter()
                .all(|item| identities.iter().all(|(_, id, _)| item.id != *id))
        );
        let mut notes = reopened.world_mut().query::<(
            Entity,
            &native::layout::LayoutBox,
            &native::sand_store::StoredSand,
        )>();
        let (note, layout, stored) = notes
            .iter(reopened.app.world())
            .find(|(_, layout, _)| layout.id == note_layout.id)
            .unwrap();
        assert_eq!(layout.rules, note_layout.rules);
        assert_eq!(stored.kind, native::sand_store::SandKind::EditableText);
        let editor = stored.content.unwrap();
        assert_eq!(
            reopened
                .app
                .world()
                .get::<native::workspace::WorkspaceMember>(note)
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            reopened
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "A draft retained through a provider interruption"
        );
        let mut areas = reopened.world_mut().query::<&native::area::InfluenceArea>();
        assert_eq!(
            areas
                .iter(reopened.app.world())
                .find(|area| area.id == area_before.id),
            Some(&area_before)
        );
        for ((_, id, _), expected) in identities.iter().zip(&profiles) {
            assert_eq!(
                &serde_json::to_value(reopened.records.as_ref().unwrap().profile(*id).unwrap())
                    .unwrap(),
                expected
            );
        }
        assert_eq!(uids(&record_rows(&runtime, &cell)), known_records);
        capture(&format!("provider-reopened-{mode:?}"), size, &frame);
        reopened.finish().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn visible_configuration_refresh_recovers_the_local_cell_and_keeps_native_drafts() {
    use super::super::shortcuts::named_button;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let data = directory.path().join(format!("refresh-{}", size[0]));
        let cell = provider(&runtime);
        let mut canvas = Presentation::new_with_cell(
            size,
            data.join("assets"),
            WakeSignal::new(|| {}),
            Some((cell.clone(), data)),
        )
        .unwrap();
        canvas
            .world_mut()
            .insert_resource(native::notifications::Notifications::new(
                cell::Diagnostics::default(),
            ));
        canvas
            .app
            .init_resource::<UiEvents>()
            .add_observer(
                |event: On<Pointer<Press>>,
                 labels: Query<&bevy::a11y::AccessibilityNode>,
                 mut events: ResMut<UiEvents>| {
                    if event.entity == event.original_event_target() {
                        let label = labels
                            .get(event.entity)
                            .ok()
                            .and_then(|node| node.label())
                            .unwrap_or("")
                            .to_owned();
                        events.presses.push((event.entity, label));
                    }
                },
            )
            .add_observer(
                |event: On<bevy::ui_widgets::Activate>, mut events: ResMut<UiEvents>| {
                    events.activated.push(event.entity);
                },
            );
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        assert!(native::workspace_config::set_physics(
            canvas.world_mut(),
            root,
            1,
            false
        ));
        let center = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center;
        let note = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            1,
            native::sand_store::SandKind::EditableText,
            "Retained native draft",
            center + bevy::math::DVec2::new(2000.0, 0.0),
        );
        let draft = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(note)
            .unwrap()
            .content
            .unwrap();
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(draft)
            .unwrap()
            .editor
            .set_text("Keep this unrelated draft");
        let owner = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            1,
            native::sand_store::SandKind::Configuration,
            "Cell settings",
            center,
        );
        settle(&mut canvas);
        canvas
            .world_mut()
            .get_mut::<native::canvas::CanvasItem>(owner)
            .unwrap()
            .size = Vec2::new(size[0] as f32 - 30.0, 580.0);
        canvas
            .world_mut()
            .get_mut::<native::layout::LayoutBox>(owner)
            .unwrap()
            .rules = lince_interface::layout::Rules::fixed([size[0] as f32 - 30.0, 580.0]);
        canvas
            .world_mut()
            .entity_mut(root)
            .insert(native::canvas_selection::SandSelection(vec![owner]));
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        canvas.desktop_action(DesktopAction::ToggleFocus).unwrap();
        settle(&mut canvas);
        let old = stop(&runtime, &mut canvas);
        let mut texts = canvas.world_mut().query::<&Text>();
        let ready = until(&mut canvas, |canvas| {
            texts
                .iter(canvas.app.world())
                .any(|text| text.0 == lince_interface::i18n::Token::LoadedSettings.text())
        });
        capture(&format!("provider-refresh-ready-{}", size[0]), size, &ready);
        let refresh = named_button(&mut canvas, "Refresh");
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::actions::ActionButton>(refresh)
                .unwrap()
                .target,
            owner
        );
        let position = native::topology::presentation::bounds(canvas.app.world(), refresh)
            .map(|bounds| bounds.center())
            .unwrap_or_else(|| {
                canvas
                    .app
                    .world()
                    .get::<UiGlobalTransform>(refresh)
                    .unwrap()
                    .transform_point2(Vec2::ZERO)
            });
        assert!(
            Rect::from_corners(Vec2::ZERO, UVec2::from_array(size).as_vec2()).contains(position)
        );
        canvas.pointer(position.to_array()).unwrap();
        settle(&mut canvas);
        canvas.button(MouseButton::Left, true).unwrap();
        settle(&mut canvas);
        canvas.button(MouseButton::Left, false).unwrap();
        let clicked = settle(&mut canvas);
        capture(
            &format!("provider-refresh-click-{}", size[0]),
            size,
            &clicked,
        );
        eprintln!(
            "Refresh {size:?}: control={refresh:?}, position={position:?}, modal={}, keyboard_modal={}, events={:?}",
            clicked.input.modal,
            clicked.input.keyboard_modal,
            canvas.app.world().resource::<UiEvents>()
        );
        let frame = until(&mut canvas, |canvas| {
            !canvas
                .app
                .world()
                .non_send::<native::cell_bridge::CellBridge>()
                .outgoing
                .is_closed()
        });
        assert!(
            !old.same_channel(
                &canvas
                    .app
                    .world()
                    .non_send::<native::cell_bridge::CellBridge>()
                    .outgoing
            )
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<UiEvents>()
                .activated
                .iter()
                .filter(|entity| **entity == refresh)
                .count(),
            1
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(draft)
                .unwrap()
                .value()
                .to_string(),
            "Keep this unrelated draft"
        );
        capture(&format!("provider-refresh-{}", size[0]), size, &frame);
        canvas.finish().unwrap();
    }
}
