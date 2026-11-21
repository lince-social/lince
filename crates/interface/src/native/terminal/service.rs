// non-lean-yet
// Scope: struct Service, fn update, fn receive.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 50, 78, 113, 184,
//   186, 187, 193.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   76.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (98, 113, 133, 138, 151, 159, 162, 175, 193, 203, 244, 258).

use super::*;
use bevy::ecs::query::QueryState;

pub(super) struct Service {
    wave: crate::native::cell_bridge::replies::Wave,
    terminals: QueryState<(Entity, &'static TerminalSand)>,
    recipients: Vec<(Entity, Entity)>,
    owners: Vec<Entity>,
    retired: Vec<Entity>,
}

impl FromWorld for Service {
    fn from_world(world: &mut World) -> Self {
        Self {
            wave: Default::default(),
            terminals: world.query(),
            recipients: Vec::new(),
            owners: Vec::new(),
            retired: Vec::new(),
        }
    }
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
    mut service: Local<Service>,
) {
    let service = &mut *service;
    service.wave.capture(
        &mut cursor,
        world.resource::<Messages<crate::native::cell_bridge::CellMessage>>(),
        |message| {
            matches!(
                message,
                ServerMessage::TerminalOpened { .. }
                    | ServerMessage::TerminalData { .. }
                    | ServerMessage::TerminalExit { .. }
                    | ServerMessage::FioteTerminal { .. }
                    | ServerMessage::Command { .. }
                    | ServerMessage::Error { .. }
            )
        },
    );
    for sequence in service.wave.drain() {
        let message = crate::native::cell_bridge::replies::message(
            world.resource::<Messages<crate::native::cell_bridge::CellMessage>>(),
            sequence,
        )
        .clone();
        receive(
            world,
            &message,
            &mut service.terminals,
            &mut service.recipients,
        );
    }
    service.retired.clear();
    service.retired.extend(
        world
            .resource::<Sessions>()
            .0
            .iter()
            .filter(|(entity, (id, _))| {
                world
                    .get::<TerminalSand>(**entity)
                    .is_none_or(|view| view.session.as_ref() != Some(id))
            })
            .map(|(entity, _)| *entity),
    );
    if service.retired.is_empty() {
        service.retired = Vec::new();
    }
    for entity in service.retired.drain(..) {
        let retry = {
            let (id, sender) = &world.resource::<Sessions>().0[&entity];
            matches!(
                sender.try_send(ClientMessage::TerminalClose { id: id.clone() }),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            )
        };
        if !retry {
            world.resource_mut::<Sessions>().0.remove(&entity);
        }
    }
    service.owners.clear();
    service
        .owners
        .extend(service.terminals.iter(world).map(|(owner, _)| owner));
    if service.owners.is_empty() {
        service.owners = Vec::new();
        service.recipients = Vec::new();
    }
    for owner in service.owners.drain(..) {
        let _service = nucleus::performance::Span::new("terminal-owner-service");
        run_view::poll(world, owner);
        login_view::poll(world, owner);
        let terminal = world.get::<TerminalSand>(owner).unwrap();
        let status = terminal.status;
        let geometry = if world.get::<run_view::RunView>(owner).is_none()
            && world.get::<login_view::LoginView>(owner).is_none()
        {
            geometry(world, terminal.screen).unwrap_or(terminal.geometry)
        } else {
            terminal.geometry
        };
        if geometry != terminal.geometry {
            let result = if terminal.opened {
                panel::send(
                    world,
                    owner,
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
            if output.input.len_bytes() != 0 {
                let full = {
                    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
                    if terminal.exited {
                        !terminal.input.fits(output.input.len_bytes())
                    } else {
                        terminal.input.append(output.input).is_err()
                    }
                };
                if full {
                    close(world, owner);
                    panel::status(world, status, "Terminal input queue is full; shell closed");
                    continue;
                }
            }
            if let Some(frame) = output.frame {
                render(world, owner, frame);
            }
        }
        loop {
            if world.get::<login_view::LoginView>(owner).is_some() {
                break;
            }
            let terminal = world.get::<TerminalSand>(owner).unwrap();
            if !terminal.opened {
                break;
            }
            let Some(bytes) = terminal.input.front() else {
                break;
            };
            let message = if let Some(run) = world.get::<run_view::RunView>(owner) {
                ClientMessage::Command {
                    id: run.run.clone(),
                    request: cell::command::Request::Input {
                        command: run.command.clone(),
                        run: run.run.clone(),
                        data_base64: BASE64.encode(bytes),
                    },
                }
            } else {
                ClientMessage::TerminalInput {
                    id: terminal.session.clone().unwrap(),
                    data_base64: BASE64.encode(bytes),
                }
            };
            if let Err(error) = panel::send(world, owner, message) {
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

fn receive(
    world: &mut World,
    message: &ServerMessage,
    terminals: &mut QueryState<(Entity, &'static TerminalSand)>,
    owners: &mut Vec<(Entity, Entity)>,
) {
    let _receive = nucleus::performance::Span::new("terminal-reply-application");
    if login_view::receive(world, message) || run_view::receive(world, message) {
        return;
    }
    let id = match &message {
        ServerMessage::TerminalOpened { id, .. }
        | ServerMessage::TerminalData { id, .. }
        | ServerMessage::TerminalExit { id, .. }
        | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    owners.clear();
    owners.extend(
        terminals
            .iter(world)
            .filter(|(_, terminal)| {
                terminal.session.as_ref() == Some(id)
                    || id == crate::native::cell_bridge::CONNECTION
            })
            .map(|(entity, terminal)| (entity, terminal.status)),
    );
    nucleus::performance::add("terminal-reply-recipients", owners.len() as u64);
    if owners.is_empty() {
        *owners = Vec::new();
    }
    for (owner, status) in owners.drain(..) {
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
