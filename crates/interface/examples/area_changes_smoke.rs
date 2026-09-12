use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    app::connected_app,
    area::{AreaShape, InfluenceArea, Property, PropertyRule, RecordProperties},
    area_panel::AreaAction,
    canvas::CanvasItem,
    edit_mode::{EditAction, EditControl, EditMode},
};
use std::sync::Arc;

#[derive(Resource)]
struct Exercise {
    uid: String,
    path: String,
    stage: u8,
    area: Option<Entity>,
    wait: u8,
}

fn activate(world: &mut World, action: EditAction) {
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, control)| control.action == action)
        .unwrap()
        .0;
    world.trigger(Activate { entity });
}

fn exercise(world: &mut World) {
    if world.resource::<FrameCount>().0 < 12 {
        return;
    }
    let uid = world.resource::<Exercise>().uid.clone();
    let Some(sand) = world
        .query::<(Entity, &RecordProperties)>()
        .iter(world)
        .find(|(_, record)| record.0["uid"].as_str() == Some(&uid))
        .map(|(entity, _)| entity)
    else {
        return;
    };
    let root = world.get::<ChildOf>(sand).unwrap().parent();
    let quantity = world.get::<RecordProperties>(sand).unwrap().0["quantity"]
        .as_f64()
        .unwrap();
    match world.resource::<Exercise>().stage {
        0 => {
            world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(-300.0, 0.0);
            let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(400.0));
            area.name = "Work in progress".into();
            area.rules.push(PropertyRule {
                property: Property::Quantity,
                value: "0".into(),
            });
            area.changes.enter.quantity = Some("-3".into());
            area.changes.enter.assert.push("working".into());
            area.changes.leave.quantity = Some("1".into());
            area.changes.leave.retract.push("working".into());
            let area = lince_interface::area::spawn_area(world, root, 1, area).unwrap();
            world.resource_mut::<Exercise>().area = Some(area);
            activate(world, EditAction::Toggle);
            world.resource_mut::<Exercise>().stage = 1;
        }
        1 => {
            activate(world, EditAction::Areas);
            world.resource_mut::<Exercise>().stage = 2;
        }
        2 => {
            let area = world.resource::<Exercise>().area.unwrap();
            activate(world, EditAction::Area(AreaAction::Select(area)));
            world.resource_mut::<Exercise>().stage = 3;
        }
        3 => {
            activate(world, EditAction::Area(AreaAction::PreviewChanges));
            assert!(!lince_interface::area_mutation::armed(
                world,
                world.resource::<Exercise>().area.unwrap()
            ));
            assert_eq!(quantity, 0.0);
            world.resource_mut::<Exercise>().stage = 4;
        }
        4 => {
            activate(world, EditAction::Area(AreaAction::ArmChanges));
            world.resource_mut::<Exercise>().stage = 5;
        }
        5 => {
            assert!(lince_interface::area_mutation::armed(
                world,
                world.resource::<Exercise>().area.unwrap()
            ));
            assert_eq!(quantity, 0.0);
            world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
            world.resource_mut::<Exercise>().stage = 6;
        }
        6 if quantity == -3.0 => {
            world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(-300.0, 0.0);
            world.resource_mut::<Exercise>().stage = 7;
        }
        7 if quantity == 1.0 => {
            let disarm = world
                .query::<(&EditControl, &Node)>()
                .iter(world)
                .find(|(control, _)| control.action == EditAction::DisarmAreaChanges)
                .unwrap()
                .1;
            assert_eq!(disarm.display, Display::Flex);
            activate(world, EditAction::Toggle);
            world.resource_mut::<Exercise>().stage = 8;
        }
        8 => {
            assert!(!world.get::<EditMode>(root).unwrap().enabled);
            activate(world, EditAction::DisarmAreaChanges);
            world.resource_mut::<Exercise>().stage = 9;
        }
        9 => {
            assert!(!lince_interface::area_mutation::armed(
                world,
                world.resource::<Exercise>().area.unwrap()
            ));
            activate(world, EditAction::Toggle);
            world.resource_mut::<Exercise>().stage = 10;
        }
        10 => {
            let panel = world.get::<EditMode>(root).unwrap().panel;
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 590.0;
            world.resource_mut::<Exercise>().stage = 11;
        }
        11 => {
            world.resource_mut::<Exercise>().wait += 1;
            if world.resource::<Exercise>().wait < 20 {
                return;
            }
            let path = world.resource::<Exercise>().path.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|capture: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                assert!(capture.image.data.as_ref().unwrap().chunks_exact(4).any(|pixel| pixel[0] > 40 && pixel[1] > 40 && pixel[2] > 40));
                println!("Area changes smoke passed: preview, explicit grant, real entry/exit changes, and Disarm outside Edit mode.");
                exit.write(AppExit::Success);
            });
            world.resource_mut::<Exercise>().stage = 12;
        }
        _ => {}
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("provide screenshot path");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, uid) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Move this work".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                engine::actions::Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: "working".into(),
                    parents: vec![],
                },
                None,
            )
            .await
            .unwrap();
        (
            cell::CellRuntime {
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                information: None,
            },
            uid,
        )
    });
    let _guard = runtime.enter();
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        eprintln!("Area changes smoke timed out");
        std::process::exit(1);
    });
    connected_app(cell)
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            uid,
            path,
            stage: 0,
            area: None,
            wait: 0,
        })
        .add_systems(Startup, |mut windows: Query<&mut Window>| {
            windows.single_mut().unwrap().resolution.set(1100.0, 800.0)
        })
        .add_systems(
            Update,
            exercise
                .after(lince_interface::record_view::ReceiveRecords)
                .before(lince_interface::physics::SimulateWorkspaces),
        )
        .run();
}
