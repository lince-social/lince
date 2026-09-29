use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct SavedPosition {
    pub selection: [usize; 2],
    pub scroll: [f32; 2],
}

#[derive(Clone, Default)]
pub(super) struct Position {
    pub saved: SavedPosition,
    pub anchors: [Option<loro::cursor::Cursor>; 2],
    pub identity: u64,
}

pub(super) fn current(world: &World, owner: Entity) -> Option<(PathBuf, Position)> {
    let view = world.get::<View>(owner)?;
    let path = view.path.as_ref()?;
    let doc = world.get_resource::<Documents>()?.0.get(path)?;
    let scroll = world
        .get::<ScrollPosition>(view.viewport)
        .map_or(Vec2::ZERO, |scroll| scroll.0);
    Some((
        path.clone(),
        Position {
            saved: SavedPosition {
                selection: view.selection,
                scroll: [scroll.x, scroll.y],
            },
            anchors: std::array::from_fn(|i| {
                view.anchors[i]
                    .clone()
                    .or_else(|| doc.buffer.anchor(view.selection[i]))
            }),
            identity: doc.buffer.identity(),
        },
    ))
}

pub(super) fn switch(world: &mut World, owner: Entity, active: Option<PathBuf>) {
    closing::cancel(world, owner);
    let previous = current(world, owner);
    let paths = world.get::<Ide>(owner).unwrap().paths.clone();
    let mut view = world.get_mut::<View>(owner).unwrap();
    if let Some((path, position)) = previous {
        view.positions.insert(path, position);
    }
    view.positions.retain(|path, _| paths.contains(path));
    let position = active
        .as_ref()
        .and_then(|path| view.positions.get(path))
        .cloned()
        .unwrap_or_default();
    view.path = active.clone();
    view.selection = position.saved.selection;
    view.anchors = position.anchors;
    view.window = None;
    view.discard = false;
    view.review = false;
    view.preview_key = None;
    view.search_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    view.search_generation += 1;
    let viewport = view.viewport;
    let matches = active
        .as_ref()
        .and_then(|path| world.resource::<Documents>().0.get(path))
        .is_some_and(|doc| doc.buffer.identity() == position.identity);
    if !matches {
        world.get_mut::<View>(owner).unwrap().anchors = [None, None];
    }
    *world.get_mut::<ScrollPosition>(viewport).unwrap() =
        ScrollPosition(Vec2::from_array(position.saved.scroll));
}

pub(super) fn saved(world: &World, owner: Entity) -> BTreeMap<PathBuf, SavedPosition> {
    let Some(view) = world.get::<View>(owner) else {
        return BTreeMap::new();
    };
    let mut positions: BTreeMap<_, _> = view
        .positions
        .iter()
        .map(|(path, position)| (path.clone(), resolved(world, path, position)))
        .collect();
    if let Some((path, position)) = current(world, owner) {
        positions.insert(path.clone(), resolved(world, &path, &position));
    }
    if let Some(ide) = world.get::<Ide>(owner) {
        positions.retain(|path, _| ide.paths.contains(path));
    }
    positions
}

fn resolved(world: &World, path: &std::path::Path, position: &Position) -> SavedPosition {
    let mut saved = position.saved.clone();
    if let Some(doc) = world
        .get_resource::<Documents>()
        .and_then(|docs| docs.0.get(path))
        .filter(|doc| doc.buffer.identity() == position.identity)
    {
        saved.selection = std::array::from_fn(|i| {
            position.anchors[i]
                .as_ref()
                .and_then(|anchor| doc.buffer.position(anchor))
                .unwrap_or(saved.selection[i])
                .min(doc.buffer.snapshot().len_chars())
        });
    }
    saved
}

pub(super) fn relocate(world: &mut World, from: &std::path::Path, to: &std::path::Path) {
    for mut view in world.query::<&mut View>().iter_mut(world) {
        view.positions = std::mem::take(&mut view.positions)
            .into_iter()
            .map(|(path, position)| {
                (
                    lince_editor::operations::relocated(&path, from, to).unwrap_or(path),
                    position,
                )
            })
            .collect();
        if let Some(path) = &view.path {
            if let Some(next) = lince_editor::operations::relocated(path, from, to) {
                view.path = Some(next);
            }
        }
    }
}
