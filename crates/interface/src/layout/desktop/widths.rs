use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreeWidth {
    pub ratio: [u16; 2],
    pub scale: f64,
}

impl FreeWidth {
    pub(super) fn valid(self) -> bool {
        Width::Fraction(self.ratio[0], self.ratio[1]).valid()
            && self.scale.is_finite()
            && (0.000_1..=10_000.0).contains(&self.scale)
    }
}

impl Width {
    pub fn ratio(self) -> Option<[u16; 2]> {
        Some(match self {
            Self::Fraction(numerator, denominator) => [numerator, denominator],
            Self::Third => [1, 3],
            Self::Half => [1, 2],
            Self::TwoThirds => [2, 3],
            Self::Full => [1, 1],
            Self::Pixels(_) => return None,
        })
    }

    pub(super) fn spaced(self, available: f64, gap: f64) -> f64 {
        match self {
            Self::Pixels(_) => self.resolve(available),
            _ => (self.resolve(available + gap) - gap).clamp(1.0, f64::from(LIMIT)),
        }
    }
}

impl Workspace {
    pub fn set_free_width(
        &mut self,
        id: PlacementId,
        ratio: [u16; 2],
        scale: f64,
    ) -> Result<(), LayoutError> {
        let intent = FreeWidth { ratio, scale };
        if !intent.valid() {
            return Err(LayoutError::InvalidState);
        }
        self.set_free_width_intent(id, Some(intent))
    }

    pub fn clear_free_width(&mut self, id: PlacementId) -> Result<(), LayoutError> {
        self.set_free_width_intent(id, None)
    }

    fn set_free_width_intent(&mut self, id: PlacementId, intent: Option<FreeWidth>) -> Result<(), LayoutError> {
        let item = self.item_mut(id)?;
        item.free_width = intent;
        let geometry = item.free;
        let sizing = rows::SizeIntent::capture(item.rules);
        if self.mode == WorkspaceLayout::Free { self.rows.authored_free(id, geometry, intent, sizing); }
        Ok(())
    }

    pub fn cycle_item_width(
        &mut self,
        presets: &[[u16; 2]],
        usable: Rectangle,
        scale: f64,
    ) -> Result<(), LayoutError> {
        if !valid_presets(presets)
            || !(FreeWidth {
                ratio: [1, 1],
                scale,
            })
            .valid()
        {
            return Err(LayoutError::InvalidState);
        }
        let id = self.focused.ok_or(LayoutError::MissingItem)?;
        if self.fullscreen.is_some() {
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
        let current = if self.maximized(id) {
            Some([1, 1])
        } else if tiled {
            self.item_width(id).and_then(Width::ratio)
        } else {
            item.free_width.map(|width| width.ratio)
        };
        let ratio = next_preset(presets, current);
        if tiled {
            self.set_item_width(id, Width::Fraction(ratio[0], ratio[1]))?;
        } else {
            self.set_free_width(id, ratio, scale)?;
        }
        self.request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        );
        self.resolve_free_widths(usable);
        Ok(())
    }

    pub(super) fn resolve_free_widths(&mut self, usable: Rectangle) {
        let gap = self.gap.min(usable.size()[0].min(usable.size()[1]) / 4.0);
        let available = (usable.size()[0] - gap * 2.0).max(1.0);
        for item in &mut self.items {
            if self.mode != WorkspaceLayout::Free && !item.floating
                || self.maximized.contains(&item.id)
                || self.fullscreen == Some(item.id)
            {
                continue;
            }
            let Some(intent) = item.free_width else {
                continue;
            };
            let width = Width::Fraction(intent.ratio[0], intent.ratio[1]).spaced(available, gap)
                / intent.scale;
            let width = width.clamp(
                f64::from(item.rules.axes[0].min),
                f64::from(item.rules.axes[0].max),
            );
            item.free.origin[0] = (item.free.origin[0] + (item.free.size[0] - width) * 0.5)
                .clamp(-1_000_000_000.0, 1_000_000_000.0);
            item.free.size[0] = width;
        }
    }
}

fn equivalent(first: [u16; 2], second: [u16; 2]) -> bool {
    u32::from(first[0]) * u32::from(second[1]) == u32::from(second[0]) * u32::from(first[1])
}

pub(super) fn valid_presets(presets: &[[u16; 2]]) -> bool {
    !presets.is_empty()
        && presets.len() <= 32
        && presets
            .iter()
            .all(|ratio| Width::Fraction(ratio[0], ratio[1]).valid())
        && !presets.iter().enumerate().any(|(index, ratio)| {
            presets[index + 1..]
                .iter()
                .any(|other| equivalent(*ratio, *other))
        })
}

pub(super) fn next_preset(presets: &[[u16; 2]], current: Option<[u16; 2]>) -> [u16; 2] {
    let index = current
        .and_then(|ratio| presets.iter().position(|preset| equivalent(*preset, ratio)))
        .map_or(0, |index| (index + 1) % presets.len());
    presets[index]
}

#[cfg(test)]
mod tests;
