use super::*;

pub(crate) struct Numbered {
    id: u64,
    entries: Vec<Workspace>,
    origin: Vec<u64>,
    next: Option<u64>,
}

impl Numbered {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn target<'a>(&'a self, world: &'a World, root: Entity) -> Option<&'a Workspace> {
        self.entries
            .iter()
            .find(|entry| entry.id == self.id)
            .or_else(|| {
                world
                    .get::<Workspaces>(root)?
                    .entries
                    .iter()
                    .find(|entry| entry.id == self.id)
            })
    }

    pub(crate) fn commit(self, world: &mut World, root: Entity) -> Result<u64, String> {
        let spaces = world
            .get::<Workspaces>(root)
            .ok_or("The workspace is unavailable.")?;
        if spaces
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>()
            != self.origin
            || spaces
                .canvas_state
                .as_ref()
                .map(nucleus::canvas::State::next_workspace_id)
                != self.next
            || self.entries.iter().any(|entry| {
                spaces
                    .trash
                    .iter()
                    .any(|removed| removed.workspace.id == entry.id)
                    || crate::native::workspace_config::next_id(world, root, entry.id)
                        != Some(entry.id)
            })
        {
            return Err("Workspace identities changed; try the command again.".into());
        }
        if self.entries.is_empty() {
            return Ok(self.id);
        }
        let last = self.entries.last().map(|entry| entry.id);
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        if let Some(id) = last
            && let Some(state) = &mut spaces.canvas_state
        {
            state.reserve_workspace_id(id)?;
        }
        spaces.entries.extend(self.entries);
        Ok(self.id)
    }
}

pub(crate) fn numbered(world: &World, root: Entity, number: u32) -> Result<Numbered, String> {
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?;
    let index = number
        .checked_sub(1)
        .ok_or("Workspace numbers start at one.")? as usize;
    let numbers = numbers(world, root);
    if let Some(id) = numbers.get(index) {
        return Ok(Numbered {
            id: *id,
            entries: Vec::new(),
            origin: spaces.entries.iter().map(|entry| entry.id).collect(),
            next: spaces
                .canvas_state
                .as_ref()
                .map(nucleus::canvas::State::next_workspace_id),
        });
    }
    if number > 9 {
        return Err("Only missing keyboard destinations 1–9 are created automatically.".into());
    }
    new(world, root, index + 1 - numbers.len())
}

pub(crate) fn new(world: &World, root: Entity, count: usize) -> Result<Numbered, String> {
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?;
    let numbers = numbers(world, root);
    if count == 0 || count > 9 || numbers.len() + count > 64 {
        return Err("A canvas supports up to 64 workspaces.".into());
    }
    let mut next = spaces
        .entries
        .iter()
        .filter(|entry| numbers.contains(&entry.id))
        .chain(spaces.trash.iter().map(|entry| &entry.workspace))
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Workspace identities are exhausted.")?;
    let reserved = spaces
        .canvas_state
        .as_ref()
        .map(nucleus::canvas::State::next_workspace_id);
    next = next.max(reserved.unwrap_or(next));
    let mode = world
        .get_resource::<crate::native::layout::desktop::DefaultLayout>()
        .map_or(crate::host::WorkspaceLayout::Free, |mode| mode.0);
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let id = loop {
            let id = crate::native::workspace_config::next_id(world, root, next)
                .ok_or("Workspace identities are exhausted.")?;
            if !spaces.entries.iter().any(|entry| entry.id == id) {
                break id;
            }
            next = id
                .checked_add(1)
                .ok_or("Workspace identities are exhausted.")?;
        };
        next = id
            .checked_add(1)
            .ok_or("Workspace identities are exhausted.")?;
        entries.push(Workspace {
            desktop: crate::layout::desktop::Workspace::new(mode),
            topology: Default::default(),
            id,
            name: format!("Workspace {id}"),
            center: [0.0; 2],
            zoom: 1.0,
            colors: Default::default(),
            color_overrides: [false; 2],
        });
    }
    Ok(Numbered {
        id: entries.last().unwrap().id,
        entries,
        origin: spaces.entries.iter().map(|entry| entry.id).collect(),
        next: reserved,
    })
}
