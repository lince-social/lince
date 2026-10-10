use bevy::{input_focus::InputFocus, prelude::*};
use lince_desktop::{
    actions::{Action, ActionButton},
    area::InfluenceArea,
    area_panel::AreaAction,
    canvas::CanvasItem,
    canvas_host::Identity,
    container::BoxRoot,
    edit_mode::{EditAction, EditControl},
    workspace::{WorkspaceFile, WorkspaceMember, WorkspacePlugin, Workspaces},
};
use std::path::PathBuf;

fn pick(app: &mut App, root: Entity, entity: Entity) {
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
    let position = app.world().get::<CanvasItem>(entity).unwrap().position;
    {
        let mut view = app
            .world_mut()
            .get_mut::<lince_desktop::canvas::CanvasView>(root)
            .unwrap();
        view.center = position;
        view.zoom = 0.5;
    }
    app.update();
    let screen = app
        .world()
        .get::<UiGlobalTransform>(root)
        .unwrap()
        .translation;
    let camera = app
        .world()
        .resource::<lince_desktop::topology::presentation::SceneCamera>()
        .0;
    let bevy::camera::RenderTarget::Image(target) = app
        .world()
        .get::<bevy::camera::RenderTarget>(camera)
        .unwrap()
    else {
        panic!("missing offscreen target")
    };
    let location = Location {
        target: bevy::camera::NormalizedRenderTarget::Image(target.clone()),
        position: screen,
    };
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<Window>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(screen));
    for action in [
        PointerAction::Move { delta: Vec2::ZERO },
        PointerAction::Press(PointerButton::Primary),
        PointerAction::Release(PointerButton::Primary),
    ] {
        if let PointerAction::Press(_) | PointerAction::Release(_) = &action {
            app.world_mut()
                .write_message(bevy::window::WindowEvent::MouseButtonInput(
                    bevy::input::mouse::MouseButtonInput {
                        button: MouseButton::Left,
                        state: if matches!(&action, PointerAction::Press(_)) {
                            bevy::input::ButtonState::Pressed
                        } else {
                            bevy::input::ButtonState::Released
                        },
                        window,
                    },
                ));
        }
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            action,
        ));
        app.update();
    }
    println!(
        "PICK {entity} at {screen:?} selection={:?} area_selected={:?} inspection={:?}",
        app.world()
            .get::<lince_desktop::canvas_selection::SandSelection>(root)
            .map(|s| &s.0),
        app.world()
            .get::<lince_desktop::area_panel::AreaEditor>(root)
            .and_then(|e| e.selected),
        app.world()
            .get::<lince_desktop::inspection::Inspection>(root)
            .and_then(|i| i.selected)
    );
    println!(
        "ROUTING spatial_root={} view_spatial={:?} ray_hit={:?} pickable={:?}",
        app.world()
            .get::<lince_desktop::topology::presentation::SpatialRoot>(root)
            .is_some(),
        app.world()
            .get::<lince_desktop::topology::view::View>(root)
            .map(|v| v.spatial),
        app.world()
            .resource::<lince_desktop::topology::input::PointerState>()
            .hit,
        app.world().get::<Pickable>(entity)
    );
    if let Some(hits) = app
        .world()
        .resource::<bevy::picking::hover::HoverMap>()
        .get(&PointerId::Mouse)
    {
        for (hit, _) in hits {
            println!(
                "HOVER {hit} text={:?} canvas_item={} parent={:?}",
                app.world().get::<Text>(*hit).map(|t| &t.0),
                app.world().get::<CanvasItem>(*hit).is_some(),
                app.world().get::<ChildOf>(*hit).map(ChildOf::parent)
            );
        }
    }
}

fn menu_delete(world: &mut World, entity: Entity) -> bool {
    let button = world
        .query::<(&lince_desktop::icons::IconButton, &ActionButton)>()
        .iter(world)
        .find(|(icon, button)| {
            icon.icon == lince_desktop::icons::Icon::Delete && button.target == entity
        })
        .map(|(_, button)| button.clone());
    if let Some(button) = button {
        button.actions.run(world, button.target);
        true
    } else {
        false
    }
}

fn output(world: &mut World) {
    let target = bevy::camera::RenderTarget::Image(bevy::camera::ImageRenderTarget {
        handle: world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1440,
                1080,
                bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb,
                None,
            )),
        scale_factor: 1.0,
    });
    let cameras: Vec<_> = world
        .query_filtered::<Entity, With<Camera>>()
        .iter(world)
        .collect();
    for camera in cameras {
        world.entity_mut(camera).insert(target.clone());
    }
}

fn activate(world: &mut World, root: Entity, action: EditAction) -> bool {
    let button = world
        .query::<(&EditControl, &ActionButton)>()
        .iter(world)
        .find(|(control, _)| control.root == root && control.action == action)
        .map(|(_, button)| button.clone());
    if let Some(button) = button {
        button.actions.run(world, button.target);
        true
    } else {
        false
    }
}

fn confirm(world: &mut World, root: Entity) -> bool {
    let button = world
        .query::<(&bevy::a11y::AccessibilityNode, &ActionButton)>()
        .iter(world)
        .find(|(node, button)| node.label() == Some("Delete") && button.target == root)
        .map(|(_, button)| button.clone());
    if let Some(button) = button {
        button.actions.run(world, button.target);
        true
    } else {
        false
    }
}

fn audit() {
    let directory = PathBuf::from(std::env::args().nth(1).expect("copied workspace directory"));
    let mode = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "sequential".into());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let full = std::env::args().any(|arg| arg == "--full");
    let picking = std::env::args().any(|arg| arg == "--picking");
    let keep_renderer = std::env::args().any(|arg| arg == "--keep-renderer");
    let mut app = if full {
        eprintln!("OPEN copied database");
        let engine = runtime
            .block_on(engine::Engine::open(&format!(
                "sqlite://{}",
                directory.join("lince.db").display()
            )))
            .unwrap();
        let engine = std::sync::Arc::new(engine);
        let fiote = runtime
            .block_on(cell::fiote::Host::open(
                engine.clone(),
                directory.join("fiote"),
            ))
            .unwrap();
        eprintln!("OPEN offscreen app");
        let cell = cell::CellRuntime {
            store: engine.store.clone(),
            engine,
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            commands: Default::default(),
            speech: None,
            fiote: Some(std::sync::Arc::new(fiote)),
            information: None,
        };
        let mut app = lince_desktop::app::offscreen_interface_app();
        app.insert_resource(lince_desktop::app::CellHandle(cell))
            .add_plugins(lince_desktop::cell_bridge::CellBridgePlugin)
            .add_systems(PostStartup, output);
        app.world_mut()
            .query::<&mut Window>()
            .single_mut(app.world_mut())
            .unwrap()
            .resolution
            .set(1440.0, 1080.0);
        eprintln!("FINISH offscreen plugins");
        app.finish();
        app.cleanup();
        eprintln!("READY offscreen plugins");
        app
    } else {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<lince_desktop::theme::Typography>()
            .init_resource::<InputFocus>()
            .add_plugins((
                WorkspacePlugin,
                lince_desktop::edit_mode::EditModePlugin,
                lince_desktop::protein_area::ProteinAreaPlugin,
            ));
        app
    };
    app.insert_resource(WorkspaceFile::new(directory.join("interface.json")));
    let root = app.world_mut().spawn(BoxRoot).id();
    eprintln!("RESTORE copied workspace");
    app.update();
    if full {
        if !keep_renderer {
            app.remove_sub_app(bevy::render::RenderApp);
            eprintln!("REPLAY controls with initialized UI and renderer detached");
        }
        for _ in 0..6 {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    EditAction::Open.apply(app.world_mut(), root);
    assert!(activate(app.world_mut(), root, EditAction::Areas));
    let mut targets: Vec<_> = app
        .world_mut()
        .query::<(
            Entity,
            &CanvasItem,
            &ChildOf,
            Option<&InfluenceArea>,
            Option<&Identity>,
        )>()
        .iter(app.world())
        .filter(|(_, _, parent, _, _)| parent.parent() == root)
        .map(|(entity, _, _, area, identity)| {
            let name = area.map_or_else(
                || {
                    if app
                        .world()
                        .get::<lince_desktop::kanban::Kanban>(entity)
                        .is_some()
                    {
                        "Kanban".into()
                    } else if app
                        .world()
                        .get::<lince_desktop::karma_castle::KarmaCastle>(entity)
                        .is_some()
                    {
                        "Karma Castle".into()
                    } else if app
                        .world()
                        .get::<lince_desktop::frequency_castle::FrequencyCastle>(entity)
                        .is_some()
                    {
                        "Frequency Castle".into()
                    } else if let Some(binding) =
                        app.world()
                            .get::<lince_desktop::protein_area::RecordBinding>(entity)
                    {
                        format!("Generated card {} owner={}", binding.uid, binding.area)
                    } else {
                        format!(
                            "{:?}",
                            app.world()
                                .get::<lince_desktop::sand_store::StoredSand>(entity)
                                .map(|sand| sand.kind)
                        )
                    }
                },
                |area| area.name.clone(),
            );
            let id = area
                .map(|area| area.id.clone())
                .or_else(|| identity.map(|id| id.0.clone()))
                .unwrap_or_default();
            (entity, name, id, area.is_some())
        })
        .collect();
    targets.sort_by_key(|(_, name, _, _)| match name.as_str() {
        "Relation Castle" => 0,
        "Kanban" => 1,
        "Some(Organ)" => 2,
        _ => 3,
    });
    println!(
        "RESTORED {} items, workspace {}",
        targets.len(),
        app.world().get::<Workspaces>(root).unwrap().active
    );
    if mode == "inventory" {
        for (entity, name, id, _) in &targets {
            println!(
                "ITEM {name} {id} entity={entity} geometry={:?} pickable={:?} layout={:?}",
                app.world()
                    .get::<CanvasItem>(*entity)
                    .map(|i| (i.position, i.size)),
                app.world().get::<Pickable>(*entity),
                app.world()
                    .get::<lince_desktop::layout::LayoutBox>(*entity)
                    .map(|l| l.parent)
            );
        }
        return;
    }
    for (entity, name, id, area) in targets {
        if mode != "sequential" && mode != id {
            continue;
        }
        if app.world().get_entity(entity).is_err() {
            println!("COMPANION_DELETED {name} {id}");
            continue;
        }
        println!(
            "TRY {name} {id} entity={entity} member={:?} parent={:?} area_valid={:?}",
            app.world().get::<WorkspaceMember>(entity).map(|m| m.0),
            app.world().get::<ChildOf>(entity).map(ChildOf::parent),
            app.world()
                .get::<InfluenceArea>(entity)
                .map(InfluenceArea::validate)
        );
        let before = app
            .world_mut()
            .query_filtered::<Entity, With<CanvasItem>>()
            .iter(app.world())
            .count();
        if picking {
            assert!(full, "picking requires --full");
            pick(&mut app, root, entity);
            println!(
                "MENU_DELETE_CONTROL {}",
                menu_delete(app.world_mut(), entity)
            );
        } else if area {
            let selected = activate(
                app.world_mut(),
                root,
                EditAction::Area(AreaAction::Select(entity)),
            );
            println!("SELECT_CONTROL {selected}");
            if full {
                app.update();
            }
            let removed = activate(app.world_mut(), root, EditAction::Area(AreaAction::Remove));
            println!("REMOVE_CONTROL {removed}");
        } else {
            lince_desktop::canvas_item::DeleteItem.apply(app.world_mut(), entity);
        }
        let confirmed = confirm(app.world_mut(), root);
        println!(
            "CONFIRM {confirmed} REMOVED {}",
            app.world().get_entity(entity).is_err()
        );
        app.update();
        println!(
            "AFTER_UPDATE REMOVED {}",
            app.world().get_entity(entity).is_err()
        );
        let after = app
            .world_mut()
            .query_filtered::<Entity, With<CanvasItem>>()
            .iter(app.world())
            .count();
        println!("ITEM_COUNT {before} -> {after}");
    }
}

fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(audit)
        .unwrap()
        .join()
        .unwrap();
}
