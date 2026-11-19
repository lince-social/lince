mod input;
pub(crate) mod login_view;
mod pending_input;
mod presentation;
mod run_view;
mod service;
#[cfg(test)]
mod tests;
mod vt;
mod worker;

pub(crate) use run_view::attach;
use presentation::render;

use crate::native::{actions::Action, sand_panel as panel};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bevy::{input_focus::InputFocus, prelude::*};
use cell::{ClientMessage, ServerMessage};
use std::collections::HashMap;

const CELL_WIDTH: f32 = 15.0 * 1233.0 / 2048.0;
const CELL_HEIGHT: f32 = 18.0;
pub(crate) const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "portable-pty",
        author: "Wez Furlong and contributors",
        license: include_str!("../../licenses/native/portable-pty.txt"),
    },
    crate::native::credits::Attribution {
        name: "libghostty",
        author: include_str!("terminal/vendor/UPSTREAM.txt"),
        license: include_str!("terminal/vendor/LICENSE.txt"),
    },
    crate::native::credits::Attribution {
        name: "Wasmi",
        author: "Wasmi contributors",
        license: include_str!("terminal/vendor/wasmi-LICENSE.txt"),
    },
    crate::native::credits::Attribution {
        name: "DejaVu Sans Mono",
        author: "Bitstream and DejaVu contributors",
        license: include_str!("terminal/vendor/DejaVu-LICENSE.txt"),
    },
];

#[derive(Resource, Clone)]
struct Mono(Handle<Font>, Handle<Font>);

pub(crate) fn preview_fonts(world: &mut World, preview: &mut World) {
    world.init_resource::<Mono>();
    preview.insert_resource(world.resource::<Mono>().clone());
}

pub(crate) fn code_font(world: &mut World) -> TextFont {
    world.init_resource::<Mono>();
    TextFont {
        font: world.resource::<Mono>().0.clone().into(),
        font_size: FontSize::Px(14.0),
        ..default()
    }
}
impl FromWorld for Mono {
    fn from_world(world: &mut World) -> Self {
        let mut fonts = world.resource_mut::<Assets<Font>>();
        Self(
            fonts.add(Font::from_bytes(
                include_bytes!("terminal/vendor/DejaVuSansMono.ttf").to_vec(),
            )),
            fonts.add(Font::from_bytes(
                include_bytes!("terminal/vendor/DejaVuSansMono-Bold.ttf").to_vec(),
            )),
        )
    }
}

#[derive(Component)]
pub struct TerminalSand {
    screen: Entity,
    status: Entity,
    session: Option<String>,
    opened: bool,
    exited: bool,
    worker: Option<worker::Worker>,
    geometry: (u16, u16),
    frame: Option<std::sync::Arc<vt::Frame>>,
    lines: Vec<Entity>,
    cursor: Entity,
    input: pending_input::Input,
}

#[derive(Component)]
struct Screen(Entity);

#[derive(Resource, Default)]
struct Sessions(HashMap<Entity, (String, crate::native::cell_bridge::Sender)>);

pub struct TerminalPlugin;
impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sessions>()
            .init_resource::<InputFocus>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                service::update.after(crate::native::cell_bridge::ReceiveCell),
            );
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    populate_view(world, sand, false)
}

fn populate_view(world: &mut World, sand: Entity, embedded: bool) -> Entity {
    world.init_resource::<Sessions>();
    let body = panel::frame(world, sand, "Terminal");
    let controls = panel::row(world, body);
    for (caption, command) in [
        ("Open shell", Command::Open),
        ("Close", Command::Close),
        ("Copy screen", Command::Copy),
        ("Paste", Command::Paste),
        ("Scroll up", Command::Scroll(-10)),
        ("Scroll down", Command::Scroll(10)),
    ] {
        if embedded && matches!(command, Command::Open | Command::Close) {
            continue;
        }
        panel::button(world, controls, sand, caption, command);
    }
    let screen = world.spawn((ChildOf(body), Screen(sand), bevy::input_focus::tab_navigation::TabIndex(0),
        Node { width: percent(100), flex_grow: 1.0, min_height: px(72), overflow: Overflow::clip(), ..default() },
        BackgroundColor(Color::BLACK), crate::native::icons::Tooltip("Click to type. Ctrl+Shift+C copies the screen; Ctrl+Shift+V pastes. Ctrl+Tab releases focus.".into()),
    )).observe(input::focus).observe(input::keyboard).id();
    let cursor = world
        .spawn((
            ChildOf(screen),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                width: px(CELL_WIDTH),
                height: px(CELL_HEIGHT),
                border: UiRect::all(px(1)),
                display: Display::None,
                ..default()
            },
            BorderColor::all(Color::WHITE),
        ))
        .id();
    let status = crate::native::edit_mode::label(
        world,
        body,
        "Open a local shell. Click the terminal to type.",
        12.0,
    );
    panel::credits(world, controls, body, CREDITS);
    world.entity_mut(sand).insert((
        TerminalSand {
            screen,
            status,
            session: None,
            opened: false,
            exited: false,
            worker: None,
            geometry: (80, 24),
            frame: None,
            lines: Vec::new(),
            cursor,
            input: pending_input::Input::default(),
        },
        crate::native::sand_store::SandCredits(CREDITS),
    ));
    sand
}

#[derive(Clone)]
enum Command {
    Open,
    Close,
    Copy,
    Paste,
    Scroll(i32),
}
impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Open)
            && crate::native::practice_cells::source(world, owner).is_some()
        {
            crate::native::notifications::report(
                world,
                "Terminal practice",
                "Inspect the controls here. Open a shell outside this practice when you choose to use your own shell.",
            );
            return;
        }
        if crate::native::laboratory::blocked(world, owner) {
            return;
        }
        let Some(terminal) = world.get::<TerminalSand>(owner) else {
            return;
        };
        let status = terminal.status;
        let result = match self {
            Self::Open => open(world, owner),
            Self::Close => {
                close(world, owner);
                Ok(())
            }
            Self::Scroll(delta) => command(world, owner, worker::Command::Scroll(Some(*delta))),
            Self::Copy => {
                let text = displayed_text(world, owner);
                world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .ok_or("Clipboard is unavailable".into())
                    .and_then(|mut clipboard| clipboard.set_text(text).map_err(|e| e.to_string()))
            }
            Self::Paste => {
                let text = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .and_then(|mut clipboard| clipboard.fetch_text().poll_result())
                    .ok_or("Clipboard is unavailable".to_string())
                    .and_then(|result| result.map_err(|e| e.to_string()));
                text.and_then(|text| {
                    if text.len() > 1024 * 1024 - 16 {
                        return Err("Paste is too large (1 MiB maximum)".into());
                    }
                    command(world, owner, worker::Command::Paste(text))
                })
            }
        };
        if let Err(error) = result {
            panel::status(world, status, error);
        }
    }
}

pub fn displayed_text(world: &World, owner: Entity) -> String {
    world
        .get::<TerminalSand>(owner)
        .and_then(|terminal| terminal.frame.as_ref())
        .map(|frame| {
            frame
                .lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

pub(crate) fn controls_visible(world: &World, owner: Entity) -> bool {
    world.get::<TerminalSand>(owner).is_some()
}

fn command(world: &World, owner: Entity, command: worker::Command) -> Result<(), String> {
    if matches!(
        command,
        worker::Command::Key { .. } | worker::Command::Paste(_)
    ) && world.get::<run_view::RunView>(owner).is_some()
        && world
            .get::<TerminalSand>(owner)
            .is_none_or(|terminal| !terminal.opened || terminal.exited)
    {
        return Err("This output is read-only".into());
    }
    world
        .get::<TerminalSand>(owner)
        .and_then(|terminal| terminal.worker.as_ref())
        .ok_or("Open a shell first")?
        .send(command)
}

fn open(world: &mut World, owner: Entity) -> Result<(), String> {
    let terminal = world
        .get::<TerminalSand>(owner)
        .ok_or("Terminal is closed")?;
    if terminal.session.is_some() && !terminal.exited {
        return Err("Close the current shell before opening another".into());
    }
    let (cols, rows) = terminal.geometry;
    let worker = worker::Worker::start(
        cols,
        rows,
        world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned(),
    )?;
    let id = nucleus::new_uid("terminal");
    panel::send(
        world,
        owner,
        ClientMessage::TerminalOpen {
            id: id.clone(),
            cols,
            rows,
            pixel_width: cols * 9,
            pixel_height: rows * 18,
        },
    )?;
    let sender = crate::native::practice_cells::sender(world, owner)
        .ok_or("The terminal connection is unavailable")?;
    world
        .resource_mut::<Sessions>()
        .0
        .insert(owner, (id.clone(), sender));
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.session = Some(id);
    terminal.worker = Some(worker);
    terminal.opened = false;
    terminal.exited = false;
    terminal.input.clear();
    let status = terminal.status;
    panel::status(world, status, "Opening shell…");
    Ok(())
}

fn close(world: &mut World, owner: Entity) {
    run_view::detach(world, owner);
    let Some(mut terminal) = world.get_mut::<TerminalSand>(owner) else {
        return;
    };
    terminal.worker = None;
    terminal.opened = false;
    terminal.input.clear();
    terminal.session = None;
    let status = terminal.status;
    panel::status(world, status, "Shell closed");
}

fn geometry(world: &World, screen: Entity) -> Option<(u16, u16)> {
    world
        .get::<ComputedNode>(screen)
        .filter(|node| node.size().x > 0.0)
        .map(|node| {
            let size = node.size() * node.inverse_scale_factor();
            (
                (size.x / CELL_WIDTH).floor().clamp(10.0, 240.0) as u16,
                (size.y / CELL_HEIGHT).floor().clamp(4.0, 100.0) as u16,
            )
        })
}

struct Run {
    start: usize,
    width: usize,
    text: String,
}

fn runs(cells: &[vt::Cell]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        if cell.width == 0 {
            continue;
        }
        let ascii = cell.text.len() == 1 && cell.text.is_ascii();
        if ascii && let Some(last) = runs.last_mut() {
            let previous = &cells[last.start];
            if last.start + last.width == index
                && last.text.is_ascii()
                && last.text.len() == last.width
                && previous.foreground == cell.foreground
                && previous.background == cell.background
                && previous.bold == cell.bold
                && previous.underline == cell.underline
            {
                last.text.push_str(&cell.text);
                last.width += usize::from(cell.width);
                continue;
            }
        }
        runs.push(Run {
            start: index,
            width: usize::from(cell.width),
            text: cell.text.clone(),
        });
    }
    runs
}
