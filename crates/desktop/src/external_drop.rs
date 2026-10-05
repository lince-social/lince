#[cfg(target_os = "linux")]
mod native;
mod source;
pub(crate) mod tests;
mod ui;
mod worker;

use crate::actions::Action;
use bevy::{ecs::message::MessageCursor, math::DVec2, prelude::*, window::FileDragAndDrop};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Component)]
pub(crate) struct Preview;

#[derive(Resource, Default)]
struct Sessions {
    next: u64,
    entries: HashMap<Entity, Session>,
}

struct Session {
    id: u64,
    window: Option<Entity>,
    workspace: u64,
    position: DVec2,
    elevation: f64,
    source: source::Source,
    hovering: bool,
    cancelled: Arc<AtomicBool>,
    prepared: Option<worker::Prepared>,
    preview: Option<Entity>,
    panel: Option<Entity>,
    status: String,
    busy: bool,
    ready: bool,
}

#[derive(Resource, Default)]
struct Pulse {
    active: bool,
    sender: Option<std::sync::mpsc::Sender<bool>>,
    task: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Pulse {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

fn pulse(world: &mut World) {
    let active = world
        .resource::<Sessions>()
        .entries
        .values()
        .any(|session| {
            session.hovering || session.busy || session.prepared.is_some() && !session.ready
        });
    if active
        && world.resource::<Pulse>().sender.is_none()
        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = std::thread::spawn(move || {
            let mut active = false;
            loop {
                let result = if active {
                    receiver.recv_timeout(std::time::Duration::from_millis(16))
                } else {
                    receiver
                        .recv()
                        .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
                };
                match result {
                    Ok(next) => active = next,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => wake.ring(),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        let mut pulse = world.resource_mut::<Pulse>();
        let _ = sender.send(active);
        pulse.active = active;
        pulse.sender = Some(sender);
        pulse.task = Some(task);
    }
    let mut pulse = world.resource_mut::<Pulse>();
    if pulse.active != active {
        if let Some(sender) = &pulse.sender {
            let _ = sender.send(active);
        }
        pulse.active = active;
    }
}

pub struct ExternalDropPlugin;

impl Plugin for ExternalDropPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sessions>()
            .init_resource::<Pulse>()
            .init_resource::<worker::Worker>()
            .add_message::<FileDragAndDrop>()
            .add_systems(
                Update,
                (events, update)
                    .chain()
                    .after(crate::workspace::PrepareWorkspaces)
                    .after(crate::topology::assets::update),
            )
            .add_systems(
                PostUpdate,
                ui::tint
                    .after(crate::token_style::ApplyTokenStyles)
                    .after(crate::document_viewer::RenderDocuments)
                    .before(bevy::ui::UiSystems::Layout),
            );
    }
}

fn focused_draft(world: &mut World, point: Vec2) -> bool {
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let owner = world
        .query::<(Entity, &crate::message_content::Draft)>()
        .iter(world)
        .find(|(_, draft)| Some(draft.input) == focus && !draft.locked)
        .map(|(owner, _)| owner);
    let Some(owner) = owner else {
        return false;
    };
    let Some(hover) = world.get_resource::<bevy::picking::hover::HoverMap>() else {
        return false;
    };
    [
        bevy::picking::pointer::PointerId::Mouse,
        crate::topology::input::CONTENT_POINTER,
    ]
    .iter()
    .any(|pointer| {
        hover.get(pointer).is_some_and(|hits| {
            hits.keys().any(|entity| {
                if crate::inspection::bounds(world, *entity)
                    .is_some_and(|bounds| !bounds.contains(point))
                {
                    return false;
                }
                let mut cursor = Some(*entity);
                while let Some(entity) = cursor {
                    if entity == owner {
                        return true;
                    }
                    cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                false
            })
        })
    })
}

pub(crate) fn receives(world: &mut World, window: Entity) -> bool {
    world.contains_resource::<Sessions>() && target(world, window).is_some()
}

fn target(world: &mut World, window: Entity) -> Option<(Entity, DVec2, f64)> {
    #[cfg(target_os = "linux")]
    let point = native::position(world, window).unwrap_or_else(|| {
        world
            .get::<Window>(window)
            .and_then(Window::cursor_position)
    })?;
    #[cfg(not(target_os = "linux"))]
    let point = world.get::<Window>(window)?.cursor_position()?;
    if focused_draft(world, point) {
        return None;
    }
    let primary = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<crate::container::BoxRoot>>()
        .iter(world)
        .collect();
    for root in roots {
        if crate::laboratory::suspended(world, root) {
            continue;
        }
        let Some(bounds) = crate::inspection::bounds(world, root) else {
            continue;
        };
        if !bounds.contains(point) {
            continue;
        }
        if let Some(camera) = world
            .get::<bevy::ui::ComputedUiTargetCamera>(root)
            .and_then(|camera| camera.get())
            && let Some(bevy::camera::RenderTarget::Window(reference)) =
                world.get::<bevy::camera::RenderTarget>(camera)
            && reference.normalize(primary)
                != bevy::window::WindowRef::Entity(window).normalize(primary)
        {
            continue;
        }
        if ui::blocked(world, root, point) {
            continue;
        }
        let elevation = world
            .get::<crate::topology::view::View>(root)
            .map_or(0.0, |view| view.plane);
        let position = if world.contains_resource::<crate::topology::presentation::SceneCamera>() {
            let point = crate::topology::input::plane_point(world, root, point, elevation)?;
            DVec2::new(point.x, point.z)
        } else {
            let view = world.get::<crate::canvas::CanvasView>(root)?;
            view.center + (point - bounds.center()).as_dvec2() / view.zoom
        };
        return Some((root, position, elevation));
    }
    None
}

fn events(world: &mut World, mut cursor: Local<MessageCursor<FileDragAndDrop>>) {
    let events: Vec<_> = world
        .get_resource::<Messages<FileDragAndDrop>>()
        .map(|events| cursor.read(events).take(32).cloned().collect())
        .unwrap_or_default();
    for event in events {
        let hovering = matches!(event, FileDragAndDrop::HoveredFile { .. });
        match event {
            FileDragAndDrop::HoveredFileCanceled { window } => {
                let roots: Vec<_> = world
                    .resource::<Sessions>()
                    .entries
                    .iter()
                    .filter(|(_, session)| session.window == Some(window) && session.hovering)
                    .map(|(root, _)| *root)
                    .collect();
                for root in roots {
                    cancel(world, root);
                }
            }
            FileDragAndDrop::HoveredFile { window, path_buf }
            | FileDragAndDrop::DroppedFile { window, path_buf } => {
                let Some((root, position, elevation)) = target(world, window) else {
                    if !hovering {
                        let roots: Vec<_> = world
                            .resource::<Sessions>()
                            .entries
                            .iter()
                            .filter(|(_, session)| {
                                session.window == Some(window) && session.hovering
                            })
                            .map(|(root, _)| *root)
                            .collect();
                        for root in roots {
                            cancel(world, root);
                        }
                    }
                    continue;
                };
                let source = source::Source::from_path(path_buf);
                if let Some(session) = world.resource_mut::<Sessions>().entries.get_mut(&root)
                    && session.source == source
                    && session.window == Some(window)
                    && session.hovering
                {
                    session.position = position;
                    session.elevation = elevation;
                    if !hovering {
                        session.hovering = false;
                        ui::chooser(world, root);
                    }
                    continue;
                }
                if world
                    .resource::<Sessions>()
                    .entries
                    .get(&root)
                    .is_some_and(|session| !session.hovering)
                {
                    crate::notifications::report(
                        world,
                        "Drop onto Box",
                        "Finish or cancel the current file choice before dropping another file.",
                    );
                    continue;
                }
                begin(
                    world,
                    root,
                    Some(window),
                    source,
                    position,
                    elevation,
                    hovering,
                );
            }
        }
    }
}

fn begin(
    world: &mut World,
    root: Entity,
    window: Option<Entity>,
    source: source::Source,
    position: DVec2,
    elevation: f64,
    hovering: bool,
) {
    cancel(world, root);
    let Some(workspace) = world
        .get::<crate::workspace::Workspaces>(root)
        .map(|spaces| spaces.active)
    else {
        return;
    };
    let mut sessions = world.resource_mut::<Sessions>();
    sessions.next = sessions.next.wrapping_add(1);
    let id = sessions.next;
    sessions.entries.insert(
        root,
        Session {
            id,
            window,
            workspace,
            position,
            elevation,
            source,
            hovering,
            cancelled: Arc::new(AtomicBool::new(false)),
            prepared: None,
            preview: None,
            panel: None,
            status: "Preparing preview…".into(),
            busy: false,
            ready: false,
        },
    );
    start(world, root);
    ui::chooser(world, root);
}

fn start(world: &mut World, root: Entity) {
    let session = &world.resource::<Sessions>().entries[&root];
    if session.busy || session.prepared.is_some() {
        return;
    }
    if matches!(session.source, source::Source::Url(_)) {
        world.resource_mut::<Sessions>().entries.get_mut(&root).unwrap().status = "Choose Link Sand to keep the URL, or Download preview to inspect the file. Web pages have no embedded viewer.".into();
        return;
    }
    enqueue(world, root);
}

fn enqueue(world: &mut World, root: Entity) {
    let session = &world.resource::<Sessions>().entries[&root];
    let directory = world
        .get_resource::<crate::topology::assets::AssetDirectory>()
        .map(|directory| directory.0.clone());
    let job = worker::Job {
        root,
        id: session.id,
        source: session.source.clone(),
        directory,
        cancelled: session.cancelled.clone(),
        wake: world.get_resource::<crate::wake::WakeSignal>().cloned(),
    };
    let result = world.resource::<worker::Worker>().sender.try_send(job);
    let session = world
        .resource_mut::<Sessions>()
        .into_inner()
        .entries
        .get_mut(&root)
        .unwrap();
    match result {
        Ok(()) => {
            session.busy = true;
            session.status = "Preparing preview…".into();
        }
        Err(_) => {
            session.status =
                "The preview queue is full. Cancel and try again when the current preview finishes."
                    .into()
        }
    }
}

fn cancel(world: &mut World, root: Entity) {
    if let Some(mut session) = world.resource_mut::<Sessions>().entries.remove(&root) {
        session.cancelled.store(true, Ordering::Release);
        if let Some(entity) = session.preview.take() {
            world.despawn(entity);
        }
        if let Some(entity) = session.panel.take() {
            world.despawn(entity);
        }
    }
}

#[derive(Component)]
struct CancelledChoice;

pub(crate) fn prepare_choice(world: &mut World, root: Entity, path: std::path::PathBuf) {
    if world.resource::<Sessions>().entries.contains_key(&root) {
        return;
    }
    world.entity_mut(root).remove::<CancelledChoice>();
    begin(
        world,
        root,
        None,
        source::Source::from_path(path),
        DVec2::ZERO,
        0.0,
        false,
    );
}

pub(crate) fn choice_owner(world: &World, root: Entity) -> Option<Entity> {
    world.get_resource::<Sessions>()?.entries.get(&root)?.panel
}

pub(crate) fn choice_ready(world: &World, root: Entity) -> bool {
    world.get_resource::<Sessions>().is_some_and(|sessions| {
        sessions.entries.get(&root).is_some_and(|session| {
            session.prepared.is_some() && session.ready && !session.busy && session.panel.is_some()
        })
    })
}

pub(crate) fn cancel_choice(world: &mut World, root: Entity) {
    ui::Control::Cancel.apply(world, root);
}

pub(crate) fn cancel_owned_choice(world: &mut World, root: Entity, directory: &std::path::Path) {
    let owned = world.get_resource::<Sessions>().and_then(|sessions| sessions.entries.get(&root)).is_some_and(|session| matches!(&session.source, source::Source::File(path) if path.starts_with(directory)));
    if owned { cancel_choice(world, root); }
}

pub(crate) fn choice_cancelled(world: &World, root: Entity) -> bool {
    world.get::<CancelledChoice>(root).is_some() && choice_owner(world, root).is_none()
}

fn update(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Sessions>()
        .entries
        .iter()
        .filter(|(root, session)| {
            world
                .get::<crate::workspace::Workspaces>(**root)
                .is_none_or(|spaces| spaces.active != session.workspace)
                || crate::laboratory::suspended(world, **root)
        })
        .map(|(root, _)| *root)
        .collect();
    for root in stale {
        cancel(world, root);
    }
    let replies: Vec<_> = world
        .resource::<worker::Worker>()
        .replies
        .lock()
        .unwrap()
        .try_iter()
        .collect();
    for (root, id, result) in replies {
        if !world
            .resource::<Sessions>()
            .entries
            .get(&root)
            .is_some_and(|session| session.id == id)
        {
            continue;
        }
        let session = world
            .resource_mut::<Sessions>()
            .into_inner()
            .entries
            .get_mut(&root)
            .unwrap();
        session.busy = false;
        match result {
            Ok(prepared) => {
                session.status = prepared.explanation();
                session.prepared = Some(prepared);
                ui::preview(world, root);
            }
            Err(error) => session.status = error,
        }
        ui::chooser(world, root);
    }
    let roots: Vec<_> = world
        .resource::<Sessions>()
        .entries
        .keys()
        .copied()
        .collect();
    for root in roots {
        let session = &world.resource::<Sessions>().entries[&root];
        let state = session.preview.and_then(|preview| {
            if world.get_entity(preview).is_err() { return Some(Err("The file could not be loaded by this Sand. See the viewer notification for details.".into())); }
            match session.prepared.as_ref()?.kind {
                source::Kind::Image | source::Kind::Text => Some(Ok(())),
                source::Kind::Document => crate::document_viewer::preview_status(world, preview),
                source::Kind::Model => world.get::<crate::topology::assets::Ready>(preview).map(|_| Ok(())),
            }
        });
        if let Some(state) = state {
            match state {
                Ok(()) if !session.ready => {
                    world
                        .resource_mut::<Sessions>()
                        .entries
                        .get_mut(&root)
                        .unwrap()
                        .ready = true;
                    ui::chooser(world, root);
                }
                Err(error) => {
                    let session = world
                        .resource_mut::<Sessions>()
                        .into_inner()
                        .entries
                        .get_mut(&root)
                        .unwrap();
                    let preview = session.preview.take();
                    session.prepared = None;
                    session.ready = false;
                    session.status = error;
                    if let Some(preview) = preview {
                        world.despawn(preview);
                    }
                    ui::chooser(world, root);
                }
                _ => {}
            }
        }
        let session = &world.resource::<Sessions>().entries[&root];
        let (hovering, window, preview) = (session.hovering, session.window, session.preview);
        if hovering && let Some(window) = window {
            if let Some((target_root, position, elevation)) = target(world, window) {
                if target_root == root {
                    let session = world
                        .resource_mut::<Sessions>()
                        .into_inner()
                        .entries
                        .get_mut(&root)
                        .unwrap();
                    session.position = position;
                    session.elevation = elevation;
                    if let Some(preview) = preview {
                        crate::topology::set_position(
                            world,
                            preview,
                            bevy::math::DVec3::new(position.x, elevation, position.y),
                        );
                        world.entity_mut(preview).insert(Visibility::Visible);
                    }
                }
            } else if let Some(preview) = preview {
                world.entity_mut(preview).insert(Visibility::Hidden);
            }
        }
    }
    pulse(world);
}
