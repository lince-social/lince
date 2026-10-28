use super::*;
use bevy::prelude::{Entity, With};
use lince_interface::{
    host::Direction,
    layout::desktop::{PlacementId, Workspace},
    native::{self, actions::Action},
};

const DRAFT: &str = "Keep this native draft through mixed window movement";
const RECORD: &str = "Retained mixed-row Record";

fn root(canvas: &mut crate::presentation::Presentation) -> Entity {
    let world = canvas.world_mut();
    world
        .query_filtered::<Entity, With<native::workspace::Workspaces>>()
        .single(world)
        .unwrap()
}

fn settle(canvas: &mut crate::presentation::Presentation) {
    for _ in 0..4 {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
    }
}

fn prepare(canvas: &mut crate::presentation::Presentation, spatial: bool) {
    settle(canvas);
    let root = root(canvas);
    assert!(native::workspace_config::set_physics(
        canvas.world_mut(),
        root,
        1,
        false
    ));
    let note = native::sand_store::spawn_sand(
        canvas.world_mut(),
        root,
        1,
        native::sand_store::SandKind::EditableText,
        "Native note",
        bevy::math::DVec2::new(-400.0, 0.0),
    );
    let editor = canvas
        .world_mut()
        .get::<native::sand_store::StoredSand>(note)
        .unwrap()
        .content
        .unwrap();
    canvas
        .world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text(DRAFT);
    let cell = canvas
        .world_mut()
        .resource::<native::app::CellHandle>()
        .clone();
    let uid = tokio::runtime::Handle::current()
        .block_on(cell.0.engine.act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: RECORD.into(),
                body: "Keep the original local Record binding".into(),
                quantity: 7.0,
            },
            None,
        ))
        .unwrap()
        .created
        .unwrap();
    canvas.desktop_action(DesktopAction::Workspace(2)).unwrap();
    native::full_record::open(
        canvas.world_mut(),
        root,
        &uid,
        native::protein_area::Source::Local,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        settle(canvas);
        let world = canvas.world_mut();
        if world
            .query::<(
                &native::protein_area::RecordBinding,
                &native::canvas::CanvasItem,
            )>()
            .iter(world)
            .any(|(binding, _)| binding.uid == uid)
        {
            break;
        }
        assert!(Instant::now() < deadline, "The native Record did not load");
    }
    canvas.desktop_action(DesktopAction::Workspace(1)).unwrap();
    if spatial {
        native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    }
    settle(canvas);
}

pub(super) fn flat(canvas: &mut crate::presentation::Presentation) {
    prepare(canvas, false);
}

pub(super) fn spatial(canvas: &mut crate::presentation::Presentation) {
    prepare(canvas, true);
}

fn document(directory: &Path) -> serde_json::Value {
    let path = std::fs::read_dir(directory.join("interface.snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .max()
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    if let Some(artifacts) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let artifacts = PathBuf::from(artifacts);
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(
            artifacts.join(format!(
                "native-workspace-{}-{}",
                directory.file_name().unwrap().to_string_lossy(),
                path.file_name().unwrap().to_string_lossy()
            )),
            &bytes,
        )
        .unwrap();
    }
    serde_json::from_slice(&bytes).unwrap()
}

fn note(document: &serde_json::Value) -> &serde_json::Value {
    let notes: Vec<_> = document["sands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|sand| sand["kind"] == "EditableText")
        .collect();
    assert_eq!(notes.len(), 1);
    notes[0]
}

fn record(document: &serde_json::Value) -> &serde_json::Value {
    let records: Vec<_> = document["areas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|area| area["area"]["name"] == "Record Castle")
        .collect();
    assert_eq!(records.len(), 1);
    records[0]
}

fn geometry(fixture: &Fixture, id: ItemId) -> ItemGeometry {
    fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .geometry
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .clone()
}

pub(super) fn text_point(fixture: &Fixture, id: ItemId, pixels: &[u8]) -> [f64; 2] {
    let item = geometry(fixture, id);
    let viewport = fixture.viewport();
    pixels
        .chunks_exact(4)
        .enumerate()
        .find_map(|(index, pixel)| {
            if pixel[..3].iter().any(|value| *value < 200) {
                return None;
            }
            let point = viewport.raster.logical_point([
                (index % fixture.size[0] as usize) as f64 + 0.5,
                (index / fixture.size[0] as usize) as f64 + 0.5,
            ]);
            item.surface_point(point).is_some().then_some(point)
        })
        .expect("The native draft has no visible text pixels")
}

fn verify_restored(fixture: &Fixture, note_id: [u8; 16], uid: &str, area_id: &str) {
    let mut canvas = crate::presentation::Presentation::new_with_cell(
        fixture.size,
        fixture.directory.join("interface-assets"),
        WakeSignal::new(|| {}),
        Some((fixture.cell.clone(), fixture.directory.clone())),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        settle(&mut canvas);
        let world = canvas.world_mut();
        let notes: Vec<_> = world
            .query::<(&native::layout::LayoutBox, &native::sand_store::StoredSand)>()
            .iter(world)
            .filter(|(layout, _)| layout.id == note_id)
            .map(|(_, stored)| (stored.kind, stored.content.unwrap()))
            .collect();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, native::sand_store::SandKind::EditableText);
        assert_eq!(
            native::sand_text::value(world, notes[0].1),
            format!("{DRAFT}x")
        );
        let binding = world
            .query::<(
                &native::protein_area::RecordBinding,
                &native::canvas::CanvasItem,
            )>()
            .iter(world)
            .find(|(binding, _)| binding.uid == uid);
        if let Some((binding, _)) = binding {
            assert_eq!(binding.source, native::protein_area::Source::Local);
            assert_eq!(
                world
                    .get::<native::area::InfluenceArea>(binding.area)
                    .unwrap()
                    .id,
                area_id
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The original native Record binding was not restored"
        );
    }
    canvas.finish().unwrap();
}

fn exercise(spatial: bool) {
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
            &directory
                .path()
                .join(format!("mixed-{spatial}-{}", size[0])),
            size,
            Some(if spatial { self::spatial } else { flat }),
        );
        let initial_document = document(&fixture.directory);
        assert_eq!(note(&initial_document)["texts"][0]["text"], DRAFT);
        let note_id = serde_json::from_value::<[u8; 16]>(
            note(&initial_document)["placement"]["layout"]["id"].clone(),
        )
        .unwrap();
        let binding = record(&initial_document);
        let uid = binding["area"]["protein"]["draft"]["query"]["where"][0]["all"][0]["uid_eq"]
            .as_str()
            .unwrap()
            .to_owned();
        let area_id = binding["area"]["id"].as_str().unwrap().to_owned();
        assert!(binding["area"]["records"].get(&uid).is_some());
        fixture.activate(2);
        fixture.action(DesktopAction::Move(Direction::Down));
        fixture.activate(1);
        fixture.action(DesktopAction::Move(Direction::Down));
        fixture.activate(0);
        fixture.action(DesktopAction::Focus(Direction::Left));
        fixture.configure();
        let id = ItemId::Native(note_id);
        assert_eq!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .focused,
            Some(id)
        );
        let identities = fixture.identities();
        let anchor = fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .pointer_targets
            .iter()
            .find(|target| target.item == ItemId::Application(fixture.clients[0].1))
            .unwrap()
            .placement;
        let sizes = fixture.sizes();
        for commit in [false, true] {
            let name = |phase: &str| format!("native-tiled-{phase}-{spatial}-{commit}");
            let original = geometry(&fixture, id);
            assert_eq!(original.mapping.corners.is_some(), spatial);
            let painted = fixture.render(&name("initial"));
            let start = if commit {
                text_point(&fixture, id, &painted)
            } else {
                interior(&original, fixture.viewport().bounds).unwrap()
            };
            assert!(fixture.viewport().bounds.contains(start));
            assert!(original.surface_point(start).is_some());
            fixture.harness.client.pointer_buttons.clear();
            fixture.desktop.move_pointer(start, 1).unwrap();
            assert!(matches!(
                fixture.desktop.target(),
                crate::desktop::input::Target::Native
            ));
            fixture.desktop.key(125, true, 2).unwrap();
            fixture.desktop.button(272, true, 3).unwrap();
            fixture.flush();
            fixture.desktop.key(125, false, 4).unwrap();
            fixture.flush();
            assert_eq!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .manipulation
                    .unwrap()
                    .target
                    .item,
                id
            );
            let source = geometry(&fixture, id);
            let targets = fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .pointer_targets
                .clone();
            let baseline = fixture.render(&name("grabbed"));
            fixture
                .desktop
                .move_pointer(fixture.surface_point(0, [0.51, 0.5]), 5)
                .unwrap();
            fixture.flush();
            fixture.configure();
            assert_eq!(geometry(&fixture, id), source);
            assert_eq!(fixture.sizes(), sizes);
            assert_eq!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .pointer_targets,
                targets
            );
            let preview = fixture.render(&name("destination"));
            assert!(
                accent(&preview) > accent(&baseline) + 100,
                "The native tiled destination is not visible"
            );
            assert!(fixture.harness.client.pointer_buttons.is_empty());
            if !commit {
                fixture.desktop.key(1, true, 6).unwrap();
                fixture.desktop.key(1, false, 7).unwrap();
                fixture.flush();
            }
            fixture.desktop.button(272, false, 8).unwrap();
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
                    .is_none()
            );
            assert!(!fixture.desktop.server.manipulation_grabbed());
            let completed = fixture.render(&name("finished"));
            assert!(
                accent(&completed) < accent(&preview),
                "The destination outline survived the gesture"
            );
            if commit {
                assert!(
                    geometry(&fixture, id).mapping.canvas.origin()[0]
                        > fixture.geometry(0).mapping.canvas.origin()[0]
                );
            } else {
                assert_eq!(geometry(&fixture, id), original);
            }
            assert_eq!(fixture.sizes(), sizes);
            fixture.sync();
            assert!(fixture.harness.client.pointer_buttons.is_empty());
        }
        let original = geometry(&fixture, id);
        let mapping = original.mapping;
        let logical = std::array::from_fn(|axis| {
            mapping.surface.origin()[axis] + mapping.surface.size()[axis] * [0.9, 0.5][axis]
        });
        let start = mapping.canvas_point(logical).unwrap();
        assert!(fixture.viewport().bounds.contains(start));
        fixture.desktop.move_pointer(start, 11).unwrap();
        fixture.desktop.key(125, true, 12).unwrap();
        fixture.desktop.button(273, true, 13).unwrap();
        fixture.flush();
        fixture.desktop.key(125, false, 14).unwrap();
        fixture.flush();
        assert_eq!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .unwrap()
                .target
                .item,
            id
        );
        assert_eq!(geometry(&fixture, id), original);
        let destination = mapping
            .canvas_point([logical[0] + 8.0, logical[1]])
            .unwrap();
        assert!(fixture.viewport().bounds.contains(destination));
        fixture.desktop.move_pointer(destination, 15).unwrap();
        fixture.flush();
        fixture.configure();
        fixture.render(&format!("native-tiled-width-{spatial}"));
        let resized = geometry(&fixture, id).mapping.surface.size();
        assert!((resized[0] - mapping.surface.size()[0] - 8.0).abs() < 0.01);
        assert_eq!(resized[1], mapping.surface.size()[1]);
        assert_eq!(fixture.sizes(), sizes);
        assert_eq!(fixture.resizing(), [false; 3]);
        fixture.desktop.key(1, true, 16).unwrap();
        fixture.desktop.key(1, false, 17).unwrap();
        fixture.desktop.button(273, false, 18).unwrap();
        fixture.flush();
        fixture.configure();
        fixture.render(&format!("native-tiled-width-cancel-{spatial}"));
        assert_eq!(geometry(&fixture, id), original);
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        let painted = fixture.render(&format!("native-tiled-normal-{spatial}"));
        let point = text_point(&fixture, id, &painted);
        fixture.desktop.move_pointer(point, 19).unwrap();
        fixture.flush();
        fixture.render(&format!("native-tiled-hover-{spatial}"));
        fixture.desktop.button(272, true, 20).unwrap();
        fixture.flush();
        fixture.desktop.button(272, false, 21).unwrap();
        fixture.flush();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard,
            "Clicking the visible draft did not focus its native editor"
        );
        let editor = fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref()
            .expect("The captured native text editor must publish its IME endpoint");
        let editor_owner = editor.owner;
        assert_ne!(editor.projection, 0);
        assert!(editor.caret[2] > 0 && editor.caret[3] > 0);
        assert!(fixture.viewport().bounds.contains([f64::from(editor.caret[0]), f64::from(editor.caret[1])]));
        for (code, pressed) in [
            (29, true),
            (107, true),
            (107, false),
            (29, false),
            (45, true),
            (45, false),
        ] {
            fixture.desktop.key(code, pressed, 22).unwrap();
            for _ in 0..4 {
                fixture.desktop.dispatch().unwrap();
                fixture.flush();
            }
        }
        super::editor::compose(&mut fixture, editor_owner, &format!("{DRAFT}x"));
        fixture.normal_context_clicks(0);
        assert_eq!(fixture.identities(), identities);
        fixture.desktop.worker.stop().unwrap();
        let saved = document(&fixture.directory);
        assert_eq!(note(&saved)["texts"][0]["text"], format!("{DRAFT}x"));
        assert_eq!(saved["sands"].as_array().unwrap().len(), 1);
        assert_eq!(
            note(&saved)["placement"]["layout"]["id"],
            serde_json::to_value(note_id).unwrap()
        );
        let restored = record(&saved);
        assert_eq!(restored["area"]["id"], area_id);
        assert_eq!(restored["area"]["protein"], binding["area"]["protein"]);
        assert!(restored["area"]["records"].get(&uid).is_some());
        let model: Workspace =
            serde_json::from_value(saved["workspaces"][0]["desktop"].clone()).unwrap();
        assert_eq!(model.items().len(), 4);
        let row = model
            .tiled_rows()
            .into_iter()
            .find(|row| row.contains(&PlacementId(note_id)))
            .unwrap();
        assert_eq!(row, vec![anchor, PlacementId(note_id)]);
        verify_restored(&fixture, note_id, &uid, &area_id);
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn flat_native_tiles_keep_one_destination_drafts_bindings_and_normal_client_input() {
    exercise(false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn spatial_native_tiles_keep_one_destination_drafts_bindings_and_normal_client_input() {
    exercise(true);
}
