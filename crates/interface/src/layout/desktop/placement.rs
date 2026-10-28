use super::*;

impl Workspace {
    pub fn place_near(&mut self, id: PlacementId, slot: DropSlot) -> Result<bool, LayoutError> {
        if !self.valid() || self.mode != WorkspaceLayout::Tiled || self.fullscreen.is_some() {
            return Err(LayoutError::InvalidState);
        }
        for target in [id, slot.anchor] {
            let item = self
                .items
                .iter()
                .find(|item| item.id == target)
                .ok_or(LayoutError::MissingItem)?;
            if item.floating || self.minimized(target) || !self.rows.contains(target) {
                return Err(LayoutError::InvalidState);
            }
        }
        if id == slot.anchor {
            return Ok(false);
        }
        let mut next = self.clone();
        next.place_item(id, slot.anchor, slot.side)?;
        if !next.valid() {
            return Err(LayoutError::InvalidState);
        }
        let changed = next != *self;
        *self = next;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests;
