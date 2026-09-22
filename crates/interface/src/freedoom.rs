mod process;
#[cfg(test)]
mod tests;

use crate::{actions::Action, sand_panel as panel};
use bevy::{input_focus::InputFocus, prelude::*};

const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "Freedoom Phase 1",
        author: include_str!("freedoom/vendor/CREDITS.txt"),
        license: include_str!("freedoom/vendor/COPYING.txt"),
    },
    crate::credits::Attribution {
        name: "Doomgeneric",
        author: include_str!("freedoom/vendor/engine/UPSTREAM.txt"),
        license: include_str!("freedoom/vendor/engine/LICENSE.txt"),
    },
];

#[derive(Component)]
pub struct FreedoomSand {
    screen: Entity,
    status: Entity,
    image: Handle<Image>,
    process: Option<process::Game>,
    pressed: Vec<u8>,
    paused: bool,
}

pub struct FreedoomPlugin;
impl Plugin for FreedoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputFocus>()
            .init_resource::<Assets<Image>>()
            .add_systems(Update, update);
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Assets<Image>>();
    let body = panel::frame(world, sand, "Freedoom");
    let controls = panel::row(world, body);
    panel::button(world, controls, sand, "Start / restart", Command::Start);
    panel::button(world, controls, sand, "Pause / resume", Command::Pause);
    panel::button(world, controls, sand, "Stop", Command::Stop);
    let image = Handle::default();
    let screen = world
        .spawn((
            ChildOf(body),
            BackgroundColor(Color::BLACK),
            bevy::input_focus::tab_navigation::TabIndex(0),
            Node {
                width: percent(100),
                aspect_ratio: Some(1.6),
                min_height: px(0),
                flex_shrink: 1.0,
                ..default()
            },
            crate::icons::Tooltip(
                "Click the game to play. Escape opens its menu. Tab leaves the game.".into(),
            ),
            GameScreen(sand),
        ))
        .observe(focus)
        .observe(keyboard)
        .id();
    crate::edit_mode::label(
        world,
        body,
        "Arrows / WASD: move · Ctrl: fire · Space: use · Shift: run\nEnter: choose · Escape: menu · Tab: release focus · No sound",
        12.0,
    );
    let status = crate::edit_mode::label(world, body, "Start a local game", 12.0);
    panel::credits(world, controls, body, CREDITS);
    world.entity_mut(sand).insert((
        FreedoomSand {
            screen,
            status,
            image,
            process: None,
            pressed: Vec::new(),
            paused: false,
        },
        crate::sand_store::SandCredits(CREDITS),
    ));
    sand
}

fn image(world: &mut World) -> Handle<Image> {
    let mut image = Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[0, 0, 0, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    world.resource_mut::<Assets<Image>>().add(image)
}

#[derive(Component)]
struct GameScreen(Entity);

fn focus(
    mut event: On<Pointer<Press>>,
    screens: Query<&GameScreen>,
    mut focus: ResMut<InputFocus>,
) {
    if screens.contains(event.entity) && event.button == PointerButton::Primary {
        focus.set(event.entity, bevy::input_focus::FocusCause::Pressed);
        event.propagate(false);
    }
}

fn key(code: KeyCode) -> Option<u8> {
    Some(match code {
        KeyCode::ArrowUp | KeyCode::KeyW => 0xad,
        KeyCode::ArrowDown | KeyCode::KeyS => 0xaf,
        KeyCode::ArrowLeft => 0xac,
        KeyCode::ArrowRight => 0xae,
        KeyCode::KeyA => b',',
        KeyCode::KeyD => b'.',
        KeyCode::ControlLeft | KeyCode::ControlRight => 0x9d,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => 0xb6,
        KeyCode::AltLeft | KeyCode::AltRight => 0xb8,
        KeyCode::Space => b' ',
        KeyCode::Enter => 13,
        KeyCode::Escape => 27,
        KeyCode::Digit1 => b'1',
        KeyCode::Digit2 => b'2',
        KeyCode::Digit3 => b'3',
        KeyCode::Digit4 => b'4',
        KeyCode::Digit5 => b'5',
        KeyCode::Digit6 => b'6',
        KeyCode::Digit7 => b'7',
        KeyCode::KeyY => b'y',
        KeyCode::KeyN => b'n',
        KeyCode::Backspace => 127,
        _ => return None,
    })
}

fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    screens: Query<&GameScreen>,
    mut games: Query<&mut FreedoomSand>,
) {
    let Ok(screen) = screens.get(event.focused_entity) else {
        return;
    };
    if event.input.key_code == KeyCode::Tab {
        return;
    }
    event.propagate(false);
    if event.input.repeat {
        return;
    }
    let Some(key) = key(event.input.key_code) else {
        return;
    };
    let Ok(mut game) = games.get_mut(screen.0) else {
        return;
    };
    if game.paused {
        return;
    }
    let pressed = event.input.state.is_pressed();
    if let Some(process) = &game.process {
        process.send([u8::from(pressed), key]);
    }
    game.pressed.retain(|held| *held != key);
    if pressed {
        game.pressed.push(key);
    }
}

#[derive(Clone)]
enum Command {
    Start,
    Pause,
    Stop,
}
impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::active(world) {
            return;
        }
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let image = matches!(self, Self::Start).then(|| image(world));
        let Some(mut game) = world.get_mut::<FreedoomSand>(owner) else {
            return;
        };
        let status = game.status;
        match self {
            Self::Start => {
                game.process = None;
                match process::Game::start(wake) {
                    Ok(process) => {
                        game.process = Some(process);
                        game.paused = false;
                        game.pressed.clear();
                        game.image = image.unwrap();
                        let screen = game.screen;
                        let image = game.image.clone();
                        world.entity_mut(screen).insert(ImageNode::new(image));
                        panel::status(world, status, "Click the game to play");
                    }
                    Err(error) => panel::status(world, status, error),
                }
            }
            Self::Pause => {
                release(&mut game);
                game.paused = !game.paused;
                let message = if game.paused {
                    "Paused"
                } else {
                    "Click the game to play"
                };
                panel::status(world, status, message);
            }
            Self::Stop => {
                game.process = None;
                game.pressed.clear();
                panel::status(world, status, "Stopped");
            }
        }
    }
}

fn release(game: &mut FreedoomSand) {
    for key in game.pressed.drain(..) {
        if let Some(process) = &game.process {
            process.send([0, key]);
        }
    }
}

fn update(
    mut games: Query<(
        &mut FreedoomSand,
        &Node,
        Option<&InheritedVisibility>,
        Option<&crate::workspace::WorkspaceMember>,
        Option<&crate::sand::InBox>,
    )>,
    spaces: Query<&crate::workspace::Workspaces>,
    windows: Query<&Window>,
    focus: Res<InputFocus>,
    mut images: ResMut<Assets<Image>>,
    mut texts: Query<&mut Text>,
) {
    for (mut game, node, visible, member, root) in &mut games {
        let inactive = member.zip(root).is_some_and(|(member, root)| {
            spaces
                .get(root.0)
                .is_ok_and(|spaces| spaces.active != member.0)
        });
        let hidden = inactive
            || node.display == Display::None
            || visible.is_some_and(|visible| !visible.get())
            || windows.iter().any(|window| !window.focused);
        if hidden || focus.get() != Some(game.screen) {
            release(&mut game);
        }
        if let Some(process) = &game.process {
            process.suspend(hidden || game.paused);
            if let Some(pixels) = process.frame()
                && let Some(mut image) = images.get_mut(&game.image)
            {
                image.data = Some(pixels);
            }
            if let Some(message) = process.finished() {
                if let Ok(mut text) = texts.get_mut(game.status) {
                    text.0 = message.into();
                }
                game.process = None;
            }
        }
    }
}
