use bevy::{
    a11y::AccessibilityNode,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    window::{Ime, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_sandbox::{
    actions::ActionButton,
    app::{CellHandle, interface_app},
    canvas::CanvasItem,
    cell_bridge::CellBridgePlugin,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const TITLE: &str = "العربية · 日本語 · うなぎうなじ · café";
const SEARCH: &str = "Search local Records and selected knowledge sources";

#[derive(Resource)]
struct Exercise {
    directory: PathBuf,
    mobile: bool,
    stage: u8,
    started: Instant,
    changed: Instant,
    subject: Option<Entity>,
}

fn button(world: &mut World, title: &str) -> Option<Entity> {
    world
        .query::<(Entity, &AccessibilityNode, &ActionButton)>()
        .iter(world)
        .find(|(_, node, _)| node.label() == Some(title))
        .map(|(entity, _, _)| entity)
}

fn press(world: &mut World, title: &str) -> bool {
    let Some(entity) = button(world, title) else {
        return false;
    };
    let action = world.get::<ActionButton>(entity).unwrap().clone();
    action.actions.run(world, action.target);
    true
}

fn field(world: &mut World) -> Entity {
    world
        .query::<(Entity, &AccessibilityNode, &EditableText)>()
        .iter(world)
        .find(|(_, node, _)| node.label() == Some(SEARCH))
        .unwrap()
        .0
}

fn key(world: &mut World, code: KeyCode, logical: Key, state: ButtonState) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(world)
        .unwrap();
    world.get_mut::<Window>(window).unwrap().focused = true;
    let input = KeyboardInput {
        key_code: code,
        logical_key: logical,
        state,
        text: None,
        repeat: false,
        window,
    };
    world.write_message(input.clone());
    world.write_message(WindowEvent::KeyboardInput(input));
}

fn transition(world: &mut World, stage: u8) {
    let mut exercise = world.resource_mut::<Exercise>();
    exercise.stage = stage;
    exercise.changed = Instant::now();
}

fn rect(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    Rect::from_center_size(
        world.get::<UiGlobalTransform>(entity).unwrap().translation,
        node.size(),
    )
}

fn logical_rect(world: &World, entity: Entity) -> Rect {
    let bounds = rect(world, entity);
    let scale = world
        .get::<ComputedNode>(entity)
        .unwrap()
        .inverse_scale_factor();
    Rect::from_corners(bounds.min * scale, bounds.max * scale)
}

fn setup(world: &mut World) {
    let mobile = world.resource::<Exercise>().mobile;
    let size = if mobile {
        Vec2::new(390.0, 844.0)
    } else {
        Vec2::new(1100.0, 850.0)
    };
    let mut window = world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(world)
        .unwrap();
    window.resolution.set(size.x, size.y);
    window.title = "Vocabulary verification · isolated memory database".into();
    let root = world.spawn(BoxRoot).id();
    let subject = spawn_sand(world, root, 1, SandKind::Ontology, "", DVec2::ZERO);
    world.get_mut::<CanvasItem>(subject).unwrap().size = size - Vec2::splat(24.0);
    world.resource_mut::<Exercise>().subject = Some(subject);
}

fn exercise(world: &mut World) {
    let state = world.resource::<Exercise>();
    assert!(
        state.started.elapsed() < Duration::from_secs(120),
        "Vocabulary native verification timed out at stage {}",
        state.stage
    );
    if state.changed.elapsed() < Duration::from_millis(350) {
        return;
    }
    match state.stage {
        0 => {
            if world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == "Live")
            {
                let history = button(world, "Record history").unwrap();
                world
                    .resource_mut::<InputFocus>()
                    .set(history, FocusCause::Navigated);
                key(world, KeyCode::Tab, Key::Tab, ButtonState::Pressed);
                transition(world, 1);
            }
        }
        1 => {
            key(world, KeyCode::Tab, Key::Tab, ButtonState::Released);
            if world.resource::<InputFocus>().get() != button(world, "Knowledge sources") {
                return;
            }
            key(world, KeyCode::Enter, Key::Enter, ButtonState::Pressed);
            transition(world, 2);
        }
        2 => {
            key(world, KeyCode::Enter, Key::Enter, ButtonState::Released);
            if press(world, "List sources") {
                transition(world, 3);
            }
        }
        3 => {
            let source = world
                .query::<&AccessibilityNode>()
                .iter(world)
                .find_map(|node| {
                    node.label()
                        .filter(|label| label.starts_with("Native vocabulary fixture"))
                        .map(str::to_owned)
                });
            if let Some(source) = source {
                assert!(press(world, &source));
                let input = field(world);
                world
                    .resource_mut::<InputFocus>()
                    .set(input, FocusCause::Navigated);
                let window = world
                    .query_filtered::<Entity, With<PrimaryWindow>>()
                    .single(world)
                    .unwrap();
                world.write_message(Ime::Enabled { window });
                world.write_message(Ime::Preedit {
                    window,
                    value: TITLE.into(),
                    cursor: Some((TITLE.len(), TITLE.len())),
                });
                transition(world, 20);
            }
        }
        20 => {
            let input = field(world);
            if !world.get::<EditableText>(input).unwrap().is_composing() {
                return;
            }
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(Ime::Commit {
                window,
                value: TITLE.into(),
            });
            transition(world, 21);
        }
        21 => {
            let input = field(world);
            let text = world.get::<EditableText>(input).unwrap();
            if text.value() != TITLE {
                return;
            }
            let Some(layout) = text.editor.try_layout() else {
                return;
            };
            let mut glyphs = 0;
            let boundary = TITLE.find("うなじ").unwrap();
            let mut boundary_seen = false;
            for line in layout.lines() {
                for run in line.runs() {
                    for cluster in run.clusters() {
                        let start = cluster.text_range().start;
                        if start == boundary {
                            assert!(cluster.is_word_boundary());
                            boundary_seen = true;
                        }
                        for glyph in cluster.glyphs() {
                            assert_ne!(glyph.id, 0, "Missing multilingual input glyph");
                            glyphs += 1;
                        }
                    }
                }
            }
            assert!(glyphs > 0);
            assert!(boundary_seen, "Japanese words must have usable boundaries");
            assert!(press(world, "Search selected sources"));
            transition(world, 4);
        }
        4 => {
            if let Some(result) = button(world, TITLE) {
                world
                    .resource_mut::<InputFocus>()
                    .set(result, FocusCause::Navigated);
                key(world, KeyCode::Space, Key::Space, ButtonState::Pressed);
                transition(world, 5);
            }
        }
        5 => {
            key(world, KeyCode::Space, Key::Space, ButtonState::Released);
            transition(world, 6);
        }
        6 => {
            let heading = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(entity, text)| {
                    text.0 == TITLE
                        && world.get::<ChildOf>(*entity).is_some_and(|parent| {
                            world.get::<ActionButton>(parent.parent()).is_none()
                        })
                })
                .map(|(entity, _)| entity);
            if let Some(heading) = heading.filter(|entity| {
                world
                    .get::<ComputedNode>(*entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
            }) {
                let target = rect(world, heading);
                let mut parent = world.get::<ChildOf>(heading).map(ChildOf::parent);
                while let Some(entity) = parent {
                    if world
                        .get::<lince_sandbox::scroll_sand::ScrollSand>(entity)
                        .is_some()
                    {
                        let viewport = rect(world, entity);
                        let scale = world
                            .get::<ComputedNode>(entity)
                            .unwrap()
                            .inverse_scale_factor();
                        world.get_mut::<ScrollPosition>(entity).unwrap().0.y +=
                            ((target.min.y - viewport.min.y) * scale - 12.0).max(0.0);
                        break;
                    }
                    parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                transition(world, 7);
            }
        }
        7 => {
            let subject = world.resource::<Exercise>().subject.unwrap();
            let bounds = logical_rect(world, subject);
            let size = bounds.size();
            assert!(size.x > 300.0 && size.y > 700.0);
            let mut widths = Vec::new();
            for (entity, text) in world.query::<(Entity, &Text)>().iter(world) {
                if text.0 == TITLE
                    || text.0.starts_with("Source:")
                    || text.0.starts_with("العربية لغة")
                {
                    let bounds = logical_rect(world, entity);
                    assert!(
                        bounds.width() <= size.x + 1.0,
                        "Reader text exceeds its host: {:?}",
                        text.0
                    );
                    widths.push(serde_json::json!({"text":text.0.chars().take(80).collect::<String>(),"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y]}));
                }
            }
            assert!(!widths.is_empty());
            let window = world
                .query_filtered::<&Window, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            let window = serde_json::json!({"logical":[window.width(),window.height()],"physical":[window.physical_width(),window.physical_height()],"scale_factor":window.scale_factor()});
            let state = world.resource::<Exercise>();
            let report = serde_json::json!({"host":"native Bevy window","narrow_desktop_window":state.mobile,"window":window,"scale":world.resource::<UiScale>().0,"bounds_space":"native UI layout in logical pixels; canvas placement applied separately","sand_bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],"text":widths,"keyboard":"Tab navigation, Enter and Space activation","input":"Arabic, Japanese and accented Latin IME composition and commit; no missing input glyphs"});
            std::fs::write(
                state.directory.join("geometry.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            let path = state.directory.join("reader.png");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        println!("Vocabulary native verification passed");
                        exit.write(AppExit::Success);
                    },
                );
            transition(world, 8);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    let mut arguments = std::env::args().skip(1);
    let directory = PathBuf::from(arguments.next().expect("Provide a capture directory"));
    let mobile = arguments.any(|argument| argument == "--mobile");
    std::fs::create_dir_all(&directory).unwrap();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut archive = engine::knowledge::provider::test_source().archive;
    archive.source.name = "Native vocabulary fixture".into();
    archive.items[0].title = TITLE.into();
    archive.items[0].content="العربية لغة · 日本語の語彙 · café\nA source reading retains its revision, attribution, references and explicit meanings.\n".repeat(20);
    engine
        .act(
            engine::actions::Action::Knowledge {
                request: nucleus::knowledge::Command::InstallArchive {
                    package: serde_json::to_string(&archive).unwrap(),
                },
            },
            None,
        )
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    interface_app()
        .insert_resource(CellHandle(runtime))
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            directory,
            mobile,
            stage: 0,
            started: Instant::now(),
            changed: Instant::now(),
            subject: None,
        })
        .add_plugins(CellBridgePlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, exercise)
        .run();
}
