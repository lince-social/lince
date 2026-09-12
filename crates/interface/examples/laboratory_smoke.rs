use bevy::{
    a11y::AccessibilityNode,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    canvas::CanvasItem,
    container::BoxRoot,
    information::OpenInformation,
    laboratory::{Laboratory, LaboratoryAction, LaboratoryRoot, StressConfig},
    sand_store::{SandKind, spawn_sand},
};
use std::{path::PathBuf, time::Instant};

#[derive(Resource)]
struct Exercise {
    started: Instant,
    frame: u64,
    stage: u8,
    root: Option<Entity>,
    sand: Option<Entity>,
    broken: Option<Entity>,
    captured: bool,
    output: PathBuf,
}

fn activate(world: &mut World, label: &str) {
    let entity = world
        .query::<(Entity, &AccessibilityNode)>()
        .iter(world)
        .find(|(_, node)| node.label() == Some(label))
        .unwrap_or_else(|| panic!("Missing button: {label}"))
        .0;
    world.trigger(Activate { entity });
}

fn exercise(world: &mut World) {
    let mut state = world.remove_resource::<Exercise>().unwrap();
    assert!(
        state.started.elapsed().as_secs() < 180,
        "Laboratory smoke timed out"
    );
    state.frame += 1;
    if state.frame < 12 {
        world.insert_resource(state);
        return;
    }
    match state.stage {
        0 => {
            let root = world
                .query_filtered::<Entity, (With<BoxRoot>, Without<LaboratoryRoot>)>()
                .single(world)
                .unwrap();
            state.root = Some(root);
            state.sand = Some(spawn_sand(
                world,
                root,
                1,
                SandKind::EditableText,
                "Keep this draft",
                bevy::math::DVec2::new(-120.0, 40.0),
            ));
            world
                .entity_mut(state.sand.unwrap())
                .insert(Name::new("Draft"));
            let image = world
                .resource::<AssetServer>()
                .load("laboratory-missing-image.png");
            state.broken = Some(
                world
                    .spawn((
                        CanvasItem {
                            position: bevy::math::DVec2::new(150.0, 40.0),
                            size: Vec2::splat(100.0),
                        },
                        ImageNode::new(image),
                        Name::new("Missing image"),
                        lince_interface::workspace::WorkspaceMember(1),
                        ChildOf(root),
                    ))
                    .id(),
            );
            OpenInformation.apply(world, root);
            state.stage = 1;
        }
        1 => {
            let image = world.get::<ImageNode>(state.broken.unwrap()).unwrap();
            if matches!(
                world
                    .resource::<AssetServer>()
                    .get_load_state(image.image.id()),
                Some(bevy::asset::LoadState::Failed(_))
            ) {
                activate(world, "Laboratory");
                state.stage = 2;
            }
        }
        2 if world.resource::<Laboratory>().root.is_some()
            && world
                .query::<&AccessibilityNode>()
                .iter(world)
                .any(|node| node.label() == Some("Run all")) =>
        {
            assert_eq!(
                world.get::<Node>(state.root.unwrap()).unwrap().display,
                Display::None
            );
            let resources = &world.resource::<Laboratory>().resources;
            assert!(resources.graphics.name.is_some());
            let draft = resources
                .sands
                .iter()
                .find(|row| row.entity == state.sand.unwrap().to_string())
                .unwrap();
            assert!(draft.text_bytes >= "Keep this draft".len());
            let broken = resources
                .sands
                .iter()
                .find(|row| row.entity == state.broken.unwrap().to_string())
                .unwrap();
            assert!(
                broken
                    .startup
                    .iter()
                    .any(|issue| issue.failed
                        && issue.reason.contains("laboratory-missing-image.png"))
            );
            world.resource_mut::<Laboratory>().config = StressConfig {
                max_sands: 64,
                batch: 32,
                warmup_frames: 2,
                sample_frames: 5,
                budget_ms: 1000.0,
            };
            activate(world, "Run all");
            state.stage = 3;
        }
        3 if !world.resource::<Laboratory>().reports.is_empty() => {
            let lab = world.resource::<Laboratory>();
            let behavior = lab.behavior.as_ref().unwrap();
            assert!(behavior.finished());
            assert_eq!(behavior.results.len(), behavior.total);
            let failures: Vec<_> = behavior
                .results
                .iter()
                .filter(|row| row.error.is_some())
                .collect();
            assert!(failures.is_empty(), "Behavior failures: {failures:?}");
            assert!(lab.reports[0].complete);
            assert_eq!(lab.reports[0].graphics.name, lab.resources.graphics.name);
            assert_eq!(lab.reports[0].stops.len(), 6);
            assert_eq!(
                world
                    .get::<CanvasItem>(state.sand.unwrap())
                    .unwrap()
                    .position,
                bevy::math::DVec2::new(-120.0, 40.0)
            );
            std::fs::write(
                state.output.with_extension("json"),
                serde_json::to_vec_pretty(&lab.reports).unwrap(),
            )
            .unwrap();
            activate(world, "Sand resources");
            state.stage = 6;
        }
        6 if world.query::<&Text>().iter(world).any(|text| {
            text.0.contains("Sands before Laboratory suspension")
                && text.0.contains("Cannot load image")
        }) =>
        {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(state.output.clone()))
                .observe(
                    |capture: On<ScreenshotCaptured>, mut state: ResMut<Exercise>| {
                        assert!(
                            capture
                                .image
                                .data
                                .as_ref()
                                .unwrap()
                                .chunks_exact(4)
                                .any(|pixel| pixel[0] > 80 && pixel[1] > 80 && pixel[2] > 80)
                        );
                        state.captured = true;
                    },
                );
            state.stage = 4;
        }
        4 if state.captured => {
            LaboratoryAction::Close.apply(world, state.root.unwrap());
            assert!(world.resource::<Laboratory>().root.is_none());
            assert_eq!(
                world.get::<Node>(state.root.unwrap()).unwrap().display,
                Display::Flex
            );
            let saved = lince_interface::sand_text::snapshot(world, state.sand.unwrap());
            assert_eq!(saved[0].text, "Keep this draft");
            println!(
                "Laboratory smoke passed: graphics device, resource counts, asset failure, shared behavior suite, six rendered stress workloads, suspension and restoration."
            );
            world.write_message(AppExit::Success);
            state.stage = 5;
        }
        _ => {}
    }
    world.insert_resource(state);
}

fn main() {
    let output = PathBuf::from(std::env::args().nth(1).expect("provide screenshot path"));
    lince_interface::app::interface_app()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            started: Instant::now(),
            frame: 0,
            stage: 0,
            root: None,
            sand: None,
            broken: None,
            captured: false,
            output,
        })
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1100.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
