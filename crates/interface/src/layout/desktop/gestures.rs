use super::*;
use crate::host::Manipulation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropSlot {
    pub anchor: PlacementId,
    pub side: Direction,
}

pub struct Gesture {
    source: Item,
    operation: Manipulation,
    mode: WorkspaceLayout,
    usable: Rectangle,
    maximized: bool,
    geometry: Geometry,
    edges: [i8; 2],
    tiled: bool,
    row: Option<(u64, Width, Width)>,
    boundary: Option<(PlacementId, (u64, Width, Width))>,
    gap: f64,
    influence: Option<Influence>,
}

impl Gesture {
    pub fn begin(
        model: &Workspace,
        id: PlacementId,
        operation: Manipulation,
        uv: [f64; 2],
        usable: Rectangle,
    ) -> Result<Self, LayoutError> {
        Self::begin_with_edges(model, id, operation, uv, usable, None)
    }

    pub fn begin_with_edges(
        model: &Workspace,
        id: PlacementId,
        operation: Manipulation,
        uv: [f64; 2],
        usable: Rectangle,
        requested_edges: Option<[i8; 2]>,
    ) -> Result<Self, LayoutError> {
        if !model.valid()
            || model.fullscreen.is_some()
            || model.minimized(id)
            || uv
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            || requested_edges.is_some_and(|edges| {
                operation != Manipulation::Resize
                    || edges == [0; 2]
                    || edges.iter().any(|edge| !(-1..=1).contains(edge))
            })
        {
            return Err(LayoutError::InvalidState);
        }
        let source = model
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or(LayoutError::MissingItem)?
            .clone();
        let tiled = model.mode == WorkspaceLayout::Tiled && !source.floating;
        if !tiled && model.mode != WorkspaceLayout::Free && !source.floating {
            return Err(LayoutError::InvalidState);
        }
        let mut normal = model.clone();
        normal.request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        );
        let geometry = if tiled {
            let solution = normal.solve(usable, usable)?;
            let rectangle = solution
                .items
                .iter()
                .find(|item| item.id == id)
                .ok_or(LayoutError::MissingItem)?
                .rectangle;
            Geometry {
                origin: rectangle.origin(),
                size: rectangle.size(),
            }
        } else {
            normal.resolve_free_widths(usable);
            let item = normal
                .items
                .iter()
                .find(|item| item.id == id)
                .ok_or(LayoutError::MissingItem)?;
            let rectangle = free_rectangle(item);
            Geometry {
                origin: rectangle.origin(),
                size: rectangle.size(),
            }
        };
        let horizontal = if uv[0] < 0.5 { -1 } else { 1 };
        let vertical = if uv[1] < 0.5 { -1 } else { 1 };
        let mut edges = requested_edges.unwrap_or_else(|| {
            if tiled && uv[1] > 0.2 && uv[1] < 0.8 {
                [horizontal, 0]
            } else if tiled && uv[0] > 0.2 && uv[0] < 0.8 {
                [0, vertical]
            } else {
                [horizontal, vertical]
            }
        });
        let boundary = if operation != Manipulation::Resize {
            None
        } else if tiled && edges[1] < 0 {
            model.preceding_row(id)
        } else if tiled && edges[1] > 0 {
            Some(id)
        } else {
            None
        }
        .and_then(|id| model.gesture_row(id).map(|row| (id, row)));
        if tiled && boundary.is_none() {
            edges[1] = 0;
        }
        Ok(Self {
            source,
            operation,
            mode: model.mode,
            usable,
            maximized: model.maximized(id),
            geometry,
            edges,
            tiled,
            row: model.gesture_row(id).filter(|_| tiled),
            boundary,
            gap: model.gap,
            influence: model.influences.get(&id).copied(),
        })
    }

    pub fn id(&self) -> PlacementId {
        self.source.id
    }
    pub fn tiled(&self) -> bool {
        self.tiled
    }
    pub fn original(&self) -> Geometry {
        self.geometry
    }

    pub fn axes(&self) -> [bool; 2] {
        self.edges.map(|edge| edge != 0)
    }

    pub fn propose(
        &self,
        current: &Workspace,
        delta: [f64; 2],
        slot: Option<DropSlot>,
    ) -> Result<(Workspace, Geometry), LayoutError> {
        if !current.valid()
            || current.mode != self.mode
            || current.fullscreen.is_some()
            || current.minimized(self.source.id)
            || current.maximized(self.source.id) != self.maximized
            || current.items.iter().find(|item| item.id == self.source.id) != Some(&self.source)
            || self.tiled && current.gesture_row(self.source.id) != self.row
            || current.gap != self.gap
            || current.influences.get(&self.source.id).copied() != self.influence
            || self.boundary.is_some_and(|(id, row)| {
                current.minimized(id) || current.gesture_row(id) != Some(row)
            })
            || self.tiled
                && self.operation == Manipulation::Resize
                && self.edges[1] < 0
                && current.preceding_row(self.source.id) != self.boundary.map(|(id, _)| id)
            || delta
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 1_000_000_000.0)
        {
            return Err(LayoutError::InvalidState);
        }
        if delta == [0.0; 2]
            || self.operation == Manipulation::Move
                && self.tiled
                && slot.is_none_or(|slot| slot.anchor == self.source.id)
            || self.operation == Manipulation::Resize
                && (0..2).all(|axis| self.edges[axis] == 0 || delta[axis] == 0.0)
        {
            let geometry = self.current_geometry(current)?;
            return Ok((current.clone(), geometry));
        }
        let mut next = current.clone();
        next.request_state(
            self.source.id,
            crate::host::WindowRequest {
                maximized: Some(false),
                ..Default::default()
            },
        );
        next.resolve_free_widths(self.usable);
        if self.operation == Manipulation::Move && self.tiled {
            if let Some(slot) = slot {
                if current.minimized(slot.anchor) {
                    return Err(LayoutError::InvalidState);
                }
                next.place_item(self.source.id, slot.anchor, slot.side)?;
            }
        } else if self.tiled {
            for axis in 0..2 {
                let delta = if axis == 1 {
                    delta[axis]
                } else {
                    delta[axis] * f64::from(self.edges[axis])
                };
                let target = if axis == 1 {
                    self.boundary.map(|(id, _)| id)
                } else {
                    Some(self.source.id)
                };
                if self.edges[axis] != 0
                    && delta != 0.0
                    && let Some(target) = target
                {
                    next.resize_item(target, axis, delta, self.usable, 1.0)?;
                }
            }
        } else {
            let mut geometry = self.geometry;
            let mut rules = self.source.rules;
            if self.operation == Manipulation::Move {
                for axis in 0..2 {
                    geometry.origin[axis] = (geometry.origin[axis] + delta[axis])
                        .clamp(-1_000_000_000.0, 1_000_000_000.0);
                }
            } else {
                for axis in 0..2 {
                    if self.edges[axis] == 0 {
                        continue;
                    }
                    let opposite = geometry.origin[axis] + geometry.size[axis];
                    let min = if self.edges[axis] < 0 {
                        f64::from(rules.axes[axis].min).max(opposite - 1_000_000_000.0)
                    } else {
                        f64::from(rules.axes[axis].min)
                    };
                    let max = if self.edges[axis] < 0 {
                        f64::from(rules.axes[axis].max).min(opposite + 1_000_000_000.0)
                    } else {
                        f64::from(rules.axes[axis].max)
                    };
                    let size = (geometry.size[axis] + delta[axis] * f64::from(self.edges[axis]))
                        .clamp(min, max);
                    if self.edges[axis] < 0 {
                        geometry.origin[axis] += geometry.size[axis] - size;
                    }
                    geometry.size[axis] = size;
                    if size != self.geometry.size[axis] {
                        rules.axes[axis].size = size as f32;
                        rules.axes[axis].sizing = crate::layout::Sizing::Fixed;
                    }
                }
                next.configure(self.source.id, rules)?;
            }
            next.set_free_geometry(self.source.id, geometry)?;
        }
        let solution = next.solve(self.usable, self.usable)?;
        let geometry = if self.tiled {
            let rectangle = solution
                .items
                .iter()
                .find(|item| item.id == self.source.id)
                .ok_or(LayoutError::MissingItem)?
                .rectangle;
            Geometry {
                origin: rectangle.origin(),
                size: rectangle.size(),
            }
        } else {
            next.items
                .iter()
                .find(|item| item.id == self.source.id)
                .ok_or(LayoutError::MissingItem)?
                .free
        };
        if !next.valid() {
            return Err(LayoutError::InvalidState);
        }
        Ok((next, geometry))
    }

    fn current_geometry(&self, model: &Workspace) -> Result<Geometry, LayoutError> {
        let mut model = model.clone();
        let solution = model.solve(self.usable, self.usable)?;
        let rectangle = solution
            .items
            .iter()
            .find(|item| item.id == self.source.id)
            .ok_or(LayoutError::MissingItem)?
            .rectangle;
        Ok(Geometry {
            origin: rectangle.origin(),
            size: rectangle.size(),
        })
    }
}

#[cfg(test)]
mod tests;
