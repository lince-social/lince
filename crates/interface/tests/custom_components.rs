#![cfg(feature = "native-runtime")]

use bevy::{math::DVec2, prelude::*, text::EditableText};
use lince_interface::{
    actions::{Action, ActionButton},
    canvas_selection::SandSelection,
    edit_mode::{EditAction, EditModePlugin},
    icons::Tooltip,
    sand_store::{SandKind, StoredSand},
    workspace::{WorkspaceFile, WorkspacePlugin},
};
use std::sync::Arc;

fn fixture(engine: Arc<engine::Engine>, path: &std::path::Path) -> (App, Entity) {
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<lince_interface::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .insert_resource(lince_interface::notifications::Notifications::new(
            cell::Diagnostics::default(),
        ))
        .insert_resource(lince_interface::wake::WakeSignal::new(|| {}))
        .insert_resource(lince_interface::app::CellHandle(runtime))
        .insert_resource(WorkspaceFile::new(path.join("interface.json")))
        .add_plugins((
            WorkspacePlugin,
            EditModePlugin,
            lince_interface::cell_bridge::CellBridgePlugin,
        ));
    let root = app
        .world_mut()
        .spawn(lince_interface::container::BoxRoot)
        .id();
    app.update();
    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Store.apply(app.world_mut(), root);
    (app, root)
}

fn activate(app: &mut App, root: Entity, caption: &str) {
    let world = app.world_mut();
    let entity = world
        .query::<(Entity, &ActionButton, &Tooltip)>()
        .iter(world)
        .find(|(_, button, tip)| button.target == root && tip.0 == caption)
        .unwrap()
        .0;
    world.trigger(bevy::ui_widgets::Activate { entity });
    app.update();
}

fn name(app: &mut App, value: &str) {
    let world = app.world_mut();
    let entity = world
        .query::<(Entity, &EditableText, &Tooltip)>()
        .iter(world)
        .find(|(_, _, tip)| tip.0 == "Component name")
        .unwrap()
        .0;
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

async fn settle(app: &mut App, caption: &str) {
    for _ in 0..2000 {
        app.update();
        if app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == caption)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let labels: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    panic!("Missing {caption}: {labels:?}");
}

#[tokio::test]
async fn organ_store_controls_create_edit_delete_and_add_independent_canvas_copies() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(engine.clone(), directory.path());
    settle(&mut app, "Components loaded").await;
    let note = lince_interface::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Original",
        DVec2::ZERO,
    );
    app.world_mut()
        .entity_mut(root)
        .insert(SandSelection(vec![note]));
    name(&mut app, "Team board");
    activate(&mut app, root, "Save selection as new component");
    settle(&mut app, "Component ready").await;
    activate(&mut app, root, "Add to canvas");
    let copy = app.world().get::<SandSelection>(root).unwrap().0[0];
    assert_ne!(copy, note);
    assert_eq!(
        lince_interface::sand_text::snapshot(app.world(), copy)[0].text,
        "Original"
    );
    name(&mut app, "Updated board");
    activate(&mut app, root, "Rename");
    settle(&mut app, "Selected: Updated board").await;
    settle(&mut app, "Component ready").await;
    let replacement = lince_interface::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "Replacement",
        DVec2::ZERO,
    );
    app.world_mut()
        .entity_mut(root)
        .insert(SandSelection(vec![replacement]));
    activate(&mut app, root, "Replace with selection");
    settle(&mut app, "Component ready").await;
    assert_eq!(
        lince_interface::sand_text::snapshot(app.world(), copy)[0].text,
        "Original"
    );
    let second_directory = tempfile::tempdir().unwrap();
    let (mut reopened, second_root) = fixture(engine, second_directory.path());
    settle(&mut reopened, "Updated board").await;
    activate(&mut reopened, second_root, "Updated board");
    settle(&mut reopened, "Component ready").await;
    activate(&mut reopened, second_root, "Add to canvas");
    let second_copy = reopened
        .world()
        .get::<SandSelection>(second_root)
        .unwrap()
        .0[0];
    assert_eq!(
        lince_interface::sand_text::snapshot(reopened.world(), second_copy)[0].text,
        "Replacement"
    );
    activate(&mut app, root, "Delete component");
    settle(&mut app, "No components in this Organ.").await;
    assert!(app.world().get::<StoredSand>(copy).is_some());
    assert!(reopened.world().get::<StoredSand>(second_copy).is_some());
}
