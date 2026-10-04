mod actions;
mod autosave;
mod closing;
pub(crate) mod credits;
mod editing;
mod file_changes;
mod highlight;
mod persistence;
mod project;
mod recovery;
mod runtime;
mod save_as;
mod search;
mod settings;
mod tabs;
mod tools;
pub use settings::Settings;
#[cfg(test)]
mod tests;

use bevy::{math::DVec2, prelude::*, text::EditableText};
pub(crate) use file_changes::{finish_change, reserve_change};
use lince_editor::{
    Buffer, TextWindow,
    files::{FileBinding, Scope, Snapshot},
};
pub(crate) use persistence::{SavedIde, snapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

const LINE_HEIGHT: f32 = 24.0;
const WINDOW_LINES: usize = 96;

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct Ide {
    pub explorer: crate::file_explorer::FileExplorer,
    pub paths: Vec<PathBuf>,
    pub active: Option<PathBuf>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub language_tools: BTreeMap<String, lince_editor::language::Tools>,
}

impl Ide {
    pub fn valid(&self) -> bool {
        self.explorer.valid()
            && self.settings.valid()
            && self.language_tools.len() <= 32
            && self
                .language_tools
                .iter()
                .all(|(language, tools)| language.len() <= 64 && tools.valid())
            && self.paths.len() <= 16
            && self
                .paths
                .iter()
                .all(|p| p.is_absolute() && p.as_os_str().len() <= 4096)
            && self
                .active
                .as_ref()
                .is_none_or(|path| self.paths.contains(path))
    }
}

struct Document {
    preview: Option<String>,
    buffer: Buffer,
    file: Option<Arc<FileBinding>>,
    disk: Snapshot,
    reading: bool,
    saving: Option<u64>,
    refresh: bool,
    moving: bool,
    error: Option<String>,
}

#[derive(Resource, Default)]
struct Documents(BTreeMap<PathBuf, Document>, BTreeSet<PathBuf>);

#[derive(Resource, Clone)]
struct EditorFont(Handle<Font>);

pub(crate) fn preview_font(world: &mut World, preview: &mut World) {
    world.init_resource::<EditorFont>();
    preview.insert_resource(world.resource::<EditorFont>().clone());
}

impl FromWorld for EditorFont {
    fn from_world(world: &mut World) -> Self {
        Self(
            world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
                include_bytes!("../../../institute/assets/fonts/DejaVuSans/DejaVuSansMono.ttf")
                    .to_vec(),
            )),
        )
    }
}

#[derive(Component)]
struct View {
    positions: BTreeMap<PathBuf, tabs::Position>,
    navigation: Entity,
    settings_panel: Entity,
    close_panel: Entity,
    replace_panel: Entity,
    replacement: Option<save_as::Pending>,
    closing: Option<PathBuf>,
    close_waiting: bool,
    settings_labels: [Entity; 4],
    shown_settings: Option<Settings>,
    search_cancel: Arc<std::sync::atomic::AtomicBool>,
    search_generation: u64,
    explorer: Entity,
    tabs: Entity,
    tab_labels: Vec<(PathBuf, bool)>,
    viewport: Entity,
    content: Entity,
    editor: Entity,
    gutter: Entity,
    conflicts: Entity,
    disk_preview: Entity,
    disk_text: Entity,
    review: bool,
    preview_key: Option<(usize, usize)>,
    status: Entity,
    find: Entity,
    replace: Entity,
    line: Entity,
    window: Option<TextWindow>,
    window_geometry: (usize, f32),
    path: Option<PathBuf>,
    selection: [usize; 2],
    anchors: [Option<loro::cursor::Cursor>; 2],
    had_input: bool,
    extending: bool,
    restore: bool,
    notice: String,
    discard: bool,
    paste: Option<bevy::clipboard::ClipboardRead>,
    draft: bool,
    follow_caret: bool,
}

impl Drop for View {
    fn drop(&mut self) {
        self.search_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct IdePlugin;

impl Plugin for IdePlugin {
    fn build(&self, app: &mut App) {
        highlight::install(app);
        app.init_resource::<Documents>()
            .init_resource::<autosave::Autosave>()
            .init_resource::<recovery::Recovery>()
            .add_systems(
                PostUpdate,
                editing::prepare.before(bevy::text::EditableTextSystems),
            )
            .add_systems(
                PostUpdate,
                editing::capture
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions)
                    .before(crate::file_explorer::RefreshFiles),
            )
            .add_systems(
                PostUpdate,
                runtime::update
                    .after(crate::file_explorer::RefreshFiles)
                    .before(bevy::ui::UiSystems::Layout),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    config: Ide,
) -> Entity {
    world.init_resource::<EditorFont>();
    let owner = crate::file_explorer::shell(
        world,
        root,
        workspace,
        position,
        "IDE Castle",
        Vec2::new(1120.0, 800.0),
    );
    let controls = crate::sand_panel::row(world, owner);
    for (caption, control) in [
        ("Save", actions::Control::Save),
        ("Open…", actions::Control::Open),
        ("Save as…", actions::Control::SaveAs),
        ("Undo", actions::Control::Undo(false)),
        ("Redo", actions::Control::Undo(true)),
        ("Refresh", actions::Control::Refresh),
        ("Close tab", actions::Control::Close),
        ("Discard…", actions::Control::Discard),
        ("Recover drafts", actions::Control::Recover),
        ("Find", actions::Control::SearchPanel),
        ("Settings", actions::Control::Settings),
        ("Language tools", actions::Control::Tools),
        ("Complete", actions::Control::Complete),
        ("Format", actions::Control::Format),
    ] {
        crate::sand_panel::button(world, controls, owner, caption, control);
    }
    crate::sand_panel::credits(world, controls, owner, credits::CREDITS);
    let (settings_panel, settings_labels) = settings::panel(world, owner);
    let close_panel = closing::panel(world, owner);
    let replace_panel = crate::sand_panel::row(world, owner);
    world.get_mut::<Node>(replace_panel).unwrap().display = Display::None;
    for (caption, control) in [
        ("Replace file", actions::Control::ReplaceFile),
        ("Cancel replacement", actions::Control::CancelReplacement),
    ] {
        crate::sand_panel::button(world, replace_panel, owner, caption, control);
    }
    let tabs = crate::sand_panel::row(world, owner);
    tools::panel(world, owner);
    let body = world
        .spawn((
            ChildOf(owner),
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                column_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let explorer = crate::file_explorer::embedded(world, body, owner, config.explorer.clone());
    settings::divider(world, body, owner);
    let pane = world
        .spawn((
            ChildOf(body),
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    let navigation = crate::sand_panel::row(world, pane);
    world.get_mut::<Node>(navigation).unwrap().flex_wrap = FlexWrap::Wrap;
    let find = crate::file_explorer::input(world, navigation, "Find text", "", 4096);
    world.get_mut::<Node>(find).unwrap().width = px(150);
    crate::sand_panel::button(
        world,
        navigation,
        owner,
        "Find next",
        actions::Control::Find,
    );
    crate::sand_panel::button(
        world,
        navigation,
        owner,
        "Previous",
        actions::Control::FindPrevious,
    );
    let replace = crate::file_explorer::input(world, navigation, "Replacement text", "", 4096);
    world.get_mut::<Node>(replace).unwrap().width = px(150);
    crate::sand_panel::button(
        world,
        navigation,
        owner,
        "Replace",
        actions::Control::Replace,
    );
    crate::sand_panel::button(
        world,
        navigation,
        owner,
        "Replace all",
        actions::Control::ReplaceAll,
    );
    let line = crate::file_explorer::input(world, navigation, "Line number", "1", 10);
    world.get_mut::<Node>(line).unwrap().width = px(65);
    crate::sand_panel::button(world, navigation, owner, "Go", actions::Control::Go);
    project::spawn(world, owner, pane, navigation);
    let viewport = world
        .spawn((
            ChildOf(pane),
            Node {
                width: percent(100),
                min_height: px(0),
                flex_grow: 1.0,
                overflow: Overflow::scroll(),
                ..default()
            },
        ))
        .id();
    crate::scroll_sand::attach(world, viewport);
    world.get_mut::<Node>(viewport).unwrap().overflow = Overflow::scroll();
    let content = world
        .spawn((
            ChildOf(viewport),
            Node {
                width: percent(100),
                height: px(LINE_HEIGHT),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let gutter = crate::edit_mode::label(world, content, "1", 15.0);
    world.entity_mut(gutter).insert((
        bevy::text::LineHeight::Px(LINE_HEIGHT),
        TextLayout::no_wrap(),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: px(58),
            ..default()
        },
    ));
    let editor = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(content),
        ))
        .id();
    world.entity_mut(editor).observe(editing::keyboard);
    let font = world.resource::<EditorFont>().0.clone();
    world.entity_mut(editor).insert((
        highlight::Syntax::default(),
        TextFont {
            font: font.into(),
            font_size: bevy::text::FontSize::Px(16.0),
            ..default()
        },
        bevy::text::LineHeight::Px(LINE_HEIGHT),
        TextLayout::no_wrap(),
        Node {
            position_type: PositionType::Absolute,
            left: px(64),
            top: px(0),
            min_width: px(100),
            padding: UiRect::ZERO,
            border: UiRect::ZERO,
            ..default()
        },
    ));
    let mut edit = world.get_mut::<EditableText>(editor).unwrap();
    edit.allow_newlines = true;
    edit.visible_lines = None;
    edit.max_characters = Some(lince_editor::MAX_WINDOW_BYTES * 2);
    edit.pending_edits.clear();
    let conflicts = crate::sand_panel::row(world, pane);
    crate::sand_panel::button(
        world,
        conflicts,
        owner,
        "Review disk",
        actions::Control::ReviewDisk,
    );
    crate::sand_panel::button(
        world,
        conflicts,
        owner,
        "Keep local version",
        actions::Control::Resolve(false),
    );
    crate::sand_panel::button(
        world,
        conflicts,
        owner,
        "Use disk version",
        actions::Control::Resolve(true),
    );
    let disk_preview = world
        .spawn((
            ChildOf(pane),
            Node {
                display: Display::None,
                width: percent(100),
                max_height: px(180),
                flex_shrink: 0.0,
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    crate::scroll_sand::attach(world, disk_preview);
    let disk_text = crate::edit_mode::label(world, disk_preview, "", 13.0);
    let status = crate::edit_mode::label(
        world,
        owner,
        "Open a UTF-8 file from the Explorer · Ctrl+S saves · Ctrl+Z undoes",
        12.0,
    );
    world.entity_mut(owner).insert((
        config,
        View {
            positions: BTreeMap::new(),
            navigation,
            settings_panel,
            close_panel,
            replace_panel,
            replacement: None,
            closing: None,
            close_waiting: false,
            settings_labels,
            shown_settings: None,
            search_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            search_generation: 0,
            explorer,
            tabs,
            tab_labels: Vec::new(),
            viewport,
            content,
            editor,
            gutter,
            conflicts,
            disk_preview,
            disk_text,
            review: false,
            preview_key: None,
            status,
            find,
            replace,
            line,
            window: None,
            window_geometry: (0, 0.0),
            path: None,
            selection: [0, 0],
            anchors: [None, None],
            had_input: false,
            extending: false,
            restore: true,
            notice: String::new(),
            discard: false,
            paste: None,
            draft: false,
            follow_caret: false,
        },
    ));
    owner
}

pub(crate) fn open(world: &mut World, owner: Entity, scope: Scope, path: PathBuf) {
    open_with_activation(world, owner, scope, path, true);
}

fn open_with_activation(
    world: &mut World,
    owner: Entity,
    scope: Scope,
    path: PathBuf,
    activate: bool,
) {
    if !recovery::ready(world) {
        status(
            world,
            owner,
            "Loading saved drafts; try opening the file shortly",
        );
        return;
    }
    if path.to_str().is_none() {
        status(
            world,
            owner,
            "This editor's workspace storage requires UTF-8 paths",
        );
        return;
    }
    if world
        .get::<Ide>(owner)
        .is_none_or(|ide| ide.paths.len() >= 16 && !ide.paths.contains(&path))
    {
        status(world, owner, "At most 16 tabs can be open");
        return;
    }
    if world.resource::<Documents>().1.contains(&path) {
        status(
            world,
            owner,
            "Wait for Save As to finish before opening this destination",
        );
        return;
    }
    if world.resource::<Documents>().0.contains_key(&path) {
        let mut ide = world.get_mut::<Ide>(owner).unwrap();
        if !ide.paths.contains(&path) {
            ide.paths.push(path.clone());
        }
        if activate || ide.active.is_none() {
            ide.active = Some(path);
        }
        status(world, owner, "");
        return;
    }
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            let file = scope.bind(&path)?;
            let (disk, preview) = match file.read() {
                Ok(disk) => (disk, None),
                Err(reason) => (file.preview()?, Some(reason)),
            };
            let buffer = Buffer::new(&disk.text)?;
            Ok::<_, String>(Document {
                preview,
                buffer,
                file: Some(Arc::new(file)),
                disk,
                reading: false,
                saving: None,
                refresh: false,
                moving: false,
                error: None,
            })
        },
        move |world, result| {
            if world.get::<Ide>(owner).is_none() {
                return;
            }
            match result {
                Ok(document) => {
                    let path = document.file.as_ref().unwrap().path.clone();
                    if !activate
                        && world
                            .get::<Ide>(owner)
                            .is_none_or(|ide| !ide.paths.contains(&path))
                    {
                        return;
                    }
                    if path.to_str().is_none() {
                        status(world, owner, "Workspace storage requires UTF-8 paths");
                        return;
                    }
                    if world
                        .get::<Ide>(owner)
                        .is_none_or(|ide| ide.paths.len() >= 16 && !ide.paths.contains(&path))
                    {
                        status(world, owner, "At most 16 tabs can be open");
                        return;
                    }
                    let mut documents = world.resource_mut::<Documents>();
                    if documents.1.contains(&path) {
                        status(
                            world,
                            owner,
                            "Wait for Save As to finish before opening this destination",
                        );
                        return;
                    }
                    if !documents.0.contains_key(&path)
                        && (documents.0.len() >= 32
                            || documents
                                .0
                                .values()
                                .map(|d| d.buffer.snapshot().len_bytes())
                                .sum::<usize>()
                                + document.disk.text.len()
                                > 64 * 1024 * 1024)
                    {
                        status(
                            world,
                            owner,
                            "Close some files first; the editor session limit is 64 MiB / 32 files",
                        );
                        return;
                    }
                    documents.0.entry(path.clone()).or_insert(document);
                    let mut ide = world.get_mut::<Ide>(owner).unwrap();
                    if !ide.paths.contains(&path) {
                        ide.paths.push(path.clone());
                    }
                    if activate || ide.active.is_none() {
                        ide.active = Some(path);
                    }
                    status(world, owner, "");
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    match result {
        Ok(()) => status(world, owner, "Opening…"),
        Err(e) => status(world, owner, e),
    }
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.notice = message.into();
    }
}

pub(crate) fn watched(world: &World) -> BTreeSet<PathBuf> {
    world
        .get_resource::<Documents>()
        .map(|documents| {
            documents
                .0
                .keys()
                .filter_map(|p| p.parent().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn disk_changed(world: &mut World, changes: lince_editor::watch::Changes) {
    let Some(mut documents) = world.get_resource_mut::<Documents>() else {
        return;
    };
    for (path, document) in &mut documents.0 {
        if changes.rescan
            || changes
                .paths
                .iter()
                .any(|changed| changed == path || path.starts_with(changed))
        {
            document.refresh = true;
        }
        if let Some(error) = &changes.error {
            document.error = Some(format!("File watching failed: {error}. Use Refresh."));
        }
    }
}

pub(crate) fn protect(world: &mut World, ancestor: Option<Entity>) -> bool {
    editing::capture(world);
    if ancestor.is_none() {
        if let Some(protected) = recovery::protect_exit(world) {
            return protected;
        }
    }
    let drafts: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.draft)
        .map(|(owner, _)| owner)
        .collect();
    if drafts.iter().any(|owner| {
        ancestor.is_none_or(|ancestor| {
            let mut current = Some(*owner);
            while let Some(entity) = current {
                if entity == ancestor {
                    return true;
                }
                current = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
    }) {
        return true;
    }
    if !world.contains_resource::<Documents>() {
        return false;
    }
    let documents = world.resource::<Documents>();
    let dirty: BTreeSet<_> = documents
        .0
        .iter()
        .filter(|(_, d)| {
            d.buffer.is_dirty()
                || d.buffer.conflict().is_some()
                || d.saving.is_some()
                || d.moving
                || d.file.is_none()
        })
        .map(|(p, _)| p.clone())
        .collect();
    if dirty.is_empty() {
        return false;
    }
    let owners: Vec<_> = world
        .query::<(Entity, &Ide)>()
        .iter(world)
        .filter(|(_, ide)| ide.paths.iter().any(|p| dirty.contains(p)))
        .map(|(owner, _)| owner)
        .collect();
    let affected: Vec<_> = owners
        .into_iter()
        .filter(|owner| {
            ancestor.is_none_or(|ancestor| {
                let mut current = Some(*owner);
                while let Some(entity) = current {
                    if entity == ancestor {
                        return true;
                    }
                    current = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                false
            })
        })
        .collect();
    for owner in &affected {
        status(
            world,
            *owner,
            "Unsaved files: save them, or use Discard… before closing",
        );
    }
    ancestor.is_none() || !affected.is_empty()
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "IDE Castle",
        "A plain text editor with a reusable file Explorer, Loro undo, and guarded disk saves.",
        actions::Control::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, Ide::default()),
    );
}
