mod actions;
mod navigation;
mod operations;
mod persistence;
mod runtime;
#[cfg(test)]
mod tests;
pub(crate) mod worker;

pub use actions::BrowseFor;
use bevy::{math::DVec2, prelude::*};
use lince_editor::{
    explorer::{Entry, Listing},
    files::Scope,
};
pub(crate) use persistence::{SavedExplorer, snapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Component, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExplorer {
    pub roots: Vec<PathBuf>,
    pub grid: bool,
    pub ignored: bool,
}

impl FileExplorer {
    pub fn valid(&self) -> bool {
        self.roots.len() <= 16
            && self
                .roots
                .iter()
                .all(|path| path.is_absolute() && path.as_os_str().len() <= 4096)
    }
}

#[derive(Clone)]
enum Target {
    Standalone,
    Editor(Entity),
    Input {
        entity: Entity,
        original: String,
        extensions: Vec<String>,
        directories: bool,
    },
}

#[derive(Clone)]
struct Row {
    entry: Entry,
    root: PathBuf,
    depth: usize,
}

#[derive(Component)]
struct View {
    input: Entity,
    destination: Entity,
    selected_label: Entity,
    operations: Entity,
    selected: Option<Row>,
    directory: Option<PathBuf>,
    history: Vec<Option<PathBuf>>,
    location: Entity,
    confirmation: Option<Arc<lince_editor::operations::Entry>>,
    trash: Option<(Scope, lince_editor::operations::TrashTicket)>,
    busy: bool,
    search: Entity,
    status: Entity,
    viewport: Entity,
    content: Entity,
    target: Target,
    scopes: BTreeMap<PathBuf, Scope>,
    expanded: BTreeSet<PathBuf>,
    cache: BTreeMap<PathBuf, Listing>,
    pending: BTreeSet<PathBuf>,
    listings: BTreeMap<PathBuf, Arc<AtomicBool>>,
    rows: Vec<Row>,
    search_rows: Option<Vec<Row>>,
    cancellation: Arc<AtomicBool>,
    generation: u64,
    shown: Option<(usize, usize, usize, bool)>,
    dirty: bool,
    restore: bool,
    picking: bool,
}

impl Drop for View {
    fn drop(&mut self) {
        self.cancellation
            .store(true, std::sync::atomic::Ordering::Relaxed);
        for cancel in self.listings.values() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RefreshFiles;

pub struct FileExplorerPlugin;

impl Plugin for FileExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<worker::Worker>()
            .add_message::<bevy::window::WindowFocused>()
            .init_resource::<runtime::Watching>()
            .add_systems(Update, runtime::focus)
            .add_systems(
                PreUpdate,
                navigation::keyboard.after(bevy::input::InputSystems),
            )
            .add_systems(
                PostUpdate,
                runtime::update
                    .in_set(RefreshFiles)
                    .after(crate::actions::ApplyActions)
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::ui::UiSystems::Layout),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    config: FileExplorer,
) -> Entity {
    let owner = shell(
        world,
        root,
        workspace,
        position,
        "File Explorer Castle",
        Vec2::new(560.0, 700.0),
    );
    populate(world, owner, config, Target::Standalone);
    owner
}

pub(crate) fn shell(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    title: &str,
    size: Vec2,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            crate::workspace::WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::ide::credits::CREDITS),
            crate::canvas::CanvasItem { position, size },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(6),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    crate::edit_mode::label(world, owner, title, 20.0);
    owner
}

pub(crate) fn embedded(
    world: &mut World,
    parent: Entity,
    editor: Entity,
    config: FileExplorer,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            ChildOf(parent),
            crate::sand_store::SandCredits(crate::ide::credits::CREDITS),
            Node {
                width: px(270),
                min_width: px(180),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    populate(world, owner, config, Target::Editor(editor));
    owner
}

fn populate(world: &mut World, owner: Entity, config: FileExplorer, target: Target) {
    let input = input(world, owner, "Directory path", "", 4096);
    let row = crate::sand_panel::row(world, owner);
    button(world, row, owner, "Add root", actions::Control::Add);
    if !matches!(target, Target::Input { .. }) {
        button(world, row, owner, "New file", actions::Control::NewFile);
    }
    button(world, row, owner, "Choose…", actions::Control::Choose);
    button(world, row, owner, "Refresh", actions::Control::Refresh);
    button(world, row, owner, "Back", actions::Control::Back);
    button(world, row, owner, "Up", actions::Control::Up);
    button(
        world,
        row,
        owner,
        "Open folder",
        actions::Control::OpenFolder,
    );
    if matches!(
        target,
        Target::Input {
            directories: true,
            ..
        }
    ) {
        button(
            world,
            row,
            owner,
            "Select folder",
            actions::Control::SelectFolder,
        );
    }
    let location = crate::edit_mode::label(world, owner, "Selected roots", 12.0);
    world.entity_mut(location).insert(TextLayout::no_wrap());
    world.get_mut::<Node>(location).unwrap().overflow = Overflow::clip();
    if !matches!(target, Target::Input { .. }) {
        crate::sand_panel::button(world, row, owner, "Files…", operations::Control::Tools);
    }
    let operations = crate::sand_panel::column(world, owner);
    world.get_mut::<Node>(operations).unwrap().display = Display::None;
    let selected_label =
        crate::edit_mode::label(world, operations, "Select a file or folder", 12.0);
    world
        .entity_mut(selected_label)
        .insert(TextLayout::no_wrap());
    world.get_mut::<Node>(selected_label).unwrap().overflow = Overflow::clip();
    crate::edit_mode::label(world, operations, "New name or full destination path", 12.0);
    let destination = self::input(
        world,
        operations,
        "New name or full destination path",
        "",
        4096,
    );
    if !matches!(target, Target::Input { .. }) {
        let row = crate::sand_panel::row(world, operations);
        for (caption, action) in [
            ("New folder", operations::Control::NewFolder),
            ("Rename", operations::Control::Rename),
            ("Move", operations::Control::Move),
            ("Delete…", operations::Control::Delete),
            ("Undo delete", operations::Control::UndoDelete),
        ] {
            crate::sand_panel::button(world, row, owner, caption, action);
        }
    } else {
        world.get_mut::<Node>(destination).unwrap().display = Display::None;
        world.get_mut::<Node>(selected_label).unwrap().display = Display::None;
    }
    let search = self::input(world, owner, "Find a path in the selected roots", "", 256);
    let row = crate::sand_panel::row(world, owner);
    button(world, row, owner, "Search", actions::Control::Search);
    button(world, row, owner, "Tree", actions::Control::Tree);
    button(world, row, owner, "Grid", actions::Control::Grid);
    button(world, row, owner, "Ignored", actions::Control::Ignored);
    if matches!(target, Target::Editor(_)) {
        button(world, row, owner, "Extract", actions::Control::Extract);
    }
    if matches!(target, Target::Input { .. }) {
        button(world, row, owner, "Cancel", actions::Control::Cancel);
    }
    let status = crate::edit_mode::label(
        world,
        owner,
        if config.roots.is_empty() {
            "Choose one or more directories"
        } else {
            "Opening directories…"
        },
        12.0,
    );
    let viewport = world
        .spawn((
            ChildOf(owner),
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    crate::scroll_sand::attach(world, viewport);
    let content = world
        .spawn((
            ChildOf(viewport),
            Node {
                width: percent(100),
                height: px(0),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.entity_mut(owner).insert((
        config,
        View {
            input,
            destination,
            selected_label,
            operations,
            selected: None,
            directory: None,
            history: Vec::new(),
            location,
            confirmation: None,
            trash: None,
            busy: false,
            search,
            status,
            viewport,
            content,
            target,
            scopes: BTreeMap::new(),
            expanded: BTreeSet::new(),
            cache: BTreeMap::new(),
            pending: BTreeSet::new(),
            listings: BTreeMap::new(),
            rows: Vec::new(),
            search_rows: None,
            cancellation: Arc::new(AtomicBool::new(false)),
            generation: 0,
            shown: None,
            dirty: true,
            restore: true,
            picking: false,
        },
    ));
}

pub(crate) fn input(
    world: &mut World,
    parent: Entity,
    title: &str,
    value: &str,
    limit: usize,
) -> Entity {
    let bundle = crate::sand::text_editor(value, world.resource::<crate::theme::Typography>(), 0);
    let entity = world
        .spawn((bundle, ChildOf(parent), crate::icons::Tooltip(title.into())))
        .id();
    let mut edit = world.get_mut::<bevy::text::EditableText>(entity).unwrap();
    edit.allow_newlines = false;
    edit.visible_lines = Some(1.0);
    edit.max_characters = Some(limit);
    world.get_mut::<TextFont>(entity).unwrap().font_size = bevy::text::FontSize::Px(14.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    entity
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: actions::Control,
) {
    crate::sand_panel::button(world, parent, owner, caption, action);
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        crate::sand_panel::status(world, view.status, message);
    }
}

pub(crate) fn location(world: &World, mut entity: Entity) -> Option<(Entity, u64, DVec2)> {
    loop {
        if let Some(spaces) = world.get::<crate::workspace::Workspaces>(entity) {
            return Some((
                entity,
                spaces.active,
                world
                    .get::<crate::canvas::CanvasView>(entity)
                    .map_or(DVec2::ZERO, |view| view.center),
            ));
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "File Explorer Castle",
        "Browse directories, search paths, and open files in an IDE Castle.",
        actions::Control::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, FileExplorer::default()),
    );
}
