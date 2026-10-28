use super::*;

impl Workspace {
    pub fn windowed_fullscreen(&self, id: PlacementId) -> bool {
        self.windowed_fullscreen.contains(&id)
    }

    pub fn protocol_fullscreen(&self, id: PlacementId) -> bool {
        self.fullscreen == Some(id) || self.windowed_fullscreen(id)
    }

    pub fn set_windowed_fullscreen(&mut self, id: PlacementId, enabled: bool) -> Result<bool, LayoutError> {
        let item = self.items.iter().find(|item| item.id == id).ok_or(LayoutError::MissingItem)?;
        if item.kind != ItemKind::Application { return Err(LayoutError::InvalidItem); }
        if !enabled { return Ok(self.windowed_fullscreen.remove(&id)); }
        self.set_minimized(id, false);
        self.minimized_fullscreen.remove(&id);
        if self.fullscreen == Some(id) { self.fullscreen = None; }
        Ok(self.windowed_fullscreen.insert(id))
    }
}

#[cfg(test)]
mod tests;
