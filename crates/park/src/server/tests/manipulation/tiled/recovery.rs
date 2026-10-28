use super::*;
use bevy::prelude::{Children, ComputedNode, Entity, InheritedVisibility, Text, Vec2, With};
use lince_interface::{
    layout::desktop::{Item, PlacementId},
    native::{self as ui, canvas::CanvasItem},
};
use std::collections::BTreeSet;

const PROPERTY: &str = "park-live-recovery-property";
const RECOVERED: &str = "Owner title received after reconnect";

pub(super) fn inspect<T: Send + 'static>(
    fixture: &mut Fixture,
    inspect: impl FnOnce(&mut crate::presentation::Presentation) -> T + Send + 'static,
) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let serial = fixture
        .desktop
        .worker
        .input(Input::Test(Box::new(move |canvas| {
            sender
                .send(inspect(canvas))
                .unwrap_or_else(|_| panic!("The inspection receiver closed"));
        })))
        .unwrap();
    advance(&mut fixture.desktop, serial);
    receiver.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn root(canvas: &mut crate::presentation::Presentation) -> Entity {
    let world = canvas.world_mut();
    world
        .query_filtered::<Entity, With<ui::workspace::Workspaces>>()
        .single(world)
        .unwrap()
}

#[derive(Debug, PartialEq)]
struct State {
    items: Vec<Item>,
    rows: Vec<Vec<PlacementId>>,
    note: String,
    note_id: [u8; 16],
    bindings: BTreeMap<SurfaceId, String>,
    native: BTreeSet<(String, String)>,
}

fn state(canvas: &mut crate::presentation::Presentation) -> State {
    let root = root(canvas);
    let world = canvas.world_mut();
    let spaces = world.get::<ui::workspace::Workspaces>(root).unwrap();
    let model = &spaces
        .entries
        .iter()
        .find(|space| space.id == 1)
        .unwrap()
        .desktop;
    let items = model.items().to_vec();
    let rows = model.tiled_rows();
    let notes: Vec<_> = world
        .query::<(&ui::sand_store::StoredSand, &ui::layout::LayoutBox)>()
        .iter(world)
        .filter(|(sand, _)| sand.kind == ui::sand_store::SandKind::EditableText)
        .map(|(sand, layout)| (sand.content.unwrap(), layout.id))
        .collect();
    assert_eq!(notes.len(), 1);
    let note = ui::sand_text::value(world, notes[0].0);
    let bindings = world
        .query::<(
            &ui::layout::desktop::ApplicationWindow,
            &ui::protein_area::RecordBinding,
        )>()
        .iter(world)
        .map(|(window, binding)| {
            assert_eq!(binding.source, ui::protein_area::Source::Local);
            (window.surface, binding.uid.clone())
        })
        .collect();
    let native = world
        .query::<(
            &ui::protein_area::RecordBinding,
            &CanvasItem,
            Option<&ui::layout::desktop::ApplicationWindow>,
        )>()
        .iter(world)
        .filter(|(_, _, application)| application.is_none())
        .map(|(binding, _, _)| {
            assert_eq!(binding.source, ui::protein_area::Source::Local);
            (
                binding.uid.clone(),
                world
                    .get::<ui::area::InfluenceArea>(binding.area)
                    .unwrap()
                    .id
                    .clone(),
            )
        })
        .collect();
    State {
        items,
        rows,
        note,
        note_id: notes[0].1,
        bindings,
        native,
    }
}

fn setup(fixture: &mut Fixture) -> PlacementId {
    let data = fixture.directory.clone();
    let size = fixture.size;
    inspect(fixture, move |canvas| {
        canvas
            .world_mut()
            .insert_resource(ui::notifications::Notifications::new(
                cell::Diagnostics::default(),
            ));
        canvas.world_mut().insert_resource(
            ui::workspace::WorkspaceFile::with_settings(
                data.join("interface.json"),
                cell::InterfaceStorage {
                    snapshot_seconds: 1,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        canvas.desktop_action(DesktopAction::Workspace(3)).unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        let root = root(canvas);
        for workspace in [2, 3] {
            assert!(ui::workspace_config::set_physics(
                canvas.world_mut(),
                root,
                workspace,
                false
            ));
        }
        let center = canvas
            .world_mut()
            .get::<ui::canvas::CanvasView>(root)
            .unwrap()
            .center;
        let owner = ui::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            3,
            ui::sand_store::SandKind::Configuration,
            "Recovery settings",
            center,
        );
        for _ in 0..4 {
            canvas.probe_frame().unwrap().wait_for_probe().unwrap();
        }
        canvas
            .world_mut()
            .get_mut::<CanvasItem>(owner)
            .unwrap()
            .size = Vec2::new(size[0] as f32 - 30.0, 580.0);
        let mut layout = canvas
            .world_mut()
            .get_mut::<ui::layout::LayoutBox>(owner)
            .unwrap();
        layout.rules = lince_interface::layout::Rules::fixed([size[0] as f32 - 30.0, 580.0]);
        PlacementId(layout.id)
    })
}

fn profiles(fixture: &Fixture) -> serde_json::Value {
    let mut rows: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(fixture.directory.join("park-windows.json")).unwrap(),
    )
    .unwrap();
    rows.sort_by_key(|row| row["uid"].to_string());
    rows.into()
}

fn saved(
    fixture: &mut Fixture,
    previous: Option<&Path>,
    label: &str,
) -> (PathBuf, serde_json::Value) {
    let label_owned = label.to_owned();
    inspect(fixture, move |canvas| {
        let root = root(canvas);
        assert!(ui::workspace::rename(
            canvas.world_mut(),
            root,
            &label_owned
        ));
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        fixture.desktop.dispatch().unwrap();
        fixture.flush();
        let path = std::fs::read_dir(fixture.directory.join("interface.snapshots"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .max();
        if let Some(path) = path.filter(|path| Some(path.as_path()) != previous) {
            let bytes = std::fs::read(&path).unwrap();
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            for field in ["sands", "areas", "records"] {
                if let Some(rows) = value[field].as_array_mut() {
                    rows.sort_by_key(serde_json::Value::to_string);
                }
            }
            if value["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|space| space["id"] == 1 && space["name"] == label)
            {
                if let Some(artifacts) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
                    let artifacts = PathBuf::from(artifacts);
                    std::fs::create_dir_all(&artifacts).unwrap();
                    std::fs::write(
                        artifacts.join(format!("{label}-{}.json", fixture.size[0])),
                        &bytes,
                    )
                    .unwrap();
                }
                return (path, value);
            }
        }
        assert!(
            Instant::now() < deadline,
            "The real-client preview was not autosaved"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn model(value: &serde_json::Value) -> lince_interface::layout::desktop::Workspace {
    serde_json::from_value(
        value["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|space| space["id"] == 1)
            .unwrap()["desktop"]
            .clone(),
    )
    .unwrap()
}

fn query(
    runtime: &tokio::runtime::Runtime,
    fixture: &Fixture,
    message: cell::ClientMessage,
) -> Vec<serde_json::Value> {
    runtime
        .block_on(fixture.cell.0.local_session().handle(message))
        .into_iter()
        .find_map(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            cell::ServerMessage::Error { message, .. } => panic!("{message}"),
            _ => None,
        })
        .unwrap()
}

fn records(runtime: &tokio::runtime::Runtime, fixture: &Fixture) -> BTreeSet<String> {
    query(runtime, fixture, ui::cell_bridge::records_subscription())
        .into_iter()
        .map(|row| row["uid"].as_str().unwrap().to_owned())
        .collect()
}

fn assertions(runtime: &tokio::runtime::Runtime, fixture: &Fixture) -> Vec<serde_json::Value> {
    let mut draft = ui::protein_castle::ProteinDraft::default();
    draft.query =
        serde_json::json!({"source":"assertion", "where":[{"concept_in":PROPERTY}],"limit":512});
    let mut rows = query(
        runtime,
        fixture,
        cell::ClientMessage::Subscribe {
            id: "live-recovery-assertions".into(),
            protein: draft.compile().unwrap(),
        },
    );
    assert!(rows.len() < 512);
    rows.sort_by_key(|row| row["uid"].to_string());
    rows
}

fn refresh_point(fixture: &mut Fixture) -> Option<[f64; 2]> {
    let point = inspect(fixture, |canvas| {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &Children, &ui::actions::ActionButton)>()
            .iter(world)
            .find(|(entity, children, button)| {
                world
                    .get::<ui::sand_store::StoredSand>(button.target)
                    .is_some_and(|sand| sand.kind == ui::sand_store::SandKind::Configuration)
                    && world
                        .get::<InheritedVisibility>(*entity)
                        .is_some_and(|visible| visible.get())
                    && world
                        .get::<ComputedNode>(*entity)
                        .is_some_and(|node| node.size().min_element() > 0.0)
                    && children.iter().any(|child| {
                        world
                            .get::<Text>(*child)
                            .is_some_and(|text| text.0 == "Refresh")
                    })
            })
            .and_then(|(entity, _, _)| ui::topology::presentation::bounds(world, entity))
            .map(|bounds| bounds.center().to_array())
    });
    point.map(|point| {
        fixture
            .viewport()
            .raster
            .logical_point(point.map(f64::from))
    })
}

fn exercise(mode: WorkspaceLayout) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut fixture = Fixture::prepared(
            &runtime,
            &directory.path().join(format!("cell-{mode:?}-{}", size[0])),
            size,
            Some(super::native::flat),
        );
        let settings = setup(&mut fixture);
        fixture.action(DesktopAction::Workspace(1));
        fixture.action(DesktopAction::SetLayout(mode));
        let index = if mode == WorkspaceLayout::Free { 2 } else { 0 };
        fixture.activate(index);
        if mode == WorkspaceLayout::Free {
            fixture.action(DesktopAction::ToggleFocus);
        }
        fixture.configure();
        let identities = fixture.identities();
        let initial = inspect(&mut fixture, state);
        assert_eq!(initial.bindings.len(), 3);
        assert_eq!(initial.native.len(), 1);
        let surface = fixture.clients[index].1;
        let uid = initial.bindings[&surface].clone();
        runtime.block_on(async {
            fixture
                .cell
                .0
                .engine
                .act(
                    engine::actions::Action::CreateConcept {
                        lingua: "g_local".into(),
                        name: PROPERTY.into(),
                        parents: vec![],
                    },
                    None,
                )
                .await
                .unwrap();
            fixture
                .cell
                .0
                .engine
                .act(
                    engine::actions::Action::AssertRecord {
                        subject: uid.clone(),
                        predicate: PROPERTY.into(),
                        object: None,
                        quantity: Some("7".into()),
                        unit: None,
                    },
                    None,
                )
                .await
                .unwrap();
        });
        let before_assertions = assertions(&runtime, &fixture);
        assert_eq!(before_assertions.len(), 1);
        let before_records = records(&runtime, &fixture);
        let name = |stage: &str| format!("cell-{mode:?}-{stage}");
        let (before_path, before_document) = saved(&mut fixture, None, &name("baseline"));
        let before_profiles = profiles(&fixture);
        let before_sizes = fixture.sizes();
        let before_pixels = fixture.render(&name("before"));
        let button = if mode == WorkspaceLayout::Free {
            let start = fixture.begin_modifier_resize(index, [0.9, 0.5]);
            fixture.resize_delta(index, start, [8.0, 0.0]);
            273
        } else {
            fixture
                .desktop
                .move_pointer(fixture.point(index, [0.3, 0.5]), 1)
                .unwrap();
            fixture.desktop.key(125, true, 2).unwrap();
            fixture.desktop.button(272, true, 3).unwrap();
            fixture.desktop.key(125, false, 4).unwrap();
            fixture.flush();
            let destination = fixture.point(1, [0.0, 0.5]);
            let anchor = fixture.geometry(1).mapping;
            let local = anchor.surface_point(destination).unwrap();
            assert!(local[0] < anchor.surface.origin()[0] + anchor.surface.size()[0] * 0.5);
            assert!(fixture.viewport().bounds.contains(destination));
            fixture.desktop.move_pointer(destination, 5).unwrap();
            272
        };
        fixture.flush();
        fixture.configure();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_some()
        );
        let preview_pixels = fixture.render(&name("preview"));
        if mode == WorkspaceLayout::Free {
            assert_eq!(fixture.sizes()[index][0], before_sizes[index][0] + 8);
        } else {
            assert_eq!(fixture.sizes(), before_sizes);
            assert!(accent(&preview_pixels) > accent(&before_pixels) + 100);
        }
        assert_eq!(inspect(&mut fixture, state), initial);
        let (_, preview) = saved(&mut fixture, Some(&before_path), &name("preview-save"));
        assert_eq!(model(&preview).items(), model(&before_document).items());
        assert_eq!(
            model(&preview).tiled_rows(),
            model(&before_document).tiled_rows()
        );
        assert_eq!(preview["sands"], before_document["sands"]);
        assert_eq!(preview["areas"], before_document["areas"]);
        assert_eq!(profiles(&fixture), before_profiles);
        let old = inspect(&mut fixture, |canvas| {
            let mut bridge = canvas
                .world_mut()
                .non_send_mut::<ui::cell_bridge::CellBridge>();
            let outgoing = bridge.outgoing.clone();
            bridge.incoming.close();
            outgoing
        });
        runtime.block_on(async {
            if old
                .send(ui::cell_bridge::records_subscription())
                .await
                .is_err()
            {
                assert!(old.is_closed());
            }
            tokio::time::timeout(Duration::from_secs(5), old.closed())
                .await
                .unwrap();
            fixture
                .cell
                .0
                .engine
                .act(
                    engine::actions::Action::EditRecordText {
                        target: uid.clone(),
                        head: Some(RECOVERED.into()),
                        body: None,
                    },
                    None,
                )
                .await
                .unwrap();
        });
        fixture.flush();
        assert_eq!(inspect(&mut fixture, state), initial);
        let notices = inspect(&mut fixture, |canvas| {
            canvas
                .world_mut()
                .resource::<ui::notifications::Notifications>()
                .log
                .snapshot()
                .1
        });
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].source, "interface::connection");
        assert_eq!(notices[0].occurrences, 1);
        fixture.desktop.key(1, true, 8).unwrap();
        fixture.desktop.key(1, false, 9).unwrap();
        fixture.desktop.button(button, false, 10).unwrap();
        fixture.flush();
        fixture.configure();
        assert_eq!(fixture.sizes(), before_sizes);
        fixture.action(DesktopAction::SelectPlacement {
            workspace: 3,
            item: Some(settings.0),
        });
        fixture.action(DesktopAction::ToggleFocus);
        fixture.configure();
        let deadline = Instant::now() + Duration::from_secs(15);
        let point = loop {
            fixture.desktop.dispatch().unwrap();
            fixture.flush();
            if let Some(point) = refresh_point(&mut fixture) {
                break point;
            }
            assert!(
                Instant::now() < deadline,
                "The measured Refresh control was unavailable"
            );
        };
        assert!(fixture.viewport().bounds.contains(point));
        fixture.render(&name("refresh-ready"));
        fixture.desktop.move_pointer(point, 11).unwrap();
        fixture.flush();
        fixture.desktop.button(272, true, 12).unwrap();
        fixture.flush();
        fixture.desktop.button(272, false, 13).unwrap();
        fixture.flush();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let old = old.clone();
            if inspect(&mut fixture, move |canvas| {
                let outgoing = &canvas
                    .world_mut()
                    .non_send::<ui::cell_bridge::CellBridge>()
                    .outgoing;
                !outgoing.is_closed() && !outgoing.same_channel(&old)
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "The visible Refresh did not recover the local channel"
            );
            fixture.desktop.dispatch().unwrap();
        }
        fixture.render(&name("refresh-complete"));
        fixture.action(DesktopAction::Workspace(1));
        fixture.activate(index);
        fixture.configure();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if inspect(&mut fixture, move |canvas| {
                let world = canvas.world_mut();
                world
                    .query::<(
                        &ui::layout::desktop::ApplicationWindow,
                        &ui::area::RecordProperties,
                    )>()
                    .iter(world)
                    .any(|(window, properties)| {
                        window.surface == surface && properties.0["head"] == RECOVERED
                    })
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Recovered window subscriptions did not deliver the owner edit"
            );
        }
        assert_eq!(inspect(&mut fixture, state), initial);
        assert_eq!(fixture.identities(), identities);
        assert_eq!(fixture.sizes(), before_sizes);
        assert_eq!(profiles(&fixture), before_profiles);
        assert_eq!(assertions(&runtime, &fixture), before_assertions);
        assert_eq!(records(&runtime, &fixture), before_records);
        assert!(
            fixture
                .desktop
                .server
                .windows()
                .iter()
                .all(|window| window.mapped)
        );
        assert_eq!(fixture.desktop.server.windows().len(), 3);
        fixture.render(&name("recovered"));
        fixture.normal_context_clicks(index);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn live_clients_autosave_committed_free_geometry_and_recover_through_visible_refresh() {
    exercise(WorkspaceLayout::Free);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn live_clients_autosave_committed_tiles_and_recover_through_visible_refresh() {
    exercise(WorkspaceLayout::Tiled);
}
