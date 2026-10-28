mod credits;
pub(crate) use credits::CREDITS;
mod persistence;
mod runtime;
#[cfg(test)]
mod tests;
mod ui;
mod worker;

use crate::native::actions::Action;
use bevy::{math::DVec2, prelude::*};
use lince_document::{Info, Layout, Mode, Position};
pub(crate) use persistence::{SavedDocumentViewer, snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct DocumentViewer {
    pub path: String,
    #[serde(default)]
    pub resource: Option<lince_resources::Resource>,
    pub positions: BTreeMap<String, Position>,
    pub zoom: f32,
}

impl Default for DocumentViewer {
    fn default() -> Self {
        Self {
            path: String::new(),
            resource: None,
            positions: BTreeMap::new(),
            zoom: 1.0,
        }
    }
}

impl DocumentViewer {
    pub fn with_resource(resource: lince_resources::Resource) -> Self {
        Self {
            path: resource.uri.clone(),
            resource: Some(resource),
            ..default()
        }
    }
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            ..default()
        }
    }

    pub fn position(&self) -> Position {
        self.positions.get(&self.path).cloned().unwrap_or_default()
    }

    pub fn valid(&self) -> bool {
        valid_path(&self.path)
            && self
                .resource
                .as_ref()
                .is_none_or(|resource| resource.validate().is_ok())
            && self.positions.len() <= 128
            && self
                .positions
                .iter()
                .all(|(path, position)| valid_path(path) && position.valid())
            && self.zoom.is_finite()
            && (0.5..=2.0).contains(&self.zoom)
    }

    fn position_mut(&mut self) -> &mut Position {
        if !self.positions.contains_key(&self.path) && self.positions.len() >= 128 {
            self.positions.pop_first();
        }
        self.positions.entry(self.path.clone()).or_default()
    }
}

fn valid_path(path: &str) -> bool {
    path.len() <= 8192 && !path.contains('\0')
}

#[derive(Component)]
struct View {
    input: Entity,
    status: Entity,
    viewport: Entity,
    content: Entity,
    mode_label: Entity,
    section_input: Entity,
    info: Option<Info>,
    layout: Option<Layout>,
    key: Option<worker::Key>,
    busy: bool,
    failed: bool,
    restore: bool,
    epoch: u64,
    tiles: BTreeMap<u32, (Entity, Handle<Image>)>,
    requested: BTreeSet<u32>,
    viewport_size: Vec2,
    picking: bool,
    retention: Option<String>,
}

pub struct DocumentViewerPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RenderDocuments;

impl Plugin for DocumentViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<worker::Worker>()
            .add_systems(
                PostUpdate,
                runtime::update
                    .in_set(RenderDocuments)
                    .after(crate::native::actions::ApplyActions)
                    .before(bevy::ui::UiSystems::Layout),
            )
            .add_systems(Update, cleanup);
    }
}

fn cleanup(mut removed: RemovedComponents<DocumentViewer>, worker: Res<worker::Worker>) {
    for owner in removed.read() {
        let _ = worker.sender.try_send(worker::Job::Close(owner));
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    document: DocumentViewer,
) -> Entity {
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(credits::CREDITS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(760.0, 880.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            document,
        ))
        .id();
    crate::native::edit_mode::label(world, owner, "Document Viewer Castle", 24.0);
    populate(world, owner, owner);
    owner
}

pub(crate) fn embedded(
    world: &mut World,
    parent: Entity,
    controls: Entity,
    resource: lince_resources::Resource,
) -> Entity {
    let owner = world
        .spawn((
            DocumentViewer::with_resource(resource),
            Node {
                width: percent(100),
                height: px(560),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::sand_store::SandCredits(CREDITS),
            ChildOf(parent),
        ))
        .id();
    populate(world, owner, controls);
    let viewport = world.get::<View>(owner).unwrap().viewport;
    world
        .entity_mut(viewport)
        .insert((
            InlineViewport(owner),
            crate::native::embedded_media::OwnsMediaScroll,
        ))
        .observe(inline_scroll);
    owner
}

#[derive(Component)]
struct InlineViewport(Entity);

fn inline_scroll(
    mut event: On<Pointer<Scroll>>,
    viewports: Query<&InlineViewport>,
    mut commands: Commands,
) {
    let Ok(viewport) = viewports.get(event.entity) else {
        return;
    };
    let owner = viewport.0;
    let factor = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
        24.0
    } else {
        1.0
    };
    let delta = -Vec2::new(event.x, event.y) * factor;
    let pointer = event.pointer_id;
    let location = event.pointer_location.clone();
    let input = event.event().clone();
    event.propagate(false);
    commands.queue(move |world: &mut World| {
        if !scroll_inline(world, owner, delta)
            && let Some(parent) = world.get::<ChildOf>(owner).map(ChildOf::parent)
        {
            world.trigger(Pointer::new(pointer, location, input, parent));
        }
    });
}

fn scroll_inline(world: &mut World, owner: Entity, delta: Vec2) -> bool {
    let Some(view) = world.get::<View>(owner) else {
        return false;
    };
    let Some(layout) = view.layout else {
        return false;
    };
    let state = world.get::<DocumentViewer>(owner).unwrap();
    let count = view.info.as_ref().map_or(0, |info| info.sections.len());
    let viewport = view.viewport;
    let width = runtime::display_width(state, view);
    let height = layout.height as f32 * width / layout.width as f32;
    let maximum = Vec2::new(
        (width - view.viewport_size.x).max(0.0),
        (height - view.viewport_size.y).max(0.0),
    );
    let old = world.get::<ScrollPosition>(viewport).unwrap().0;
    let next = (old + delta).clamp(Vec2::ZERO, maximum);
    if next != old {
        world.get_mut::<ScrollPosition>(viewport).unwrap().0 = next;
        return true;
    }
    let section = state.position().section;
    let forward = delta.y > 0.0;
    if delta.y != 0.0 && ((forward && section + 1 < count) || (!forward && section > 0)) {
        let position = world
            .get_mut::<DocumentViewer>(owner)
            .unwrap()
            .into_inner()
            .position_mut();
        position.section = if forward { section + 1 } else { section - 1 };
        position.fraction = if forward { 0.0 } else { 1.0 };
        world.get_mut::<View>(owner).unwrap().restore = true;
        world.get_mut::<View>(owner).unwrap().layout = None;
        return true;
    }
    false
}

fn populate(world: &mut World, owner: Entity, controls: Entity) {
    let path = world.get::<DocumentViewer>(owner).unwrap().path.clone();
    let row = ui::row(world, controls);
    let input = ui::input(world, row, "PDF or EPUB file path", &path, 4096);
    world.get_mut::<Node>(input).unwrap().flex_grow = 1.0;
    ui::button(world, row, owner, "Open", ui::Control::Open);
    ui::button(world, row, owner, "Browse…", ui::Control::Browse);
    ui::button(world, row, owner, "Keep offline", ui::Control::KeepOffline);
    let toolbar = ui::row(world, controls);
    ui::button(world, toolbar, owner, "Previous", ui::Control::Previous);
    ui::button(world, toolbar, owner, "Next", ui::Control::Next);
    let mode = ui::button(world, toolbar, owner, "Scroll mode", ui::Control::Mode);
    let mode_label = world.get::<Children>(mode).unwrap()[0];
    ui::button(world, toolbar, owner, "−", ui::Control::Zoom(-0.25));
    ui::button(world, toolbar, owner, "+", ui::Control::Zoom(0.25));
    let navigation = ui::row(world, controls);
    crate::native::edit_mode::label(world, navigation, "PDF page / EPUB chapter", 13.0);
    let section_input = ui::input(world, navigation, "Page or chapter number", "1", 6);
    world.get_mut::<Node>(section_input).unwrap().width = px(65);
    ui::button(world, navigation, owner, "Go", ui::Control::Go);
    let status = crate::native::edit_mode::label(
        world,
        controls,
        "Open a PDF or EPUB. Your place is saved with the workspace.",
        13.0,
    );
    let viewport = world
        .spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::WHITE),
            ChildOf(owner),
        ))
        .id();
    crate::native::scroll_sand::attach(world, viewport);
    let content = world
        .spawn((
            Node {
                width: percent(100),
                height: px(1),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(viewport),
            bevy::ui::LayoutConfig {
                use_rounding: false,
            },
        ))
        .id();
    world.entity_mut(owner).insert(View {
        input,
        status,
        viewport,
        content,
        mode_label,
        section_input,
        info: None,
        retention: None,
        layout: None,
        key: None,
        busy: false,
        failed: false,
        restore: true,
        epoch: 0,
        tiles: BTreeMap::new(),
        requested: BTreeSet::new(),
        viewport_size: Vec2::ZERO,
        picking: false,
    });
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Document Viewer Castle",
        "Read PDF and EPUB books and keep your place across restarts.",
        ui::Control::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, DocumentViewer::default()),
    );
}

fn status(world: &mut World, owner: Entity, message: &str) {
    if let Some(entity) = world.get::<View>(owner).map(|view| view.status)
        && let Some(mut text) = world.get_mut::<Text>(entity)
    {
        if text.0 != message {
            text.0 = message.into();
        }
    }
}

pub(crate) fn preview_status(world: &World, owner: Entity) -> Option<Result<(), String>> {
    let view = world.get::<View>(owner)?;
    if view.failed {
        Some(Err(world
            .get::<Text>(view.status)
            .map_or("Cannot read this document.".into(), |text| {
                text.0.clone()
            })))
    } else if !view.tiles.is_empty() {
        Some(Ok(()))
    } else {
        None
    }
}

pub(crate) fn next_page(world: &mut World, owner: Entity) {
    if page_visible(world, owner, 0) {
        ui::Control::Next.apply(world, owner);
    }
}

pub(crate) fn page_visible(world: &World, owner: Entity, section: usize) -> bool {
    world
        .get::<DocumentViewer>(owner)
        .is_some_and(|state| state.position().section == section)
        && world.get::<View>(owner).is_some_and(|view| {
            !view.failed
                && !view.busy
                && !view.tiles.is_empty()
                && view.key.as_ref().is_some_and(|key| key.section == section)
        })
}
