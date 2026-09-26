mod credits;
mod persistence;
mod runtime;
#[cfg(test)]
mod tests;
mod ui;
mod worker;

use bevy::{math::DVec2, prelude::*};
use lince_document::{Info, Layout, Mode, Position};
pub(crate) use persistence::{SavedDocumentViewer, snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct DocumentViewer {
    pub path: String,
    pub positions: BTreeMap<String, Position>,
    pub zoom: f32,
}

impl Default for DocumentViewer {
    fn default() -> Self {
        Self {
            path: String::new(),
            positions: BTreeMap::new(),
            zoom: 1.0,
        }
    }
}

impl DocumentViewer {
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
    path.len() <= 4096 && !path.contains('\0')
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
}

pub struct DocumentViewerPlugin;

impl Plugin for DocumentViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<worker::Worker>()
            .add_systems(
                PostUpdate,
                runtime::update
                    .after(crate::actions::ApplyActions)
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
    let path = document.path.clone();
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            crate::workspace::WorkspaceMember(workspace),
            crate::sand_store::SandCredits(credits::CREDITS),
            crate::canvas::CanvasItem {
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
            crate::token_style::background(crate::tokens::Token::Surface),
            document,
        ))
        .id();
    crate::edit_mode::label(world, owner, "Document Viewer Castle", 24.0);
    let row = ui::row(world, owner);
    let input = ui::input(world, row, "PDF or EPUB file path", &path, 4096);
    world.get_mut::<Node>(input).unwrap().flex_grow = 1.0;
    ui::button(world, row, owner, "Open", ui::Control::Open);
    ui::button(world, row, owner, "Browse…", ui::Control::Browse);
    let controls = ui::row(world, owner);
    ui::button(world, controls, owner, "Previous", ui::Control::Previous);
    ui::button(world, controls, owner, "Next", ui::Control::Next);
    let mode = ui::button(world, controls, owner, "Scroll mode", ui::Control::Mode);
    let mode_label = world.get::<Children>(mode).unwrap()[0];
    ui::button(world, controls, owner, "−", ui::Control::Zoom(-0.25));
    ui::button(world, controls, owner, "+", ui::Control::Zoom(0.25));
    let navigation = ui::row(world, owner);
    crate::edit_mode::label(world, navigation, "PDF page / EPUB chapter", 13.0);
    let section_input = ui::input(world, navigation, "Page or chapter number", "1", 6);
    world.get_mut::<Node>(section_input).unwrap().width = px(65);
    ui::button(world, navigation, owner, "Go", ui::Control::Go);
    let status = crate::edit_mode::label(
        world,
        owner,
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
    crate::scroll_sand::attach(world, viewport);
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
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
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
