use super::*;

impl Workspace {
    pub fn scroll_to(
        &mut self,
        requested: [f64; 2],
        usable: Rectangle,
    ) -> Result<bool, LayoutError> {
        if self.mode != WorkspaceLayout::Tiled
            || self.fullscreen.is_some()
            || requested.iter().any(|value| !value.is_finite())
        {
            return Err(LayoutError::InvalidState);
        }
        let before = self.scroll;
        let extent = self.solve(usable, usable)?.extent;
        self.scroll = std::array::from_fn(|axis| {
            requested[axis].clamp(0.0, (extent[axis] - usable.size()[axis]).max(0.0))
        });
        self.rows.scroll = self.scroll;
        Ok(self.scroll != before)
    }
}

#[cfg(test)]
mod tests;
