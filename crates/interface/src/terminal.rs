mod input;
#[cfg(test)]
mod tests;
mod vt;
mod worker;

use crate::{actions::Action, sand_panel as panel};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bevy::{input_focus::InputFocus, prelude::*};
use cell::{ClientMessage, ServerMessage};
use std::collections::{HashMap, VecDeque};

const CELL_WIDTH: f32 = 15.0 * 1233.0 / 2048.0;
const CELL_HEIGHT: f32 = 18.0;
const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "libghostty",
        author: include_str!("terminal/vendor/UPSTREAM.txt"),
        license: include_str!("terminal/vendor/LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "Wasmi",
        author: "Wasmi contributors",
        license: include_str!("terminal/vendor/wasmi-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "DejaVu Sans Mono",
        author: "Bitstream and DejaVu contributors",
        license: include_str!("terminal/vendor/DejaVu-LICENSE.txt"),
    },
];

#[derive(Resource)]
struct Mono(Handle<Font>, Handle<Font>);
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
    frame: Option<vt::Frame>,
    lines: Vec<Entity>,
    cursor: Entity,
    input: VecDeque<Vec<u8>>,
}

#[derive(Component)]
struct Screen(Entity);

#[derive(Resource, Default)]
struct Sessions(HashMap<Entity, String>);

pub struct TerminalPlugin;
impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sessions>()
            .init_resource::<InputFocus>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
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
        panel::button(world, controls, sand, caption, command);
    }
    let screen = world.spawn((ChildOf(body), Screen(sand), bevy::input_focus::tab_navigation::TabIndex(0),
        Node { width: percent(100), flex_grow: 1.0, min_height: px(72), overflow: Overflow::clip(), ..default() },
        BackgroundColor(Color::BLACK), crate::icons::Tooltip("Click to type. Ctrl+Shift+C copies the screen; Ctrl+Shift+V pastes. Ctrl+Tab releases focus.".into()),
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
    let status = crate::edit_mode::label(
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
            input: VecDeque::new(),
        },
        crate::sand_store::SandCredits(CREDITS),
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
        if crate::laboratory::active(world) {
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
                let text = terminal
                    .frame
                    .as_ref()
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
                    .unwrap_or_default();
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

fn command(world: &World, owner: Entity, command: worker::Command) -> Result<(), String> {
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
        world.get_resource::<crate::wake::WakeSignal>().cloned(),
    )?;
    let id = nucleus::new_uid("terminal");
    panel::send(
        world,
        ClientMessage::TerminalOpen {
            id: id.clone(),
            cols,
            rows,
            pixel_width: cols * 9,
            pixel_height: rows * 18,
        },
    )?;
    world.resource_mut::<Sessions>().0.insert(owner, id.clone());
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

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        receive(world, message);
    }
    let removed: Vec<_> = world
        .resource::<Sessions>()
        .0
        .iter()
        .filter(|(entity, id)| {
            world
                .get::<TerminalSand>(**entity)
                .is_none_or(|view| view.session.as_ref() != Some(*id))
        })
        .map(|(entity, id)| (*entity, id.clone()))
        .collect();
    for (entity, id) in removed {
        if panel::send(world, ClientMessage::TerminalClose { id }).is_ok() {
            world.resource_mut::<Sessions>().0.remove(&entity);
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<TerminalSand>>()
        .iter(world)
        .collect();
    for owner in owners {
        let terminal = world.get::<TerminalSand>(owner).unwrap();
        let status = terminal.status;
        let geometry = world
            .get::<ComputedNode>(terminal.screen)
            .filter(|node| node.size().x > 0.0)
            .map(|node| {
                let size = node.size() * node.inverse_scale_factor();
                (
                    (size.x / CELL_WIDTH).floor().clamp(10.0, 240.0) as u16,
                    (size.y / CELL_HEIGHT).floor().clamp(4.0, 100.0) as u16,
                )
            })
            .unwrap_or(terminal.geometry);
        if geometry != terminal.geometry {
            let result = if terminal.opened {
                panel::send(
                    world,
                    ClientMessage::TerminalResize {
                        id: terminal.session.clone().unwrap(),
                        cols: geometry.0,
                        rows: geometry.1,
                        pixel_width: geometry.0 * 9,
                        pixel_height: geometry.1 * 18,
                    },
                )
                .and_then(|()| {
                    command(
                        world,
                        owner,
                        worker::Command::Resize(geometry.0, geometry.1),
                    )
                })
            } else if terminal.session.is_some() {
                Err("Waiting for shell".into())
            } else {
                Ok(())
            };
            if result.is_ok() {
                world.get_mut::<TerminalSand>(owner).unwrap().geometry = geometry;
            }
        }
        let output = world
            .get::<TerminalSand>(owner)
            .unwrap()
            .worker
            .as_ref()
            .map(worker::Worker::take);
        if let Some(output) = output {
            if let Some(error) = output.error {
                close(world, owner);
                panel::status(world, status, error);
                continue;
            }
            let queued: usize = world
                .get::<TerminalSand>(owner)
                .unwrap()
                .input
                .iter()
                .map(Vec::len)
                .sum();
            if queued + output.input.iter().map(Vec::len).sum::<usize>() > 1024 * 1024 {
                close(world, owner);
                panel::status(world, status, "Terminal input queue is full; shell closed");
                continue;
            }
            if !world.get::<TerminalSand>(owner).unwrap().exited {
                world
                    .get_mut::<TerminalSand>(owner)
                    .unwrap()
                    .input
                    .extend(output.input);
            }
            if let Some(frame) = output.frame {
                render(world, owner, frame);
            }
        }
        loop {
            let terminal = world.get::<TerminalSand>(owner).unwrap();
            if !terminal.opened {
                break;
            }
            let Some(bytes) = terminal.input.front() else {
                break;
            };
            let message = ClientMessage::TerminalInput {
                id: terminal.session.clone().unwrap(),
                data_base64: BASE64.encode(bytes),
            };
            if let Err(error) = panel::send(world, message) {
                panel::status(world, status, error);
                break;
            }
            world
                .get_mut::<TerminalSand>(owner)
                .unwrap()
                .input
                .pop_front();
        }
    }
}

fn receive(world: &mut World, message: ServerMessage) {
    let id = match &message {
        ServerMessage::TerminalOpened { id, .. }
        | ServerMessage::TerminalData { id, .. }
        | ServerMessage::TerminalExit { id, .. }
        | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    let owners: Vec<_> = world
        .query::<(Entity, &TerminalSand)>()
        .iter(world)
        .filter(|(_, terminal)| {
            terminal.session.as_ref() == Some(id) || id == crate::cell_bridge::CONNECTION
        })
        .map(|(entity, terminal)| (entity, terminal.status))
        .collect();
    for (owner, status) in owners {
        match &message {
            ServerMessage::TerminalOpened { shell, cwd, .. } => {
                world.get_mut::<TerminalSand>(owner).unwrap().opened = true;
                panel::status(world, status, format!("{shell} · {cwd}"));
            }
            ServerMessage::TerminalData { data_base64, .. } => {
                let result = BASE64
                    .decode(data_base64)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| command(world, owner, worker::Command::Feed(bytes)));
                if let Err(error) = result {
                    close(world, owner);
                    panel::status(world, status, error);
                }
            }
            ServerMessage::TerminalExit { exit_code, .. } => {
                let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
                terminal.opened = false;
                terminal.exited = true;
                terminal.input.clear();
                panel::status(
                    world,
                    status,
                    format!(
                        "Shell exited{}",
                        exit_code.map_or(String::new(), |code| format!(" ({code})"))
                    ),
                );
            }
            ServerMessage::Error { message, .. } => {
                close(world, owner);
                panel::status(world, status, message);
            }
            _ => {}
        }
    }
}

fn render(world: &mut World, owner: Entity, frame: vt::Frame) {
    let terminal = world.get::<TerminalSand>(owner).unwrap();
    if terminal.frame.as_ref() == Some(&frame) {
        return;
    }
    let (screen, cursor) = (terminal.screen, terminal.cursor);
    let mut lines = terminal.lines.clone();
    for line in lines.drain(frame.lines.len().min(lines.len())..) {
        world.despawn(line);
    }
    while lines.len() < frame.lines.len() {
        let y = lines.len();
        lines.push(
            world
                .spawn((
                    ChildOf(screen),
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(y as f32 * CELL_HEIGHT),
                        width: percent(100),
                        height: px(CELL_HEIGHT),
                        ..default()
                    },
                ))
                .id(),
        );
    }
    world.init_resource::<Mono>();
    let fonts = (
        world.resource::<Mono>().0.clone(),
        world.resource::<Mono>().1.clone(),
    );
    for (y, cells) in frame.lines.iter().enumerate() {
        if world
            .get::<TerminalSand>(owner)
            .unwrap()
            .frame
            .as_ref()
            .and_then(|frame| frame.lines.get(y))
            == Some(cells)
        {
            continue;
        }
        panel::clear(world, lines[y]);
        for run in runs(cells) {
            let cell = &cells[run.start];
            if run.text.trim().is_empty() && cell.background == frame.background && !cell.underline
            {
                continue;
            }
            let node = world
                .spawn((
                    ChildOf(lines[y]),
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(run.start as f32 * CELL_WIDTH),
                        width: px(run.width as f32 * CELL_WIDTH),
                        height: px(CELL_HEIGHT),
                        border: UiRect {
                            bottom: px(if cell.underline { 1 } else { 0 }),
                            ..default()
                        },
                        ..default()
                    },
                    BackgroundColor(Color::srgb_u8(
                        cell.background[0],
                        cell.background[1],
                        cell.background[2],
                    )),
                    BorderColor::all(Color::srgb_u8(
                        cell.foreground[0],
                        cell.foreground[1],
                        cell.foreground[2],
                    )),
                ))
                .id();
            world.spawn((
                ChildOf(node),
                Pickable::IGNORE,
                Text::new(run.text),
                TextFont {
                    font: if cell.bold {
                        fonts.1.clone()
                    } else {
                        fonts.0.clone()
                    }
                    .into(),
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb_u8(
                    cell.foreground[0],
                    cell.foreground[1],
                    cell.foreground[2],
                )),
                TextLayout::no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));
        }
    }
    world
        .entity_mut(screen)
        .insert(BackgroundColor(Color::srgb_u8(
            frame.background[0],
            frame.background[1],
            frame.background[2],
        )));
    if let Some(mut node) = world.get_mut::<Node>(cursor) {
        node.display = if frame.cursor.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some((x, y)) = frame.cursor {
            node.left = px(f32::from(x) * CELL_WIDTH);
            node.top = px(f32::from(y) * CELL_HEIGHT);
        }
    }
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.lines = lines;
    terminal.frame = Some(frame);
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
