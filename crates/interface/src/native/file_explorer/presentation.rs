// non-lean-yet
// Scope: struct Rows, struct Key, struct Button, struct CaptionInputs.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 81, 89, 93, 159,
//   171, 193, 299, 301, 385, 386.
// - [review: §3 Ownership] Check whether the borrowed container can expose a narrower view. Sites
//   flatten:path → &Path (375), flatten:root → &Path (375).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (43, 44, 47, 57, 59, 60, 103, 116, 117, 183, 184, 197, 306, 309, 325, 333, 347,
//   355, 363, 367).

use super::*;
use crate::host::performance::{self, Count, Stage};

#[derive(Default)]
pub(super) struct Rows(crate::native::retained::Children<Key>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    generation: u64,
    index: usize,
    row: Row,
    grid: bool,
    remove: bool,
}

struct Button {
    key: Key,
    expanded: bool,
    selected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CaptionInputs {
    search: bool,
    expanded: bool,
}

#[derive(Component)]
struct ButtonState {
    label: Entity,
    caption: CaptionInputs,
    columns: usize,
    selected: bool,
}

struct Layout {
    columns: usize,
    height: f32,
    search: bool,
}

pub(super) fn render(world: &mut World, owner: Entity) {
    if !world.get::<View>(owner).unwrap().search_results {
        let sorting = std::mem::take(&mut world.get_mut::<View>(owner).unwrap().pending_sorts);
        if !sorting.is_empty() {
            let _sort = performance::Span::new(Stage::NativeExplorerListingSort);
            let mut view = world.get_mut::<View>(owner).unwrap();
            for path in sorting {
                if let Some(listing) = view.cache.get_mut(&path) {
                    performance::count(Count::ExplorerListingSorts);
                    performance::add(Count::ExplorerEntriesSorted, listing.entries.len() as u64);
                    lince_editor::explorer::sort(&mut listing.entries);
                }
            }
        }
    }
    if world.get::<View>(owner).unwrap().dirty {
        let _model = performance::Span::new(Stage::NativeExplorerRows);
        let config = world.get::<FileExplorer>(owner).unwrap();
        let view = world.get::<View>(owner).unwrap();
        let rows = if view.search_results {
            performance::count(Count::ExplorerSearchModelsRetained);
            None
        } else {
            let mut rows = Vec::new();
            let roots = view
                .directory
                .as_ref()
                .map_or(config.roots.as_slice(), std::slice::from_ref);
            for directory in roots {
                let root = view
                    .scopes
                    .keys()
                    .filter(|root| directory.starts_with(root))
                    .max_by_key(|root| root.components().count())
                    .unwrap_or(directory);
                if config.grid && view.directory.is_some() {
                    if let Some(listing) = view.cache.get(directory) {
                        rows.extend(listing.entries.iter().cloned().map(|entry| Row {
                            entry,
                            root: root.clone(),
                            depth: 1,
                        }));
                    }
                    continue;
                }
                rows.push(Row {
                    entry: Entry {
                        path: directory.clone(),
                        directory: true,
                        link: false,
                    },
                    root: root.clone(),
                    depth: 0,
                });
                if !config.grid {
                    flatten(view, root, directory, 1, &mut rows);
                }
            }
            performance::add(Count::ExplorerRowsDerived, rows.len() as u64);
            Some(rows)
        };
        let mut view = world.get_mut::<View>(owner).unwrap();
        if let Some(rows) = rows {
            view.rows = rows;
        }
        view.shown = None;
        view.dirty = false;
        let location = view.location;
        let caption = view.directory.as_ref().map_or_else(
            || "Selected roots".to_owned(),
            |path| path.display().to_string(),
        );
        crate::native::sand_panel::status(world, location, caption);
    }
    let view = world.get::<View>(owner).unwrap();
    let grid = world.get::<FileExplorer>(owner).unwrap().grid;
    let columns = if grid {
        world.get::<ComputedNode>(view.viewport).map_or(2, |node| {
            ((node.size().x * node.inverse_scale_factor()) / 150.0)
                .floor()
                .max(1.0) as usize
        })
    } else {
        1
    };
    let height = if grid { 76.0 } else { 30.0 };
    let offset = world
        .get::<ScrollPosition>(view.viewport)
        .map_or(0.0, |scroll| scroll.y);
    let first = (offset.max(0.0) / height).floor() as usize * columns;
    let count = world.get::<ComputedNode>(view.viewport).map_or(30, |node| {
        ((node.size().y * node.inverse_scale_factor()) / height).ceil() as usize + 2
    }) * columns;
    let first = first.min(view.rows.len());
    let shown = (first, columns, count, grid);
    if view.shown == Some(shown) {
        return;
    }
    let _presentation = performance::Span::new(Stage::NativeExplorerPresentation);
    let content = view.content;
    let total_height = px(view.rows.len().div_ceil(columns) as f32 * height);
    let layout = Layout {
        columns,
        height,
        search: view.search_results,
    };
    let mut buttons = Vec::new();
    for (index, row) in view
        .rows
        .iter()
        .enumerate()
        .skip(first)
        .take(count.min(160))
    {
        let key = Key {
            generation: view.generation,
            index,
            row: row.clone(),
            grid,
            remove: false,
        };
        let expanded = !grid && row.entry.directory && view.expanded.contains(&row.entry.path);
        let selected = view
            .selected
            .as_ref()
            .is_some_and(|selected| selected.entry.path == row.entry.path);
        let remove = (!grid && row.depth == 0 && row.entry.path == row.root).then(|| Button {
            key: Key {
                remove: true,
                ..key.clone()
            },
            expanded: false,
            selected: false,
        });
        buttons.push(Button {
            key,
            expanded,
            selected,
        });
        buttons.extend(remove);
    }
    let mut rendered = std::mem::take(&mut world.get_mut::<View>(owner).unwrap().rendered);
    let mut node = world.get_mut::<Node>(content).unwrap();
    if node.height != total_height {
        node.height = total_height;
    }
    rendered.0.reconcile(
        world,
        content,
        buttons
            .into_iter()
            .map(|button| (button.key.clone(), button)),
        |world, parent, button| create(world, parent, owner, button, &layout),
        |world, entity, button| update(world, entity, button, &layout),
    );
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.rendered = rendered;
    view.shown = Some(shown);
}

fn caption(button: &Button, layout: &Layout) -> String {
    if button.key.remove {
        return "×".into();
    }
    let row = &button.key.row;
    let name = if layout.search {
        row.entry
            .path
            .strip_prefix(&row.root)
            .unwrap_or(&row.entry.path)
            .to_string_lossy()
    } else if row.depth == 0 {
        row.entry.path.to_string_lossy()
    } else {
        row.entry
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    };
    let link = if row.entry.link { " ↗" } else { "" };
    if button.key.grid {
        format!(
            "{}\n{name}{link}",
            if row.entry.directory { "▣" } else { "▤" }
        )
    } else {
        let prefix = if row.entry.directory {
            if button.expanded { "▾ " } else { "▸ " }
        } else {
            ""
        };
        format!("{prefix}{name}{link}")
    }
}

fn node(button: &Button, layout: &Layout) -> Node {
    let key = &button.key;
    if key.remove {
        return Node {
            position_type: PositionType::Absolute,
            right: px(0),
            top: px(key.index as f32 * layout.height),
            width: px(24),
            height: px(layout.height),
            ..default()
        };
    }
    Node {
        position_type: PositionType::Absolute,
        left: if key.grid {
            percent((key.index % layout.columns) as f32 * 100.0 / layout.columns as f32)
        } else {
            px(0)
        },
        top: px((key.index / layout.columns) as f32 * layout.height),
        width: percent(100.0 / layout.columns as f32),
        height: px(layout.height),
        padding: UiRect {
            left: px(if key.grid {
                6.0
            } else {
                key.row.depth as f32 * 14.0 + 4.0
            }),
            right: px(4),
            top: px(4),
            bottom: px(4),
        },
        overflow: Overflow::clip(),
        ..default()
    }
}

fn background(button: &Button) -> BackgroundColor {
    BackgroundColor(if button.selected {
        Color::srgba(0.25, 0.45, 0.7, 0.35)
    } else {
        Color::NONE
    })
}

fn inputs(button: &Button, layout: &Layout) -> CaptionInputs {
    CaptionInputs {
        search: !button.key.remove && layout.search,
        expanded: button.expanded,
    }
}

fn create(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    button: &Button,
    layout: &Layout,
) -> Entity {
    let caption = caption(button, layout);
    let action = if button.key.remove {
        actions::Control::RemoveRoot(button.key.row.root.clone())
    } else {
        actions::Control::Entry(button.key.row.clone())
    };
    let entity = crate::native::sand_panel::button(world, parent, owner, &caption, action);
    let label = world
        .get::<Children>(entity)
        .unwrap()
        .iter()
        .find(|child| world.get::<Text>(*child).is_some())
        .unwrap();
    world.entity_mut(entity).insert((
        node(button, layout),
        background(button),
        ButtonState {
            label,
            caption: inputs(button, layout),
            columns: layout.columns,
            selected: button.selected,
        },
    ));
    performance::count(Count::ExplorerButtonsCreated);
    entity
}

fn update(world: &mut World, entity: Entity, button: &Button, layout: &Layout) {
    let current = world.get::<ButtonState>(entity).unwrap();
    let wanted = inputs(button, layout);
    let placement_changed = current.columns != layout.columns;
    let selection_changed = current.selected != button.selected;
    let caption_changed = current.caption != wanted;
    let label = current.label;
    if placement_changed {
        let desired = node(button, layout);
        let mut node = world.get_mut::<Node>(entity).unwrap();
        if node.left != desired.left {
            node.left = desired.left;
        }
        if node.top != desired.top {
            node.top = desired.top;
        }
        if node.width != desired.width {
            node.width = desired.width;
        }
    }
    if selection_changed {
        world
            .get_mut::<BackgroundColor>(entity)
            .unwrap()
            .set_if_neq(background(button));
    }
    if caption_changed {
        let caption = caption(button, layout);
        crate::native::sand_panel::status(world, label, &caption);
        world
            .get_mut::<crate::native::icons::Tooltip>(entity)
            .unwrap()
            .0
            .clone_from(&caption);
        if let Some(mut accessible) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
            accessible.set_label(caption.as_str());
        }
        world
            .get_mut::<crate::native::canvas_host::authoring::PartName>(entity)
            .unwrap()
            .0 = format!("button-{}", caption.to_lowercase().replace(' ', "-"));
    }
    if placement_changed || selection_changed || caption_changed {
        let mut current = world.get_mut::<ButtonState>(entity).unwrap();
        current.caption = wanted;
        current.columns = layout.columns;
        current.selected = button.selected;
    }
    performance::count(Count::ExplorerButtonsRetained);
}

fn flatten(view: &View, root: &PathBuf, path: &PathBuf, depth: usize, rows: &mut Vec<Row>) {
    if depth > 64 || rows.len() >= 100_000 || !view.expanded.contains(path) {
        return;
    }
    if let Some(listing) = view.cache.get(path) {
        for entry in &listing.entries {
            if rows.len() >= 100_000 {
                break;
            }
            rows.push(Row {
                entry: entry.clone(),
                root: root.clone(),
                depth,
            });
            if entry.directory && &entry.path != path {
                flatten(view, root, &entry.path, depth + 1, rows);
            }
        }
    }
}
