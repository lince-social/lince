use super::*;
use crate::{
    host::{FileDragContext, HostRequest, OverviewHoverRequest, WakeSignal},
    layout::desktop::{HoverClock, OverviewSettings},
};
use std::{
    sync::mpsc::TrySendError,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Host {
        window: Entity,
        drag: FileDragContext,
    },
    Window {
        window: Entity,
        token: u64,
    },
}

impl Source {
    fn window(self) -> Entity {
        match self {
            Self::Host { window, .. } | Self::Window { window, .. } => window,
        }
    }
}

#[derive(Resource, Default)]
struct Incoming {
    source: Option<Source>,
    next: u64,
    drop: Option<DropGuard>,
}

#[derive(Clone, Copy)]
struct DropGuard {
    window: Entity,
    root: Entity,
    workspace: u64,
    reported: bool,
}

struct Timer {
    deadline: Instant,
    task: tokio::task::AbortHandle,
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Pending {
    request: OverviewHoverRequest,
    sent: bool,
}

pub(super) struct FileHover {
    source: Source,
    drag: FileDragContext,
    active: u64,
    viewport: Option<OutputViewport>,
    bounds: Rectangle,
    settings: OverviewSettings,
    clock: HoverClock,
    previous: Instant,
    timer: Option<Timer>,
    outlined: Option<Entity>,
    pending: Option<Pending>,
}

fn clear(world: &mut World, session: &mut Session) {
    if let Some(hover) = session.hover.take() {
        if let Some(entity) = hover.outlined
            && let Some(mut outline) = world.get_mut::<Outline>(entity)
        {
            outline.width = px(0);
        }
    }
}

fn replace(world: &mut World, source: Option<Source>) {
    world.init_resource::<Incoming>();
    if world.resource::<Incoming>().source == source {
        return;
    }
    world.resource_mut::<Incoming>().source = source;
    world.resource_mut::<Incoming>().drop = None;
    if let Some(mut overview) = world.remove_resource::<Overview>() {
        if let Some(session) = &mut overview.0 {
            clear(world, session);
            session.hover_retired = None;
        }
        world.insert_resource(overview);
    }
}

pub fn file_drag(world: &mut World, window: Entity, drag: Option<FileDragContext>) {
    replace(
        world,
        drag.filter(|drag| drag.token != 0)
            .map(|drag| Source::Host { window, drag }),
    );
}

pub(crate) fn active_file_drag(world: &World) -> bool {
    world.get_resource::<Incoming>().is_some_and(|incoming| incoming.source.is_some())
}

pub(crate) fn file_event(world: &mut World, event: &bevy::window::FileDragAndDrop) {
    use bevy::window::FileDragAndDrop;
    world.init_resource::<Incoming>();
    if matches!(
        world.resource::<Incoming>().source,
        Some(Source::Host { .. })
    ) {
        return;
    }
    match *event {
        FileDragAndDrop::HoveredFile { window, .. } => {
            world.insert_resource(crate::host::FileDragSupport { available: true, reason: None });
            if matches!(world.resource::<Incoming>().source, Some(Source::Window { window: active, .. }) if active == window)
            {
                return;
            }
            let Some(token) = world.resource::<Incoming>().next.checked_add(1) else {
                return;
            };
            world.resource_mut::<Incoming>().next = token;
            replace(world, Some(Source::Window { window, token }));
        }
        FileDragAndDrop::HoveredFileCanceled { window } => {
            if matches!(world.resource::<Incoming>().source, Some(Source::Window { window: active, .. }) if active == window)
            {
                replace(world, None);
            }
        }
        FileDragAndDrop::DroppedFile { window, .. } => {
            if matches!(world.resource::<Incoming>().source, Some(Source::Window { window: active, .. }) if active == window)
            {
                let drop = world.resource::<Incoming>().drop;
                replace(world, None);
                world.resource_mut::<Incoming>().drop = drop;
            }
        }
    }
}

pub(crate) fn drop_allowed(world: &mut World, window: Entity) -> bool {
    let Some(drop) = world
        .get_resource::<Incoming>()
        .and_then(|incoming| incoming.drop)
    else {
        return true;
    };
    if drop.window != window
        || world
            .get::<Workspaces>(drop.root)
            .is_some_and(|spaces| spaces.active == drop.workspace)
    {
        return true;
    }
    if !drop.reported {
        world
            .resource_mut::<Incoming>()
            .drop
            .as_mut()
            .unwrap()
            .reported = true;
        crate::native::notifications::report(
            world,
            "Drop",
            "The requested workspace has not opened; retry the drop after it becomes visible.",
        );
    }
    false
}

fn local_window(world: &mut World, root: Entity, window: Entity) -> bool {
    let primary = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();
    let Some(camera) = world
        .get::<bevy::ui::ComputedUiTargetCamera>(root)
        .and_then(|camera| camera.get())
        .or_else(|| {
            world
                .get::<bevy::ui::UiTargetCamera>(root)
                .map(|camera| camera.entity())
        })
    else {
        return false;
    };
    if !world
        .get::<Camera>(camera)
        .is_some_and(|camera| camera.is_active)
    {
        return false;
    }
    match world.get::<bevy::camera::RenderTarget>(camera) {
        Some(bevy::camera::RenderTarget::Window(reference)) => {
            reference.normalize(primary)
                == bevy::window::WindowRef::Entity(window).normalize(primary)
        }
        _ => false,
    }
}

fn context(world: &mut World, session: &Session, source: Source) -> Option<FileDragContext> {
    let focused = world.get::<Window>(source.window())?.focused;
    match source {
        Source::Host { drag, .. } => {
            (focused && drag.owner == session.root.to_bits() && drag.output == session.output)
                .then_some(drag)
        }
        Source::Window { window, token } => {
            local_window(world, session.root, window).then_some(FileDragContext {
                owner: session.root.to_bits(),
                output: session.output,
                token,
            })
        }
    }
}

fn valid(world: &mut World, session: &Session, hover: &FileHover) -> bool {
    dragging::context(world, session)
        && world
            .get_resource::<Incoming>()
            .and_then(|incoming| incoming.source)
            == Some(hover.source)
        && context(world, session, hover.source) == Some(hover.drag)
        && super::super::viewport(world, session.root) == Some(hover.bounds)
        && world
            .get::<HostViewport>(session.root)
            .map(|viewport| &viewport.0)
            == hover.viewport.as_ref()
        && world
            .get::<Workspaces>(session.root)
            .is_some_and(|spaces| spaces.active == hover.active)
}

fn destination(world: &World, session: &Session, point: Vec2) -> Option<(u64, Entity)> {
    session.cards.iter().rev().find_map(|card| {
        dragging::rectangle(world, card.entity).filter(|bounds| bounds.contains(point))?;
        valid_selection(world, session.root, card.workspace, None)
            .then_some((card.workspace, card.preview))
    })
}

fn arm(world: &World, hover: &mut FileHover, deadline: Option<Instant>) {
    if hover.timer.as_ref().map(|timer| timer.deadline) == deadline {
        return;
    }
    hover.timer = None;
    if let Some(deadline) = deadline
        && let Some(wake) = world.get_resource::<WakeSignal>().cloned()
        && let Ok(runtime) = tokio::runtime::Handle::try_current()
    {
        let task = runtime.spawn(async move {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
            wake.ring();
        });
        hover.timer = Some(Timer {
            deadline,
            task: task.abort_handle(),
        });
    }
}

pub(super) fn update(
    world: &mut World,
    session: &mut Session,
    now: Instant,
) -> Option<OverviewHoverRequest> {
    let source = world
        .get_resource::<Incoming>()
        .and_then(|incoming| incoming.source);
    let Some(source) = source.filter(|source| session.hover_retired != Some(*source)) else {
        clear(world, session);
        return None;
    };
    if session.drag.is_some() || !dragging::context(world, session) {
        clear(world, session);
        return None;
    }
    if session.hover.is_none() {
        let drag = context(world, session, source)?;
        session.hover = Some(FileHover {
            source,
            drag,
            active: world.get::<Workspaces>(session.root)?.active,
            viewport: world
                .get::<HostViewport>(session.root)
                .map(|viewport| viewport.0.clone()),
            bounds: super::super::viewport(world, session.root)?,
            settings: settings::current(world),
            clock: HoverClock::default(),
            previous: now,
            timer: None,
            outlined: None,
            pending: None,
        });
    }
    let mut hover = session.hover.take().unwrap();
    if !valid(world, session, &hover) {
        session.hover = Some(hover);
        clear(world, session);
        session.hover_retired = Some(source);
        return None;
    }
    if let Some(entity) = hover.outlined.take()
        && let Some(mut outline) = world.get_mut::<Outline>(entity)
    {
        outline.width = px(0);
    }
    let point = world
        .get::<Window>(source.window())
        .and_then(Window::cursor_position)
        .filter(|point| point.is_finite());
    let scrolling = point.is_some_and(|point| {
        dragging::scroll(
            world,
            session.list,
            point,
            now.saturating_duration_since(hover.previous),
            hover.settings,
        )
    });
    hover.previous = now;
    let target = point.and_then(|point| destination(world, session, point));
    let workspace = target.map(|(workspace, _)| workspace);
    if hover.clock.target() != workspace {
        hover.pending = None;
    }
    let ready = hover.clock.update(
        workspace,
        now,
        Duration::from_millis(u64::from(hover.settings.hover_delay_ms)),
    );
    if let Some((_, entity)) = target {
        world.entity_mut(entity).insert(Outline {
            width: px(3),
            offset: px(2),
            color: crate::native::theme::PURPLE,
        });
        hover.outlined = Some(entity);
    }
    if let Some(workspace) = ready {
        let Some(sequence) = session.hover_sequence.checked_add(1) else {
            session.hover = Some(hover);
            clear(world, session);
            session.hover_retired = Some(source);
            return None;
        };
        session.hover_sequence = sequence;
        hover.pending = Some(Pending {
            request: OverviewHoverRequest {
                drag: hover.drag,
                modal: session.overlay.to_bits(),
                sequence,
                workspace,
            },
            sent: false,
        });
    }
    let mut activation = None;
    let mut retry = false;
    if let Some(pending) = hover.pending.as_mut().filter(|pending| !pending.sent) {
        if let Some(requests) = world.get_resource::<HostRequests>() {
            match requests
                .0
                .try_send(HostRequest::OverviewHover(pending.request))
            {
                Ok(()) => pending.sent = true,
                Err(TrySendError::Full(_)) => retry = true,
                Err(TrySendError::Disconnected(_)) => {
                    session.hover_retired = Some(source);
                    session.hover = Some(hover);
                    clear(world, session);
                    return None;
                }
            }
        } else if matches!(hover.source, Source::Window { .. }) {
            pending.sent = true;
            activation = Some(pending.request);
        } else {
            session.hover_retired = Some(source);
            session.hover = Some(hover);
            clear(world, session);
            return None;
        }
    }
    let mut deadline = hover.clock.deadline();
    if scrolling || retry {
        let next = now.checked_add(Duration::from_millis(16));
        deadline = match (deadline, next) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
    arm(world, &mut hover, deadline);
    session.hover = Some(hover);
    activation
}

pub fn acknowledge_hover(world: &mut World, request: OverviewHoverRequest, accepted: bool) -> bool {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return false;
    };
    let result = overview.0.as_mut().is_some_and(|session| {
        let Some(hover) = session.hover.take() else {
            return false;
        };
        let matched = hover
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent && pending.request == request);
        if !matched {
            session.hover = Some(hover);
            return false;
        }
        let point = world
            .get::<Window>(hover.source.window())
            .and_then(Window::cursor_position);
        let result = accepted
            && valid(world, session, &hover)
            && point
                .and_then(|point| destination(world, session, point))
                .is_some_and(|(workspace, _)| workspace == request.workspace);
        if result && let Source::Window { window, .. } = hover.source {
            world.resource_mut::<Incoming>().drop = Some(DropGuard {
                window,
                root: session.root,
                workspace: request.workspace,
                reported: false,
            });
        }
        session.hover_retired = Some(hover.source);
        session.hover = Some(hover);
        clear(world, session);
        result
    });
    world.insert_resource(overview);
    result
}

#[cfg(test)]
mod tests;
