use super::*;
use std::time::{Duration, Instant};

#[derive(Component)]
pub(super) struct LoginView {
    parent: Entity,
    record: String,
    login: String,
    offset: u64,
    pending: Option<String>,
    next: Instant,
    sender: tokio::sync::mpsc::Sender<ClientMessage>,
}

impl Drop for LoginView {
    fn drop(&mut self) {
        let _ = self.sender.try_send(ClientMessage::FioteTerminal {
            id: nucleus::new_uid("login-close"),
            request: cell::FioteTerminalRequest {
                record: self.record.clone(),
                login: self.login.clone(),
                offset: self.offset,
                input: Default::default(),
                cols: 80,
                rows: 24,
                close: true,
            },
        });
    }
}

pub(crate) fn sync(world: &mut World, parent: Entity, record: &str, login: Option<&str>) {
    let views: Vec<_> = world
        .query::<(Entity, &LoginView)>()
        .iter(world)
        .filter(|(_, view)| view.parent == parent)
        .map(|(entity, view)| (entity, view.login.clone()))
        .collect();
    for (entity, id) in &views {
        if login != Some(id) {
            world.despawn(*entity);
        }
    }
    let Some(login) = login else { return };
    if views.iter().any(|(_, id)| id == login) {
        return;
    }
    let Some(bridge) = world.get_non_send::<crate::cell_bridge::CellBridge>() else {
        return;
    };
    let sender = bridge.outgoing.clone();
    let owner = panel::column(world, parent);
    world.get_mut::<Node>(owner).unwrap().height = px(470);
    populate_view(world, owner, true);
    let worker = worker::Worker::start(
        80,
        24,
        world.get_resource::<crate::wake::WakeSignal>().cloned(),
    );
    let status = world.get::<TerminalSand>(owner).unwrap().status;
    let Ok(worker) = worker else {
        panel::status(world, status, "Login terminal could not open");
        return;
    };
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.worker = Some(worker);
    terminal.session = Some(login.into());
    terminal.opened = true;
    world.entity_mut(owner).insert(LoginView {
        parent,
        record: record.into(),
        login: login.into(),
        offset: 0,
        pending: None,
        next: Instant::now(),
        sender,
    });
    panel::status(
        world,
        status,
        "Private login · click the terminal to type · output is not saved to messages",
    );
}

pub(super) fn poll(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<LoginView>(owner) else {
        return;
    };
    let terminal = world.get::<TerminalSand>(owner).unwrap();
    if view.pending.is_some()
        || terminal.exited
        || (view.next > Instant::now() && terminal.input.is_empty())
    {
        return;
    }
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
    let input: Vec<_> = terminal
        .input
        .iter()
        .flatten()
        .take(4096)
        .copied()
        .collect();
    let count = input.len();
    let id = nucleus::new_uid("login-frame");
    let request = cell::FioteTerminalRequest {
        record: view.record.clone(),
        login: view.login.clone(),
        offset: view.offset,
        input: cell::FioteSecret(BASE64.encode(input)),
        cols: geometry.0,
        rows: geometry.1,
        close: false,
    };
    if let Err(error) = panel::send(
        world,
        ClientMessage::FioteTerminal {
            id: id.clone(),
            request,
        },
    ) {
        panel::status(world, terminal.status, error);
        return;
    }
    let _ = command(
        world,
        owner,
        worker::Command::Resize(geometry.0, geometry.1),
    );
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.geometry = geometry;
    let mut remaining = count;
    while remaining > 0 {
        let front = terminal.input.front_mut().unwrap();
        let take = remaining.min(front.len());
        front.drain(..take);
        remaining -= take;
        if front.is_empty() {
            terminal.input.pop_front();
        }
    }
    world.get_mut::<LoginView>(owner).unwrap().pending = Some(id);
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::FioteTerminal { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &LoginView)>()
        .iter(world)
        .find(|(_, view)| view.pending.as_deref() == Some(id))
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    let status = world.get::<TerminalSand>(owner).unwrap().status;
    let mut view = world.get_mut::<LoginView>(owner).unwrap();
    view.pending = None;
    view.next = Instant::now() + Duration::from_millis(150);
    match message {
        ServerMessage::FioteTerminal { frame, .. } => {
            world.get_mut::<LoginView>(owner).unwrap().offset = frame.next;
            if let Ok(bytes) = BASE64.decode(&frame.data_base64) {
                let _ = command(world, owner, worker::Command::Feed(bytes));
            }
            if frame.trimmed {
                panel::status(
                    world,
                    status,
                    "Older login output was discarded from the private buffer",
                );
            }
            if frame.exit.is_some() || frame.error.is_some() {
                let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
                terminal.exited = true;
                terminal.opened = false;
                panel::status(
                    world,
                    status,
                    frame.error.clone().unwrap_or_else(|| {
                        format!(
                            "Login command ended ({}). Check connection to confirm access.",
                            frame.exit.unwrap()
                        )
                    }),
                );
            }
        }
        ServerMessage::Error { message, .. } => {
            world.get_mut::<TerminalSand>(owner).unwrap().exited = true;
            panel::status(world, status, message);
        }
        _ => {}
    }
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.after(Duration::from_millis(150));
    }
    true
}
