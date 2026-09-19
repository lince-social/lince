use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    area::{AreaShape, InfluenceArea, spawn_area},
    protein_area::{Binding, Config, GroupAxis, OverflowMode, RecordBinding},
};
use std::sync::Arc;

#[derive(Resource)]
struct Capture {
    path: String,
    phase: u8,
    frame: u32,
    forms: bool,
    groups: bool,
    pull: bool,
    effects: bool,
    targets: bool,
    growth: Option<(Entity, String)>,
}

fn main() {
    let path = std::env::args().nth(1).expect("screenshot path");
    let forms = std::env::args().nth(2).as_deref() == Some("forms");
    let groups = std::env::args().nth(2).as_deref() == Some("groups");
    let effects = std::env::args().nth(2).as_deref() == Some("effects");
    let targets = std::env::args().nth(2).as_deref() == Some("targets");
    let pull = std::env::args().nth(2).as_deref() == Some("filter") || effects || targets;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let engine = Arc::new(runtime.block_on(async {
        let engine = engine::Engine::open_memory().await.unwrap();
        let mut people = std::collections::HashMap::<&str, String>::new();
        if groups {
            engine.act(engine::actions::Action::CreateConcept { lingua: "g_local".into(), name: "assigned-to".into(), parents: vec![] }, None).await.unwrap();
        }
        for (head, body, quantity) in [
            ("Plan the community garden", "Find a place, gather the neighbors, and list what we can share.", 3.0),
            ("Repair workshop", "Bring tools and things that need care. Everyone can teach something and learn something.", -2.0),
            ("Meet our neighbors", "An open afternoon with food, stories and ideas for the neighborhood.", 1.0),
        ] {
            let uid = engine.act(engine::actions::Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Plain, head: head.into(), body: body.into(), quantity }, None).await.unwrap().created.unwrap();
            if groups {
                let person = if quantity < 0.0 { "Bea" } else { "Alex" };
                let assignee = if let Some(uid) = people.get(person) { uid.clone() } else {
                    let uid = engine.act(engine::actions::Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Person, head: person.into(), body: String::new(), quantity: 1.0 }, None).await.unwrap().created.unwrap();
                    people.insert(person, uid.clone());
                    uid
                };
                engine.act(engine::actions::Action::AssertRecord { subject: uid.clone(), predicate: "assigned-to".into(), object: Some(assignee), quantity: None, unit: None }, None).await.unwrap();
                engine.act(engine::actions::Action::SetExtension { target: uid, namespace: "work".into(), fds: serde_json::json!({"due":if quantity == 1.0 {"2026-09-30"} else {"2026-09-15"}}) }, None).await.unwrap();
            }
        }
        engine
    }));
    let cell = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let directory = tempfile::tempdir().unwrap();
    let mut app = lince_interface::app::interface_app();
    app.insert_resource(lince_interface::app::CellHandle(cell))
        .insert_resource(lince_interface::workspace::WorkspaceFile::new(
            directory.path().join("interface.json"),
        ))
        .add_plugins(lince_interface::cell_bridge::CellBridgePlugin)
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Capture {
            path,
            phase: 0,
            frame: 0,
            forms,
            groups,
            pull,
            effects,
            targets,
            growth: None,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, capture);
    app.run();
}

fn setup(world: &mut World) {
    let root = world.spawn(lince_interface::container::BoxRoot).id();
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .resolution
        .set(1400.0, 950.0);
    let mut config = Config {
        enabled: true,
        width: 320.0,
        columns: 3,
        ..default()
    };
    config.draft.query["where"] = serde_json::json!([{"all":[{"kind_eq":"plain"}]}]);
    config.bindings[0].overflow = OverflowMode::GrowDown;
    config.bindings[1].overflow = OverflowMode::GrowDown;
    config.bindings.push(Binding::new("quantity_exact"));
    if world.resource::<Capture>().groups {
        config.width = 260.0;
        config.columns = 1;
        config.bindings = vec![Binding::new("head"), Binding::new("body")];
        config.bindings[0].overflow = OverflowMode::GrowDown;
        config.grouping.horizontal = Some(GroupAxis::new("due_date"));
        config.grouping.vertical = Some(GroupAxis::new("assignees"));
    }
    if world.resource::<Capture>().forms {
        config.bindings = ["head", "assignees", "work_logs"]
            .into_iter()
            .map(|key| {
                let mut binding = Binding::new(key);
                binding.editable = true;
                binding.height = if key == "head" { 40.0 } else { 240.0 };
                binding.overflow = OverflowMode::GrowDown;
                binding
            })
            .collect();
    }
    let mut area = InfluenceArea::new(
        AreaShape::Square,
        DVec2::new(0.0, 240.0),
        DVec2::splat(1040.0),
    );
    area.name = "Community projects".into();
    area.protein = Some(config);
    spawn_area(world, root, 1, area).unwrap();
    if world.resource::<Capture>().pull {
        let mut area = InfluenceArea::new(
            AreaShape::Circle,
            DVec2::new(0.0, 240.0),
            DVec2::splat(1040.0),
        );
        area.name = "Needs".into();
        area.strength = 100.0;
        area.reach.mode = lince_interface::area::ReachMode::Unlimited;
        let mut filter = Config {
            enabled: true,
            bindings: vec![],
            ..default()
        };
        filter.draft.query["where"] =
            serde_json::json!([{"all":[{"quantity_lt":"0"},{"not":{"kind_eq":"person"}}]}]);
        if world.resource::<Capture>().effects {
            area.name = "Sorted projects".into();
            area.center = [0.0; 2];
            area.size = [1600.0; 2];
            area.strength = 0.0;
            area.scale = 0.65;
            area.sorting = Some(Default::default());
            filter.draft.query["where"] = serde_json::json!([{"all":[{"kind_eq":"plain"}]}]);
            filter.draft.query["order"] = serde_json::json!([{"asc":"quantity"}]);
        }
        area.filter = Some(filter);
        if world.resource::<Capture>().targets {
            area.name = "Separate target".into();
            area.center = [-180.0, -100.0];
            area.size = [260.0; 2];
            area.depth = 75.0;
            area.target = lince_interface::area::AttractionTarget::Point([100.0, 30.0]);
            area.reach.mode = lince_interface::area::ReachMode::Limited;
            area.reach.radius = 80.0;
            area.force_mode = lince_interface::area_effects::ForceMode::Newtonian;
        }
        spawn_area(world, root, 1, area).unwrap();
    }
}

fn capture(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    assert!(frame < 600, "Area failed to render");
    let rows = world
        .query::<(&RecordBinding, &lince_interface::canvas::CanvasItem)>()
        .iter(world)
        .count();
    if frame == 30 {
        world
            .query::<&mut Window>()
            .single_mut(world)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(20.0, 20.0)));
        if world.resource::<Capture>().forms {
            let editor = world
                .query::<(Entity, &bevy::text::EditableText, &RecordBinding)>()
                .iter(world)
                .next()
                .map(|(entity, text, _)| (entity, text.value().to_string()));
            if let Some((entity, _)) = editor.as_ref() {
                world
                    .get_mut::<bevy::text::EditableText>(*entity)
                    .unwrap()
                    .editor
                    .set_text(&"Growing text ".repeat(80));
            }
            world.resource_mut::<Capture>().growth = editor;
        }
    }
    if frame == 50 {
        if let Some((entity, value)) = world.resource_mut::<Capture>().growth.take() {
            assert!(
                world.get::<ComputedNode>(entity).unwrap().size().y > 200.0,
                "Editable text did not grow"
            );
            world
                .get_mut::<bevy::text::EditableText>(entity)
                .unwrap()
                .editor
                .set_text(&value);
        }
    }
    if frame > 70 && rows == 3 && world.resource::<Capture>().phase == 0 {
        for (node, parent) in world
            .query_filtered::<(&ComputedNode, &ChildOf), With<bevy::text::EditableText>>()
            .iter(world)
        {
            if world.get::<RecordBinding>(parent.parent()).is_some() {
                assert!(
                    node.size().min_element() > 0.0,
                    "Record input has no visible size"
                );
            }
        }
        for (text, node, parent) in world
            .query::<(&Text, &ComputedNode, &ChildOf)>()
            .iter(world)
        {
            if !text.0.is_empty() && world.get::<RecordBinding>(parent.parent()).is_some() {
                assert!(
                    node.size().min_element() > 0.0,
                    "Record text has no visible size"
                );
            }
        }
        world.resource_mut::<Capture>().phase = 1;
        let path = world.resource::<Capture>().path.clone();
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(|_: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                capture.phase = 2;
            });
    }
    if world.resource::<Capture>().phase == 2 {
        let root = world
            .query_filtered::<Entity, With<lince_interface::container::BoxRoot>>()
            .single(world)
            .unwrap();
        let owner = world
            .query::<(Entity, &InfluenceArea)>()
            .iter(world)
            .find(|(_, area)| {
                if world.resource::<Capture>().pull {
                    area.filter.is_some()
                } else {
                    area.protein.is_some()
                }
            })
            .map(|(entity, _)| entity)
            .unwrap();
        lince_interface::edit_mode::EditAction::Open.apply(world, root);
        lince_interface::edit_mode::EditAction::Areas.apply(world, root);
        lince_interface::edit_mode::EditAction::Area(
            lince_interface::area_panel::AreaAction::Select(owner),
        )
        .apply(world, root);
        if world.resource::<Capture>().effects {
            let panel = world
                .get::<lince_interface::edit_mode::EditMode>(root)
                .unwrap()
                .panel;
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 520.0;
        }
        world.resource_mut::<Capture>().phase = 3;
        world.resource_mut::<Capture>().frame = frame;
    }
    if world.resource::<Capture>().phase == 3 && world.resource::<Capture>().targets {
        let offset = frame - world.resource::<Capture>().frame;
        let root = world
            .query_filtered::<Entity, With<lince_interface::container::BoxRoot>>()
            .single(world)
            .unwrap();
        let owner = world
            .get::<lince_interface::area_panel::AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let window = world
            .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
            .single(world)
            .unwrap();
        let view = world
            .get::<lince_interface::canvas::CanvasView>(root)
            .unwrap();
        let computed = world.get::<ComputedNode>(root).unwrap();
        let center = world.get::<UiGlobalTransform>(root).unwrap().translation
            * computed.inverse_scale_factor();
        let start = center + ((DVec2::new(-80.0, -70.0) - view.center) * view.zoom).as_vec2();
        if offset == 3 {
            let panel = world
                .get::<lince_interface::edit_mode::EditMode>(root)
                .unwrap()
                .panel;
            let heading = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(_, t)| t.0 == "Target")
                .unwrap()
                .0;
            let y = world
                .get::<UiGlobalTransform>(heading)
                .unwrap()
                .translation
                .y
                * world
                    .get::<ComputedNode>(heading)
                    .unwrap()
                    .inverse_scale_factor();
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = (y - 250.0).max(0.0);
        }
        if offset == 5 || offset == 11 {
            world.write_message(bevy::window::WindowEvent::CursorMoved(
                bevy::window::CursorMoved {
                    window,
                    position: start
                        + if offset == 11 {
                            Vec2::new(70.0, -40.0)
                        } else {
                            Vec2::ZERO
                        },
                    delta: None,
                },
            ));
        }
        if offset == 8 || offset == 14 {
            world.write_message(bevy::window::WindowEvent::MouseButtonInput(
                bevy::input::mouse::MouseButtonInput {
                    window,
                    button: MouseButton::Left,
                    state: if offset == 8 {
                        bevy::input::ButtonState::Pressed
                    } else {
                        bevy::input::ButtonState::Released
                    },
                },
            ));
        }
        if offset == 18 {
            let area = world.get::<InfluenceArea>(owner).unwrap();
            assert_eq!(area.center, [-180.0, -100.0]);
            assert_eq!(area.size, [260.0; 2]);
            assert_eq!(area.depth, 75.0);
            assert_eq!(area.reach.radius, 80.0);
            assert_eq!(
                area.target,
                lince_interface::area::AttractionTarget::Point([170.0, -10.0])
            );
            println!(
                "Area target smoke: actual pointer dragging changed only the target; boundaries, reach and depth stayed unchanged."
            );
            world.write_message(bevy::window::WindowEvent::CursorMoved(
                bevy::window::CursorMoved {
                    window,
                    position: Vec2::new(20.0, 20.0),
                    delta: None,
                },
            ));
        }
    }
    if world.resource::<Capture>().phase == 3 && frame > world.resource::<Capture>().frame + 20 {
        world.resource_mut::<Capture>().phase = 4;
        let path = format!("{}.editor.png", world.resource::<Capture>().path);
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>,
                 mut capture: ResMut<Capture>,
                 mut exit: MessageWriter<AppExit>| {
                    if capture.pull && !capture.targets {
                        capture.phase = 5;
                    } else {
                        exit.write(AppExit::Success);
                    }
                },
            );
    }
    if world.resource::<Capture>().phase == 5 {
        let button = world
            .query::<(
                &lince_interface::icons::Tooltip,
                &lince_interface::actions::ActionButton,
            )>()
            .iter(world)
            .find(|(tooltip, _)| {
                tooltip.0 == "Edit the query in a Protein Castle; changes return to this Area"
            })
            .map(|(_, button)| button.clone())
            .unwrap();
        button.actions.run(world, button.target);
        world.resource_mut::<Capture>().phase = 6;
        world.resource_mut::<Capture>().frame = frame;
    }
    if world.resource::<Capture>().phase == 6 && frame > world.resource::<Capture>().frame + 20 {
        assert_eq!(
            world
                .query::<&lince_interface::protein_castle::ProteinCastle>()
                .iter(world)
                .count(),
            1
        );
        assert!(
            !world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "Aggregate" || text.0 == "Fields")
        );
        world.resource_mut::<Capture>().phase = 7;
        let path = format!("{}.query.png", world.resource::<Capture>().path);
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
    }
}
