use bevy::{
    a11y::AccessibilityNode,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    ui_widgets::Activate,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_desktop::{
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
    tab: usize,
    tabs_only: bool,
    root: Option<Entity>,
    sand: Option<Entity>,
    broken: Option<Entity>,
    captured: bool,
    output: PathBuf,
}

fn button(world: &mut World, label: &str) -> Entity {
    world
        .query_filtered::<(Entity, &AccessibilityNode), With<bevy::ui_widgets::Button>>()
        .iter(world)
        .find(|(_, node)| node.label() == Some(label))
        .unwrap_or_else(|| panic!("Missing button: {label}"))
        .0
}

fn activate(world: &mut World, label: &str) {
    let entity = button(world, label);
    world.trigger(Activate { entity });
}

fn pointer(world: &mut World, label: &str) {
    let entity = button(world, label);
    let node = world.get::<ComputedNode>(entity).unwrap();
    assert!(node.size().min_element() > 0.0, "Hidden button: {label}");
    let position =
        world.get::<UiGlobalTransform>(entity).unwrap().translation * node.inverse_scale_factor();
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::CursorMoved(CursorMoved {
        window,
        position,
        delta: None,
    }));
}

fn press(world: &mut World, down: bool) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    }));
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
        7 => {
            activate(world, "Edit mode");
            state.frame = 0;
            state.stage = 8;
        }
        8 => {
            let tabs = [
                ("General", "General"),
                ("Sand store", "Sand store"),
                ("Customization", "Customization"),
                ("Workspaces", "Workspace"),
                ("Information", "Information"),
                ("Shortcuts", "Shortcuts"),
                ("Areas of influence", "Areas"),
                ("Licenses and credits", "Licenses and credits"),
                ("Sand store", "Sand store"),
            ];
            match state.frame % 12 {
                0 => pointer(world, "General"),
                3 => pointer(world, tabs[state.tab].0),
                5 => press(world, true),
                7 => press(world, false),
                11 => {
                    let heading = tabs[state.tab].1;
                    assert!(
                        world
                            .query::<&Text>()
                            .iter(world)
                            .any(|text| text.0 == heading),
                        "Missing rendered tab heading after clicking: {heading}"
                    );
                    state.tab += 1;
                    if state.tab == tabs.len() {
                        activate(world, "Close edit mode");
                        if state.tabs_only {
                            println!(
                                "Edit tabs smoke passed: mouse clicks on all eight tabs and Sand store reopening."
                            );
                            world.write_message(AppExit::Success);
                            state.stage = 5;
                        } else {
                            state.stage = 0;
                        }
                    }
                }
                _ => {}
            }
        }
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
                        lince_desktop::workspace::WorkspaceMember(1),
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
            let saved = lince_desktop::sand_text::snapshot(world, state.sand.unwrap());
            assert_eq!(saved[0].text, "Keep this draft");
            println!(
                "Laboratory smoke passed: all edit tabs, Sand store reopening, graphics device, resource counts, asset failure, shared behavior suite, six rendered stress workloads, suspension and restoration."
            );
            world.write_message(AppExit::Success);
            state.stage = 5;
        }
        _ => {}
    }
    world.insert_resource(state);
}

#[tokio::main]
async fn main() {
    let output = PathBuf::from(std::env::args().nth(1).expect("provide screenshot path"));
    lince_desktop::app::interface_app()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            started: Instant::now(),
            frame: 0,
            stage: 7,
            tab: 0,
            tabs_only: std::env::args().any(|arg| arg == "--edit-tabs-only"),
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
