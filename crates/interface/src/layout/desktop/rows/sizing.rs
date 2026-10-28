use super::*;

impl Workspace {
    pub fn row_height(&self, id: PlacementId) -> Option<Width> {
        let (row, _) = self.rows.location(id)?;
        Some(self.rows.rows[row].height)
    }

    pub fn set_row_height(&mut self, id: PlacementId, height: Width) -> Result<(), LayoutError> {
        if !height.valid() {
            return Err(LayoutError::InvalidItem);
        }
        let (row, _) = self.rows.location(id).ok_or(LayoutError::MissingItem)?;
        self.rows.rows[row].height = height;
        let row = self.rows.rows[row].id;
        for home in &mut self.rows.homes {
            if home.row == row {
                home.height = height;
            }
        }
        Ok(())
    }

    pub fn cycle_row_height(&mut self, presets: &[[u16; 2]]) -> Result<(), LayoutError> {
        if !super::super::widths::valid_presets(presets)
            || self.mode != WorkspaceLayout::Tiled
            || self.fullscreen.is_some()
        {
            return Err(LayoutError::InvalidState);
        }
        let id = self.focused.ok_or(LayoutError::MissingItem)?;
        let height = self.row_height(id).ok_or(LayoutError::MissingItem)?;
        let current = if self.maximized(id) {
            Some([1, 1])
        } else {
            height.ratio()
        };
        let [numerator, denominator] = super::super::widths::next_preset(presets, current);
        self.set_row_height(id, Width::Fraction(numerator, denominator))?;
        self.request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        );
        Ok(())
    }

    pub fn resize_focused(
        &mut self,
        axis: usize,
        delta: f64,
        usable: Rectangle,
        scale: f64,
    ) -> Result<(), LayoutError> {
        if delta.abs() > 10_000.0 {
            return Err(LayoutError::InvalidState);
        }
        let id = self.focused.ok_or(LayoutError::MissingItem)?;
        self.resize_item(id, axis, delta, usable, scale)
    }

    pub(in crate::layout::desktop) fn resize_item(
        &mut self,
        id: PlacementId,
        axis: usize,
        delta: f64,
        usable: Rectangle,
        scale: f64,
    ) -> Result<(), LayoutError> {
        if axis > 1
            || !delta.is_finite()
            || delta == 0.0
            || delta.abs() > 1_000_000_000.0
            || !scale.is_finite()
            || !(0.000_1..=10_000.0).contains(&scale)
            || self.fullscreen.is_some()
            || !self.valid()
        {
            return Err(LayoutError::InvalidState);
        }
        let item = self
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or(LayoutError::MissingItem)?;
        let tiled = self.mode == WorkspaceLayout::Tiled && !item.floating;
        if !tiled && self.mode != WorkspaceLayout::Free && !item.floating {
            return Err(LayoutError::InvalidState);
        }
        let gap = self.gap.min(usable.size()[0].min(usable.size()[1]) / 4.0);
        let available = (usable.size()[axis] - 2.0 * gap).max(1.0);
        if tiled && axis == 0 {
            if self.compact_group(id).is_some() {
                let current = if self.maximized(id) { available } else { self.item_width(id).ok_or(LayoutError::MissingItem)?.spaced(available, gap) };
                self.set_item_width(id, Width::Pixels((current + delta).clamp(1.0, f64::from(LIMIT))))?;
                self.request_state(id, crate::host::WindowRequest { maximized: Some(false), ..Default::default() });
                return Ok(());
            }
            let influence = self.influences.get(&id).copied().unwrap_or_default().scale;
            let rules = item.rules.axes[0];
            let current = if self.maximized(id) {
                available
            } else {
                (self
                    .item_width(id)
                    .ok_or(LayoutError::MissingItem)?
                    .spaced(available, gap)
                    * influence)
                    .clamp(f64::from(rules.min), f64::from(rules.max))
            };
            let pixels =
                (current + delta).clamp(f64::from(rules.min), f64::from(rules.max)) / influence;
            self.set_item_width(id, Width::Pixels(pixels.clamp(1.0, f64::from(LIMIT))))?;
        } else if tiled {
            let (row, _) = self.rows.location(id).ok_or(LayoutError::MissingItem)?;
            let current = if self.maximized(id) {
                available
            } else {
                let members: BTreeSet<_> = self.rows.rows[row]
                    .items
                    .iter()
                    .map(|item| item.id)
                    .collect();
                self.items
                    .iter()
                    .filter(|item| members.contains(&item.id) && !self.minimized.contains(&item.id) && self.compact_group(item.id).is_none())
                    .map(|item| f64::from(item.rules.axes[1].min))
                    .fold(self.rows.rows[row].height.spaced(available, gap), f64::max)
                    .max(if members.iter().any(|id| self.compact_group(*id).is_some()) { COMPACT_HEADER_HEIGHT + 1.0 } else { 1.0 })
            };
            self.set_row_height(
                id,
                Width::Pixels((current + delta).clamp(1.0, f64::from(LIMIT))),
            )?;
        } else {
            self.resolve_free_widths(usable);
            let item = self
                .items
                .iter()
                .find(|item| item.id == id)
                .ok_or(LayoutError::MissingItem)?;
            let mut rules = item.rules;
            let mut geometry = item.free;
            let size = (geometry.size[axis] + delta / scale).clamp(
                f64::from(rules.axes[axis].min),
                f64::from(rules.axes[axis].max),
            );
            geometry.origin[axis] = (geometry.origin[axis] + (geometry.size[axis] - size) * 0.5)
                .clamp(-1_000_000_000.0, 1_000_000_000.0);
            geometry.size[axis] = size;
            rules.axes[axis].size = size as f32;
            rules.axes[axis].sizing = crate::layout::Sizing::Fixed;
            self.configure(id, rules)?;
            self.set_free_geometry(id, geometry)?;
        }
        self.request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
