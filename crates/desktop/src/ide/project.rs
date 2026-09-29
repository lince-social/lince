use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Component)]
pub(super) struct Search {
    viewport: Entity,
    content: Entity,
    button: Entity,
    rows: Vec<lince_editor::project_search::Match>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    shown: Option<(usize, usize, u64)>,
    target: Option<lince_editor::project_search::Match>,
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub(super) fn spawn(world: &mut World, owner: Entity, pane: Entity, navigation: Entity) {
    let button = crate::sand_panel::button(
        world,
        navigation,
        owner,
        "Search files",
        actions::Control::ProjectSearch,
    );
    let viewport = world
        .spawn((
            ChildOf(pane),
            Node {
                display: Display::None,
                width: percent(100),
                height: px(180),
                flex_shrink: 0.0,
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
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.entity_mut(owner).insert(Search {
        viewport,
        content,
        button,
        rows: Vec::new(),
        generation: 0,
        cancel: Arc::new(AtomicBool::new(false)),
        shown: None,
        target: None,
    });
}

pub(super) fn run(world: &mut World, owner: Entity) {
    let ide = world.get::<Ide>(owner).unwrap().clone();
    if !ide.settings.project_search {
        status(world, owner, "Enable project search in Settings");
        return;
    }
    if ide.explorer.roots.is_empty() {
        status(world, owner, "Add a folder to search");
        return;
    }
    let input = world.get::<View>(owner).unwrap().find;
    let Ok(needle) = crate::sand_panel::value(world, input) else {
        return;
    };
    let query = needle.clone();
    let roots = ide.explorer.roots.clone();
    let options = ide.settings.search;
    let ignored = ide.explorer.ignored;
    let mut search = world.get_mut::<Search>(owner).unwrap();
    search.cancel.store(true, Ordering::Relaxed);
    search.cancel = Arc::new(AtomicBool::new(false));
    let cancel = search.cancel.clone();
    search.generation += 1;
    let generation = search.generation;
    search.rows.clear();
    search.shown = None;
    let viewport = search.viewport;
    world.entity_mut(viewport).insert(ScrollPosition::default());
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            lince_editor::project_search::search(
                ide.explorer.roots,
                &query,
                ide.settings.search,
                ide.explorer.ignored,
                &cancel,
            )
        },
        move |world, result| {
            if world.get::<Ide>(owner).is_none_or(|ide| {
                !ide.settings.project_search
                    || ide.settings.search != options
                    || ide.explorer.roots != roots
                    || ide.explorer.ignored != ignored
            }) || crate::sand_panel::value(world, input).ok().as_ref() != Some(&needle)
            {
                if world
                    .get::<Search>(owner)
                    .is_some_and(|search| search.generation == generation)
                {
                    status(world, owner, "Search changed; run it again");
                }
                return;
            }
            let Some(mut search) = world.get_mut::<Search>(owner).filter(|search| {
                search.generation == generation && !search.cancel.load(Ordering::Relaxed)
            }) else {
                return;
            };
            match result {
                Ok(matches) => {
                    let message = format!(
                        "{} matching lines on disk{}; unsaved edits use Find",
                        matches.rows.len(),
                        if matches.limited {
                            " · partial results, narrow the search"
                        } else {
                            ""
                        }
                    );
                    search.rows = matches.rows;
                    search.shown = None;
                    status(world, owner, message);
                }
                Err(e) => status(world, owner, e),
            }
        },
    );
    status(
        world,
        owner,
        result.map_or_else(|e| e, |()| "Searching files on disk…".into()),
    );
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(search) = world.get::<Search>(owner) else {
        return;
    };
    let enabled = world.get::<Ide>(owner).unwrap().settings.project_search;
    let (viewport, content, button) = (search.viewport, search.content, search.button);
    let display = if enabled && !search.rows.is_empty() {
        Display::Flex
    } else {
        Display::None
    };
    if world.get::<Node>(viewport).unwrap().display != display {
        world.get_mut::<Node>(viewport).unwrap().display = display;
    }
    let display = if enabled {
        Display::Flex
    } else {
        Display::None
    };
    if world.get::<Node>(button).unwrap().display != display {
        world.get_mut::<Node>(button).unwrap().display = display;
    }
    let search = world.get::<Search>(owner).unwrap();
    if !enabled {
        search.cancel.store(true, Ordering::Relaxed);
        return;
    }
    let first = (world
        .get::<ScrollPosition>(viewport)
        .map_or(0.0, |scroll| scroll.y)
        .max(0.0)
        / 30.0) as usize;
    let key = (first, search.rows.len(), search.generation);
    if search.shown != Some(key) {
        let rows: Vec<_> = search.rows.iter().skip(first).take(8).cloned().collect();
        let height = search.rows.len() as f32 * 30.0;
        crate::sand_panel::clear(world, content);
        world.get_mut::<Node>(content).unwrap().height = px(height);
        for (index, row) in rows.into_iter().enumerate() {
            let name = row
                .path
                .strip_prefix(&row.root)
                .unwrap_or(&row.path)
                .display();
            let caption = format!("{name}:{}  {}", row.line + 1, row.text);
            let button = crate::sand_panel::button(
                world,
                content,
                owner,
                &caption,
                actions::Control::OpenMatch(row),
            );
            let mut node = world.get_mut::<Node>(button).unwrap();
            node.position_type = PositionType::Absolute;
            node.top = px((first + index) as f32 * 30.0);
            node.height = px(30);
            node.width = percent(100);
            node.overflow = Overflow::clip();
        }
        world.get_mut::<Search>(owner).unwrap().shown = Some(key);
    }
    let target = world.get::<Search>(owner).unwrap().target.clone();
    if let Some(target) = target {
        if world.get::<View>(owner).unwrap().path.as_ref() == Some(&target.path) {
            if let Some(doc) = world.resource::<Documents>().0.get(&target.path) {
                let rope = doc.buffer.snapshot();
                let line = target.line.min(rope.len_lines() - 1);
                let position =
                    rope.line_to_char(line) + target.column.min(rope.line(line).len_chars());
                editing::select(world, owner, [position; 2], line);
                world.get_mut::<Search>(owner).unwrap().target = None;
                runtime::render(world, owner);
            }
        }
    }
}

pub(super) fn open_match(
    world: &mut World,
    owner: Entity,
    row: lince_editor::project_search::Match,
) {
    let target = row.clone();
    let result = crate::file_explorer::worker::run(
        world,
        move || Scope::open(&row.root).map(|scope| (scope, row.path)),
        move |world, result| match result {
            Ok((scope, path)) => {
                if let Some(mut search) = world.get_mut::<Search>(owner) {
                    search.target = Some(target);
                }
                open(world, owner, scope, path);
            }
            Err(e) => status(world, owner, e),
        },
    );
    if let Err(e) = result {
        status(world, owner, e);
    }
}
