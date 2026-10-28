use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scratch {
    home_workspace: u64,
    home: rows::ScratchHome,
    #[serde(skip)]
    shown: bool,
}

pub(super) mod entries {
    use super::*;

    pub fn serialize<S: serde::Serializer>(entries: &BTreeMap<PlacementId, Scratch>, serializer: S) -> Result<S::Ok, S::Error> {
        entries.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<PlacementId, Scratch>, D::Error> {
        let entries = Vec::<(PlacementId, Scratch)>::deserialize(deserializer)?;
        if entries.len() > MAX_ITEMS { return Err(serde::de::Error::custom("Too many scratch associations.")); }
        let mut result = BTreeMap::new();
        for (id, scratch) in entries {
            if !scratch.valid(id) || result.insert(id, scratch).is_some() { return Err(serde::de::Error::custom("Invalid or duplicate scratch association.")); }
        }
        Ok(result)
    }
}

impl Scratch {
    pub fn home_workspace(&self) -> u64 { self.home_workspace }

    pub fn shown(&self) -> bool { self.shown }

    pub fn home_geometry(&self) -> Geometry { self.home.geometry() }

    pub(super) fn valid(&self, id: PlacementId) -> bool {
        self.home_workspace != 0 && self.home.valid(id)
    }
}

impl Workspace {
    pub fn scratch(&self, id: PlacementId) -> Option<&Scratch> { self.scratch.get(&id) }

    pub fn scratch_items(&self) -> impl Iterator<Item = (PlacementId, &Scratch)> {
        self.scratch.iter().map(|(id, scratch)| (*id, scratch))
    }

    pub fn scratch_hidden(&self, id: PlacementId) -> bool {
        self.scratch.get(&id).is_some_and(|scratch| !scratch.shown)
    }

    pub fn retire_scratch(&mut self, id: PlacementId) -> bool {
        self.scratch.remove(&id).is_some()
    }

    pub fn hidden(&self, id: PlacementId) -> bool {
        self.minimized(id) || self.compact_hidden(id) || self.scratch_hidden(id)
    }

    pub(super) fn hidden_items(&self) -> BTreeSet<PlacementId> {
        self.items.iter().filter(|item| self.hidden(item.id)).map(|item| item.id).collect()
    }

    pub fn mark_scratch(&mut self, id: PlacementId, workspace: u64) -> Result<(), LayoutError> {
        if workspace == 0 || !self.valid() || self.compact_group(id).is_some() {
            return Err(LayoutError::InvalidState);
        }
        if self.scratch.contains_key(&id) { return Ok(()); }
        let item = self.items.iter().find(|item| item.id == id).ok_or(LayoutError::MissingItem)?.clone();
        if self.hidden(id) { return Err(LayoutError::InvalidState); }
        let home = self.rows.scratch_home(&item)?;
        self.scratch.insert(id, Scratch { home_workspace: workspace, home, shown: true });
        Ok(())
    }

    pub fn show_scratch(&mut self, id: PlacementId, geometry: Geometry) -> Result<(), LayoutError> {
        if geometry.rectangle().is_none() { return Err(LayoutError::InvalidItem); }
        let mut proposed = self.clone();
        if !proposed.valid() || !proposed.scratch.contains_key(&id) { return Err(LayoutError::InvalidState); }
        let item = proposed.items.iter().find(|item| item.id == id).ok_or(LayoutError::MissingItem)?;
        if !item.floating {
            proposed.toggle_floating(id, geometry.rectangle().ok_or(LayoutError::InvalidItem)?)?;
        }
        proposed.scratch.get_mut(&id).unwrap().shown = true;
        proposed.set_minimized(id, false);
        proposed.focused = Some(id);
        if !proposed.valid() { return Err(LayoutError::InvalidState); }
        *self = proposed;
        Ok(())
    }

    pub fn hide_scratch(&mut self, id: PlacementId) -> Result<(), LayoutError> {
        let scratch = self.scratch.get_mut(&id).ok_or(LayoutError::MissingItem)?;
        scratch.shown = false;
        if self.fullscreen == Some(id) { self.fullscreen = None; }
        if self.focused == Some(id) {
            self.focused = self.items.iter().rev().map(|item| item.id).find(|candidate| !self.hidden(*candidate));
        }
        if self.rows.last_focus == Some(id) {
            self.rows.last_focus = self.focused.filter(|id| self.rows.contains(*id));
        }
        Ok(())
    }

    pub fn unmark_scratch(&mut self, id: PlacementId, original_slot: bool) -> Result<(), LayoutError> {
        self.restore_scratch(id, original_slot, false)
    }

    pub fn normalize_scratch(&mut self, id: PlacementId, original_slot: bool) -> Result<(), LayoutError> {
        self.restore_scratch(id, original_slot, true)
    }

    fn restore_scratch(&mut self, id: PlacementId, original_slot: bool, retain: bool) -> Result<(), LayoutError> {
        let mut proposed = self.clone();
        let scratch = proposed.scratch.get(&id).ok_or(LayoutError::MissingItem)?.clone();
        let index = proposed.items.iter().position(|item| item.id == id).ok_or(LayoutError::MissingItem)?;
        proposed.remove_from_columns(id);
        scratch.home.restore(&mut proposed.rows, &mut proposed.items[index], original_slot);
        if !proposed.items[index].floating {
            let mut column = new_column(id, scratch.home.width());
            column.maximized = proposed.maximized.contains(&id);
            proposed.columns.push(column);
        }
        if retain {
            proposed.hide_scratch(id)?;
        } else {
            proposed.scratch.remove(&id);
        }
        if !proposed.valid() { return Err(LayoutError::InvalidState); }
        *self = proposed;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
