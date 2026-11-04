use super::*;
use cell::command::{Request, Response, Run};
use std::time::{Duration, Instant};

#[derive(Component)]
pub(super) struct RunView {
    pub command: String,
    pub run: String,
    offset: u64,
    pending: bool,
    complete: bool,
    follow: bool,
    requested: bool,
    next: Instant,
    request: String,
    changes: Option<crate::native::cell_bridge::CommandChanges>,
    source: Option<cell::terminal::commands::Source>,
}

pub(crate) fn attach(world: &mut World, parent: Entity, run: &Run) -> Result<Entity, String> {
    let owner = panel::column(world, parent);
    world.get_mut::<Node>(owner).unwrap().height = px(470);
    populate_view(world, owner, true);
    let controls = panel::row(world, owner);
    for (title, action) in [
        ("From start", Playback::Start),
        ("Next output", Playback::Next),
        ("Follow latest", Playback::Follow),
    ] {
        panel::button(world, controls, owner, title, action);
    }
    let worker = worker::Worker::start(
        80,
        24,
        world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned(),
    )?;
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.worker = Some(worker);
    terminal.session = Some(run.id.clone());
    terminal.exited = run.finished_ms.is_some();
    let status = terminal.status;
    panel::status(world, status, "Loading saved terminal output…");
    let changes = run.finished_ms.is_none().then(|| crate::native::cell_bridge::CommandChanges::new(world, owner, &run.command, Some(&run.id))).flatten();
    let source = crate::native::practice_cells::command_source(world, owner);
    world.entity_mut(owner).insert(RunView {
        command: run.command.clone(),
        run: run.id.clone(),
        offset: 0,
        pending: false,
        complete: false,
        follow: true,
        requested: true,
        next: Instant::now(),
        request: nucleus::new_uid("output"),
        changes,
        source,
    });
    Ok(owner)
}

#[derive(Clone)]
enum Playback {
    Start,
    Next,
    Follow,
}

impl Action for Playback {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::blocked(world, owner) || world.get::<RunView>(owner).is_none_or(|run| run.pending) {
            return;
        }
        if !current(world, owner) { return; }
        if !matches!(self, Self::Follow) {
            let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
            terminal.opened = false;
            terminal.input.clear();
        }
        if matches!(self, Self::Start) {
            let worker = worker::Worker::start(
                80,
                24,
                world
                    .get_resource::<crate::native::wake::WakeSignal>()
                    .cloned(),
            );
            let Ok(worker) = worker else { return };
            let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
            terminal.worker = Some(worker);
            terminal.input.clear();
            terminal.opened = false;
            terminal.frame = None;
        }
        let changes = if matches!(self, Self::Follow) {
            let view = world.get::<RunView>(owner).unwrap();
            crate::native::cell_bridge::CommandChanges::new(world, owner, &view.command, Some(&view.run))
        } else { None };
        let mut view = world.get_mut::<RunView>(owner).unwrap();
        view.changes = changes;
        view.follow = matches!(self, Self::Follow);
        view.requested = true;
        view.next = Instant::now();
        if matches!(self, Self::Start) {
            view.offset = 0;
            view.complete = false;
        }
    }
}

pub(super) fn poll(world: &mut World, owner: Entity) {
    if world.get::<RunView>(owner).is_none_or(|view| view.complete || view.pending || Instant::now() < view.next) { return; }
    if crate::native::laboratory::blocked(world, owner) { return; }
    if !current(world, owner) { super::close(world, owner); return; }
    {
        let mut view = world.get_mut::<RunView>(owner).unwrap();
        if view.follow && view.changes.as_ref().is_some_and(|changes| changes.take()) { view.requested = true; }
    }
    resize(world, owner);
    let Some(view) = world.get::<RunView>(owner) else {
        return;
    };
    if view.pending
        || view.complete
        || !view.requested
        || Instant::now() < view.next
    {
        return;
    }
    let request = ClientMessage::Command {
        id: view.request.clone(),
        request: Request::Read {
            command: view.command.clone(),
            run: view.run.clone(),
            offset: view.offset,
        },
    };
    if panel::send(world, owner, request).is_ok() {
        let mut view = world.get_mut::<RunView>(owner).unwrap();
        view.pending = true;
        view.requested = false;
    } else {
        let due = crate::native::cell_bridge::CommandChanges::retry(world, owner);
        let mut view = world.get_mut::<RunView>(owner).unwrap();
        view.next = due;
        if let Some(changes) = &view.changes { changes.retain(); }
    }
}

fn resize(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<RunView>(owner) else {
        return;
    };
    let terminal = world.get::<TerminalSand>(owner).unwrap();
    if !terminal.opened || !view.follow || view.pending || Instant::now() < view.next {
        return;
    }
    let Some(node) = world
        .get::<ComputedNode>(terminal.screen)
        .filter(|node| node.size().x > 0.0)
    else {
        return;
    };
    let size = node.size() * node.inverse_scale_factor();
    let geometry = (
        (size.x / CELL_WIDTH).floor().clamp(10.0, 240.0) as u16,
        (size.y / CELL_HEIGHT).floor().clamp(4.0, 100.0) as u16,
    );
    if geometry == terminal.geometry {
        return;
    }
    let request = ClientMessage::Command {
        id: view.run.clone(),
        request: Request::Resize {
            command: view.command.clone(),
            run: view.run.clone(),
            cols: geometry.0,
            rows: geometry.1,
        },
    };
    if panel::send(world, owner, request).is_ok() {
        let _ = command(
            world,
            owner,
            worker::Command::Resize(geometry.0, geometry.1),
        );
        world.get_mut::<TerminalSand>(owner).unwrap().geometry = geometry;
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::Command { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &RunView)>()
        .iter(world)
        .find(|(_, view)| view.request == *id)
        .map(|(owner, _)| owner);
    let Some(owner) = owner else {
        if let ServerMessage::Error { message, .. } = message {
            let owners: Vec<_> = world
                .query::<(Entity, &RunView)>()
                .iter(world)
                .filter(|(_, view)| view.run == *id)
                .map(|(owner, _)| owner)
                .collect();
            for owner in &owners {
                let terminal = world.get::<TerminalSand>(*owner).unwrap();
                panel::status(world, terminal.status, message);
            }
            return !owners.is_empty();
        }
        return false;
    };
    if !current(world, owner) { super::close(world, owner); return true; }
    let status = world.get::<TerminalSand>(owner).unwrap().status;
    world.get_mut::<RunView>(owner).unwrap().pending = false;
    match message {
        ServerMessage::Command {
            response:
                Response::Output {
                    events,
                    next_offset,
                    complete,
                },
            ..
        } => {
            let following = world.get::<RunView>(owner).unwrap().follow;
            let live = following && !complete && !world.get::<TerminalSand>(owner).unwrap().exited;
            if !events.is_empty() && let Err(error) =
                command(world, owner, worker::Command::Journal(events.clone(), live))
            {
                panel::status(world, status, error);
                let mut view = world.get_mut::<RunView>(owner).unwrap();
                view.requested = true;
                view.next = Instant::now() + Duration::from_millis(20);
                if let Some(changes) = &view.changes { changes.retain(); }
                if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                    wake.after(Duration::from_millis(20));
                }
                return true;
            }
            let mut view = world.get_mut::<RunView>(owner).unwrap();
            view.offset = *next_offset;
            view.complete = *complete;
            view.next = Instant::now();
            view.requested |= following && !complete && !events.is_empty();
            if *complete { view.changes = None; }
            let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
            if *complete {
                terminal.exited = true;
            }
            terminal.opened = live;
            panel::status(
                world,
                status,
                if *complete {
                    "End of saved output · read-only"
                } else if following {
                    "Following output · click the terminal to type"
                } else {
                    "Saved output · Next output advances through the full run"
                },
            );
        }
        ServerMessage::Error { message, .. } => {
            world.get_mut::<RunView>(owner).unwrap().complete = true;
            world.get_mut::<RunView>(owner).unwrap().changes = None;
            let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
            terminal.opened = false;
            terminal.input.clear();
            panel::status(world, status, message);
        }
        _ => {}
    }
    true
}

pub(super) fn detach(world: &mut World, owner: Entity) {
    if let Some(mut view) = world.get_mut::<RunView>(owner) {
        view.complete = true;
        view.pending = false;
        view.requested = false;
        view.changes = None;
        view.request = nucleus::new_uid("output");
    }
}

fn current(world: &World, owner: Entity) -> bool {
    world.get::<RunView>(owner).is_some_and(|view| view.source.as_ref().is_none_or(|source|
        crate::native::practice_cells::command_source(world, owner).is_some_and(|current| source.same(&current))))
}

#[cfg(test)]
mod tests;
