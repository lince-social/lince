use super::*;

pub const COMPACT_HEADER_HEIGHT: f64 = 36.0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactGroup {
    pub members: Vec<PlacementId>,
    pub active: PlacementId,
}

impl Rows {
    pub(in crate::layout::desktop) fn select_compact(&mut self, id: PlacementId) -> bool {
        let Some(index) = self.compact.iter().position(|group| group.members.contains(&id)) else { return false; };
        let previous = self.compact[index].active;
        if previous == id { return false; }
        let Some((row, slot)) = self.location(previous) else { return false; };
        self.rows[row].items[slot].id = id;
        self.compact[index].active = id;
        if self.last_focus == Some(previous) { self.last_focus = Some(id); }
        true
    }

    pub(super) fn home(&self, item: &Item) -> Result<Home, LayoutError> {
        let (row_index, index) = self.location(item.id).ok_or(LayoutError::MissingItem)?;
        let row = &self.rows[row_index];
        Ok(Home {
            item: row.items[index].clone(), row: row.id, row_index, index,
            before: index.checked_sub(1).map(|index| row.items[index].id),
            after: row.items.get(index + 1).map(|item| item.id), height: row.height,
            free: item.free, free_width: item.free_width, free_sizing: SizeIntent::capture(item.rules),
        })
    }

    pub(super) fn retire_compact_member(&mut self, id: PlacementId) -> Option<PlacementId> {
        let index = self.compact.iter().position(|group| group.members.contains(&id))?;
        let position = self.compact[index].members.iter().position(|member| *member == id).unwrap();
        let active = self.compact[index].active == id;
        if active {
            let next = self.compact[index].members.get(position + 1).copied()
                .or_else(|| position.checked_sub(1).map(|position| self.compact[index].members[position])).unwrap();
            self.select_compact(next);
        }
        self.compact[index].members.remove(position);
        self.homes.retain(|home| home.item.id != id);
        let survivor = self.compact[index].active;
        if self.compact[index].members.len() == 1 {
            self.compact.remove(index);
            self.homes.retain(|home| home.item.id != survivor);
        }
        active.then_some(survivor)
    }

    fn dissolve_compact(&mut self, id: PlacementId) -> Result<(), LayoutError> {
        let index = self.compact.iter().position(|group| group.members.contains(&id)).ok_or(LayoutError::MissingItem)?;
        let group = self.compact.remove(index);
        let (row, slot) = self.location(group.active).ok_or(LayoutError::InvalidState)?;
        self.rows[row].items.remove(slot);
        self.rows.retain(|row| !row.items.is_empty());
        let mut homes = Vec::with_capacity(group.members.len());
        for member in &group.members {
            let index = self.homes.iter().position(|home| home.item.id == *member).ok_or(LayoutError::InvalidState)?;
            homes.push(self.homes.remove(index));
        }
        homes.sort_by_key(|home| (home.row_index, home.index));
        for home in homes { self.insert_home(home); }
        self.last_focus = Some(group.active);
        Ok(())
    }
}

impl Workspace {
    pub fn compact_group(&self, id: PlacementId) -> Option<&CompactGroup> {
        self.rows.compact.iter().find(|group| group.members.contains(&id))
    }

    pub fn compact_groups(&self) -> &[CompactGroup] { &self.rows.compact }

    pub fn compact_hidden(&self, id: PlacementId) -> bool {
        self.mode == WorkspaceLayout::Tiled && self.compact_group(id).is_some_and(|group| group.active != id)
    }

    pub fn compact_viewport(&self, id: PlacementId, usable: Rectangle, solution: &Solution) -> Option<[f64; 2]> {
        if self.mode != WorkspaceLayout::Tiled || self.fullscreen.is_some()
            || !self.compact_group(id).is_some_and(|group| group.active == id)
        { return None; }
        let (row, index) = self.rows.location(id)?;
        let row = &self.rows.rows[row];
        let gap = self.gap.min(usable.size()[0].min(usable.size()[1]) / 4.0);
        if self.maximized(id) {
            return Some([(usable.size()[0] - gap * 2.0).max(1.0), (usable.size()[1] - gap * 2.0 - COMPACT_HEADER_HEIGHT).max(1.0)]);
        }
        let width = row.items[index].width.spaced((usable.size()[0] - gap * 2.0).max(1.0), gap);
        let height = row.items.iter().filter(|member| self.compact_group(member.id).is_none())
            .filter_map(|member| solution.items.iter().find(|item| item.id == member.id))
            .map(|item| item.rectangle.size()[1])
            .fold(row.height.spaced((usable.size()[1] - gap * 2.0).max(1.0), gap), f64::max).max(COMPACT_HEADER_HEIGHT + 1.0);
        Some([width, (height - COMPACT_HEADER_HEIGHT).max(1.0)])
    }

    pub fn create_compact_group(&mut self, anchor: PlacementId, members: &[PlacementId]) -> Result<(), LayoutError> {
        if self.mode != WorkspaceLayout::Tiled || self.fullscreen.is_some() || !self.valid() || members.is_empty() || members.len() >= MAX_ITEMS {
            return Err(LayoutError::InvalidState);
        }
        let mut ids = vec![anchor];
        ids.extend_from_slice(members);
        let mut unique = BTreeSet::new();
        if ids.iter().any(|id| !unique.insert(*id)) { return Err(LayoutError::DuplicateItem); }
        if ids.iter().any(|id| self.compact_group(*id).is_some() || self.minimized(*id) || self.scratch(*id).is_some()) { return Err(LayoutError::InvalidState); }
        let mut candidate = self.clone();
        let homes = ids.iter().map(|id| {
            let item = candidate.items.iter().find(|item| item.id == *id).ok_or(LayoutError::MissingItem)?;
            if item.floating { return Err(LayoutError::InvalidState); }
            candidate.rows.home(item)
        }).collect::<Result<Vec<_>, _>>()?;
        for row in &mut candidate.rows.rows { row.items.retain(|item| item.id == anchor || !unique.contains(&item.id)); }
        candidate.rows.rows.retain(|row| !row.items.is_empty());
        candidate.rows.homes.extend(homes);
        candidate.rows.compact.push(CompactGroup { members: ids, active: anchor });
        candidate.focused = Some(anchor);
        candidate.rows.last_focus = Some(anchor);
        if !candidate.valid() { return Err(LayoutError::InvalidState); }
        *self = candidate;
        Ok(())
    }

    pub fn add_compact_members(&mut self, anchor: PlacementId, members: &[PlacementId]) -> Result<(), LayoutError> {
        if self.mode != WorkspaceLayout::Tiled || self.fullscreen.is_some() || members.is_empty() || !self.valid() { return Err(LayoutError::InvalidState); }
        let group = self.compact_group(anchor).ok_or(LayoutError::MissingItem)?;
        let mut unique: BTreeSet<_> = group.members.iter().copied().collect();
        if members.iter().any(|id| !unique.insert(*id)) { return Err(LayoutError::DuplicateItem); }
        let mut candidate = self.clone();
        let mut homes = Vec::with_capacity(members.len());
        for id in members {
            let item = candidate.items.iter().find(|item| item.id == *id).ok_or(LayoutError::MissingItem)?;
            if item.floating || candidate.minimized(*id) || candidate.compact_group(*id).is_some() || candidate.scratch(*id).is_some() { return Err(LayoutError::InvalidState); }
            homes.push(candidate.rows.home(item)?);
        }
        let index = candidate.rows.compact.iter().position(|group| group.members.contains(&anchor)).unwrap();
        candidate.rows.compact[index].members.extend_from_slice(members);
        let removed: BTreeSet<_> = members.iter().copied().collect();
        for row in &mut candidate.rows.rows { row.items.retain(|item| !removed.contains(&item.id)); }
        candidate.rows.rows.retain(|row| !row.items.is_empty());
        candidate.rows.homes.extend(homes);
        if !candidate.valid() { return Err(LayoutError::InvalidState); }
        *self = candidate;
        Ok(())
    }

    pub fn select_compact_member(&mut self, id: PlacementId) -> Result<bool, LayoutError> {
        if self.mode != WorkspaceLayout::Tiled || !self.valid() || self.compact_group(id).is_none() { return Err(LayoutError::InvalidState); }
        self.set_minimized(id, false);
        let changed = self.rows.select_compact(id);
        self.focused = Some(id);
        self.rows.last_focus = Some(id);
        if self.fullscreen.is_some() { self.fullscreen = Some(id); }
        Ok(changed)
    }

    pub fn reorder_compact_member(&mut self, id: PlacementId, direction: Direction) -> Result<bool, LayoutError> {
        if !matches!(direction, Direction::Left | Direction::Right) || !self.valid() { return Err(LayoutError::InvalidState); }
        let group = self.rows.compact.iter_mut().find(|group| group.members.contains(&id)).ok_or(LayoutError::MissingItem)?;
        let index = group.members.iter().position(|member| *member == id).unwrap();
        let target = if direction == Direction::Left { index.checked_sub(1) } else { (index + 1 < group.members.len()).then_some(index + 1) };
        if let Some(target) = target { group.members.swap(index, target); Ok(true) } else { Ok(false) }
    }

    pub fn dissolve_compact_group(&mut self, id: PlacementId) -> Result<(), LayoutError> {
        if !self.valid() { return Err(LayoutError::InvalidState); }
        let mut candidate = self.clone();
        candidate.rows.dissolve_compact(id)?;
        if !candidate.valid() { return Err(LayoutError::InvalidState); }
        *self = candidate;
        Ok(())
    }

    pub fn detach_compact_member(&mut self, id: PlacementId) -> Result<(), LayoutError> {
        if !self.valid() || self.compact_group(id).is_none() { return Err(LayoutError::InvalidState); }
        let mut candidate = self.clone();
        let home = candidate.rows.homes.iter().find(|home| home.item.id == id).cloned().ok_or(LayoutError::InvalidState)?;
        candidate.rows.retire_compact_member(id);
        candidate.rows.insert_home(home);
        if !candidate.valid() { return Err(LayoutError::InvalidState); }
        *self = candidate;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
