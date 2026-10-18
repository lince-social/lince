use super::{LIMIT, Rules};
use crate::host::{Direction, Rectangle, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ITEMS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PlacementId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub origin: [f64; 2],
    pub size: [f64; 2],
}

impl Geometry {
    pub fn rectangle(self) -> Option<Rectangle> {
        self.origin
            .iter()
            .all(|value| value.abs() <= 1_000_000_000.0)
            .then_some(())?;
        self.size
            .iter()
            .all(|value| (1.0..=f64::from(LIMIT)).contains(value))
            .then_some(())?;
        Rectangle::new(self.origin, self.size)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: PlacementId,
    #[serde(default)]
    pub kind: ItemKind,
    pub rules: Rules,
    pub free: Geometry,
    pub floating: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    #[default]
    Native,
    Application,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Width {
    Third,
    Half,
    TwoThirds,
    Full,
    Pixels(f64),
}

impl Width {
    fn resolve(self, available: f64) -> f64 {
        match self {
            Self::Third => available / 3.0,
            Self::Half => available / 2.0,
            Self::TwoThirds => available * 2.0 / 3.0,
            Self::Full => available,
            Self::Pixels(pixels) => pixels,
        }
        .clamp(1.0, f64::from(LIMIT))
    }

    fn valid(self) -> bool {
        !matches!(self, Self::Pixels(size) if !size.is_finite() || !(1.0..=f64::from(LIMIT)).contains(&size))
    }

    fn next(self) -> Self {
        match self {
            Self::Third => Self::Half,
            Self::Half => Self::TwoThirds,
            Self::TwoThirds => Self::Full,
            Self::Full | Self::Pixels(_) => Self::Third,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    items: Vec<PlacementId>,
    width: Width,
    maximized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Influence {
    pub scale: f64,
    pub order: Option<i64>,
}

impl Default for Influence {
    fn default() -> Self {
        Self {
            scale: 1.0,
            order: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default)]
    minimized: BTreeSet<PlacementId>,
    #[serde(default)]
    minimized_fullscreen: BTreeSet<PlacementId>,
    #[serde(skip)]
    influences: BTreeMap<PlacementId, Influence>,
    mode: WorkspaceLayout,
    gap: f64,
    items: Vec<Item>,
    columns: Vec<Column>,
    focused: Option<PlacementId>,
    scroll: [f64; 2],
    fullscreen: Option<PlacementId>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(WorkspaceLayout::Free)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolvedItem {
    pub id: PlacementId,
    pub rectangle: Rectangle,
    pub floating: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    pub items: Vec<SolvedItem>,
    pub scroll: [f64; 2],
    pub extent: [f64; 2],
    pub focused: Option<PlacementId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    InvalidState,
    InvalidItem,
    DuplicateItem,
    TooManyItems,
    MissingItem,
}

impl Workspace {
    pub fn new(mode: WorkspaceLayout) -> Self {
        Self {
            minimized: BTreeSet::new(),
            minimized_fullscreen: BTreeSet::new(),
            influences: BTreeMap::new(),
            mode,
            gap: 8.0,
            items: Vec::new(),
            columns: Vec::new(),
            focused: None,
            scroll: [0.0; 2],
            fullscreen: None,
        }
    }

    pub fn mode(&self) -> WorkspaceLayout {
        self.mode
    }

    pub fn set_mode(&mut self, mode: WorkspaceLayout) {
        self.mode = mode;
        self.scroll = [0.0; 2];
    }

    pub fn focused(&self) -> Option<PlacementId> {
        self.focused
    }

    pub fn scroll(&self) -> [f64; 2] {
        self.scroll
    }

    pub fn fullscreen(&self) -> Option<PlacementId> {
        self.fullscreen
    }

    pub fn maximized(&self, id: PlacementId) -> bool {
        self.column(id)
            .is_some_and(|column| self.columns[column].maximized)
    }

    pub fn minimized(&self, id: PlacementId) -> bool { self.minimized.contains(&id) }

    pub fn clear_focus(&mut self) { self.focused = None; }

    pub fn set_minimized(&mut self, id: PlacementId, minimized: bool) -> bool {
        if !self.items.iter().any(|item| item.id == id) { return false; }
        if minimized {
            self.minimized.insert(id);
            if self.fullscreen == Some(id) { self.minimized_fullscreen.insert(id); self.fullscreen = None; }
            if self.focused == Some(id) {
                self.focused = self.columns.iter().flat_map(|column| &column.items).copied()
                    .chain(self.items.iter().map(|item| item.id)).find(|candidate| !self.minimized.contains(candidate));
            }
        } else {
            self.minimized.remove(&id);
            if self.minimized_fullscreen.remove(&id) && self.fullscreen.is_none() { self.fullscreen = Some(id); }
        }
        true
    }

    pub fn request_state(&mut self, id: PlacementId, request: crate::host::WindowRequest) -> bool {
        if !self.items.iter().any(|item| item.id == id) {
            return false;
        }
        if request.fullscreen == Some(true) || request.maximized == Some(true) { self.set_minimized(id, false); }
        if request.fullscreen == Some(false) { self.minimized_fullscreen.remove(&id); }
        if let Some(enabled) = request.fullscreen {
            if enabled {
                self.fullscreen = Some(id);
            } else if self.fullscreen == Some(id) {
                self.fullscreen = None;
            }
        }
        if let Some(enabled) = request.maximized
            && let Some(column) = self.column(id)
        {
            self.columns[column].maximized = enabled;
        }
        if let Some(minimized) = request.minimized { self.set_minimized(id, minimized); }
        true
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn set_gap(&mut self, gap: f64) -> Result<(), LayoutError> {
        if !gap.is_finite() || !(0.0..=10_000.0).contains(&gap) {
            return Err(LayoutError::InvalidState);
        }
        self.gap = gap;
        Ok(())
    }

    pub fn valid(&self) -> bool {
        if self.items.len() > MAX_ITEMS
            || self.columns.len() > self.items.len()
            || !self.gap.is_finite()
            || !(0.0..=10_000.0).contains(&self.gap)
            || self
                .scroll
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return false;
        }
        let mut all = BTreeSet::new();
        let mut tiled = BTreeSet::new();
        for item in &self.items {
            if item.id.0 == [0; 16]
                || !all.insert(item.id)
                || !item.rules.valid()
                || item.free.rectangle().is_none()
            {
                return false;
            }
            if !item.floating {
                tiled.insert(item.id);
            }
        }
        let mut arranged = BTreeSet::new();
        for column in &self.columns {
            if column.items.is_empty()
                || !column.width.valid()
                || column.items.iter().any(|id| !arranged.insert(*id))
            {
                return false;
            }
        }
        tiled == arranged
            && self.minimized.iter().all(|id| all.contains(id))
            && self.minimized_fullscreen.iter().all(|id| self.minimized.contains(id))
            && self.focused.is_none_or(|id| !self.minimized.contains(&id))
            && self.fullscreen.is_none_or(|id| !self.minimized.contains(&id))
            && self.influences.iter().all(|(id, influence)| {
                all.contains(id)
                    && influence.scale.is_finite()
                    && (0.05..=20.0).contains(&influence.scale)
            })
            && self.focused.is_none_or(|id| all.contains(&id))
            && self.fullscreen.is_none_or(|id| all.contains(&id))
    }

    pub fn insert(&mut self, item: Item) -> Result<(), LayoutError> {
        if item.id.0 == [0; 16] || !item.rules.valid() || item.free.rectangle().is_none() {
            return Err(LayoutError::InvalidItem);
        }
        if self.items.iter().any(|previous| previous.id == item.id) {
            return Err(LayoutError::DuplicateItem);
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(LayoutError::TooManyItems);
        }
        if !item.floating {
            let position = self
                .focused
                .and_then(|id| self.column(id))
                .map_or(self.columns.len(), |column| column + 1);
            self.columns
                .insert(position, new_column(item.id, Width::Half));
        }
        self.focused = Some(item.id);
        self.items.push(item);
        Ok(())
    }

    pub fn remove(&mut self, id: PlacementId) -> Option<Item> {
        let index = self.items.iter().position(|item| item.id == id)?;
        let column = self.column(id).unwrap_or(0);
        let row = self
            .columns
            .get(column)
            .and_then(|column| column.items.iter().position(|item| *item == id))
            .unwrap_or(0);
        self.remove_from_columns(id);
        let removed = self.items.remove(index);
        self.minimized.remove(&id);
        self.minimized_fullscreen.remove(&id);
        self.influences.remove(&id);
        if self.fullscreen == Some(id) {
            self.fullscreen = None;
        }
        if self.focused == Some(id) {
            self.focused = self.columns.iter().skip(column.min(self.columns.len().saturating_sub(1)))
                .flat_map(|column| &column.items).skip(row).copied()
                .chain(self.items.iter().rev().map(|item| item.id)).find(|id| !self.minimized.contains(id));
        }
        Some(removed)
    }

    pub fn configure(&mut self, id: PlacementId, rules: Rules) -> Result<(), LayoutError> {
        if !rules.valid() {
            return Err(LayoutError::InvalidItem);
        }
        self.item_mut(id)?.rules = rules;
        Ok(())
    }

    pub fn influence(&mut self, id: PlacementId, influence: Influence) -> Result<(), LayoutError> {
        if !influence.scale.is_finite() || !(0.05..=20.0).contains(&influence.scale) {
            return Err(LayoutError::InvalidItem);
        }
        self.item_mut(id)?;
        if influence == Influence::default() {
            self.influences.remove(&id);
        } else {
            self.influences.insert(id, influence);
        }
        Ok(())
    }

    pub fn set_free_geometry(
        &mut self,
        id: PlacementId,
        geometry: Geometry,
    ) -> Result<(), LayoutError> {
        if geometry.rectangle().is_none() {
            return Err(LayoutError::InvalidItem);
        }
        self.item_mut(id)?.free = geometry;
        Ok(())
    }

    pub fn focus(&mut self, id: PlacementId, usable: Rectangle) -> Result<(), LayoutError> {
        self.item_mut(id)?;
        self.set_minimized(id, false);
        self.focused = Some(id);
        if self.fullscreen.is_some() {
            self.fullscreen = Some(id);
            return Ok(());
        }
        let solution = self.solve(usable, usable)?;
        if let Some(item) = solution
            .items
            .iter()
            .find(|item| item.id == id && !item.floating)
        {
            for axis in 0..2 {
                let low = usable.origin()[axis] + self.gap.min(usable.size()[axis] / 4.0);
                let high = usable.origin()[axis] + usable.size()[axis]
                    - self.gap.min(usable.size()[axis] / 4.0);
                let origin = item.rectangle.origin()[axis];
                let size = item.rectangle.size()[axis];
                let delta = if origin < low {
                    origin - low
                } else if origin + size > high {
                    (origin + size - high).min((origin - low).max(0.0))
                } else {
                    0.0
                };
                self.scroll[axis] = (self.scroll[axis] + delta).max(0.0);
            }
        }
        Ok(())
    }

    pub fn focus_direction(
        &mut self,
        direction: Direction,
        usable: Rectangle,
    ) -> Result<bool, LayoutError> {
        let fullscreen = self.fullscreen.take();
        let solution = self.solve(usable, usable);
        self.fullscreen = fullscreen;
        let solution = solution?;
        let Some(current) = solution
            .items
            .iter()
            .find(|item| Some(item.id) == self.focused)
        else {
            if let Some(id) = solution.items.first().map(|item| item.id) { self.focus(id, usable)?; return Ok(true); }
            return Ok(false);
        };
        let axis = match direction {
            Direction::Left | Direction::Right => 0,
            _ => 1,
        };
        let sign = match direction {
            Direction::Left | Direction::Up => -1.0,
            _ => 1.0,
        };
        let center = center_of(current.rectangle);
        let target = solution
            .items
            .iter()
            .filter(|item| item.id != current.id)
            .filter_map(|item| {
                let other = center_of(item.rectangle);
                let primary = (other[axis] - center[axis]) * sign;
                if primary <= 0.001 {
                    return None;
                }
                let cross = (other[1 - axis] - center[1 - axis]).abs();
                let overlap = overlap(current.rectangle, item.rectangle, 1 - axis);
                Some((item.id, !overlap, primary, cross))
            })
            .min_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| a.2.total_cmp(&b.2))
                    .then_with(|| a.3.total_cmp(&b.3))
                    .then_with(|| a.0.cmp(&b.0))
            })
            .map(|candidate| candidate.0);
        if let Some(target) = target {
            self.focus(target, usable)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn move_focused(&mut self, direction: Direction) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        let Some(column) = self.column(id) else {
            return false;
        };
        let row = self.columns[column]
            .items
            .iter()
            .position(|item| *item == id)
            .unwrap();
        if matches!(
            self.mode,
            WorkspaceLayout::Row | WorkspaceLayout::Column | WorkspaceLayout::Grid
        ) {
            let minimized = &self.minimized;
            let positions: Vec<_> = self
                .columns
                .iter()
                .enumerate()
                .flat_map(|(column, entry)| {
                    entry
                        .items
                        .iter()
                        .enumerate()
                        .filter(move |(_, id)| !minimized.contains(id))
                        .map(move |(row, id)| (column, row, *id))
                })
                .collect();
            let index = positions
                .iter()
                .position(|(_, _, current)| *current == id)
                .unwrap();
            let columns = match self.mode {
                WorkspaceLayout::Column => 1,
                WorkspaceLayout::Grid => (positions.len() as f64).sqrt().ceil() as usize,
                _ => positions.len(),
            };
            let target = match direction {
                Direction::Left => (index % columns > 0).then(|| index - 1),
                Direction::Right => (index % columns + 1 < columns).then(|| index + 1),
                Direction::Up => index.checked_sub(columns),
                Direction::Down => index.checked_add(columns),
            }
            .filter(|target| *target < positions.len());
            let Some(target) = target else {
                return false;
            };
            let (other_column, other_row, other) = positions[target];
            self.columns[column].items[row] = other;
            self.columns[other_column].items[other_row] = id;
            return true;
        }
        match direction {
            Direction::Up | Direction::Down => {
                let target = if direction == Direction::Up {
                    (0..row).rev().find(|row| !self.minimized.contains(&self.columns[column].items[*row]))
                } else {
                    (row + 1..self.columns[column].items.len()).find(|row| !self.minimized.contains(&self.columns[column].items[*row]))
                };
                let Some(target) = target else {
                    return false;
                };
                self.columns[column].items.swap(row, target);
            }
            Direction::Left | Direction::Right => {
                if self.columns[column].items.iter().filter(|id| !self.minimized.contains(id)).count() > 1 {
                    let width = self.columns[column].width;
                    self.columns[column].items.remove(row);
                    let target = column + usize::from(direction == Direction::Right);
                    self.columns.insert(target, new_column(id, width));
                } else {
                    let target = self.adjacent_column(column, direction);
                    let Some(target) = target else {
                        return false;
                    };
                    self.columns.swap(column, target);
                }
            }
        }
        true
    }

    pub fn stack_focused(&mut self, direction: Direction) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        let Some(column) = self.column(id) else {
            return false;
        };
        let target = self.adjacent_column(column, direction);
        let Some(target) = target else {
            return false;
        };
        self.columns[target].items.push(id);
        self.columns[column].items.retain(|item| *item != id);
        self.columns.retain(|column| !column.items.is_empty());
        true
    }

    pub fn toggle_floating(
        &mut self,
        id: PlacementId,
        current: Rectangle,
    ) -> Result<(), LayoutError> {
        let geometry = Geometry {
            origin: current.origin(),
            size: current.size(),
        };
        if geometry.rectangle().is_none() {
            return Err(LayoutError::InvalidItem);
        }
        let floating = self.item_mut(id)?.floating;
        if floating {
            let position = self
                .focused
                .and_then(|id| self.column(id))
                .map_or(self.columns.len(), |column| column + 1);
            self.columns.insert(position, new_column(id, Width::Half));
        } else {
            self.remove_from_columns(id);
        }
        let item = self.item_mut(id)?;
        item.floating = !floating;
        item.free = geometry;
        Ok(())
    }

    pub fn cycle_width(&mut self) -> bool {
        let Some(column) = self.focused.and_then(|id| self.column(id)) else {
            return false;
        };
        self.columns[column].width = self.columns[column].width.next();
        true
    }

    pub fn resize_column(&mut self, delta: f64, available: f64) -> bool {
        if !delta.is_finite() || !available.is_finite() || available <= 0.0 {
            return false;
        }
        let Some(column) = self.focused.and_then(|id| self.column(id)) else {
            return false;
        };
        self.columns[column].width = Width::Pixels(
            (self.columns[column].width.resolve(available) + delta).clamp(1.0, f64::from(LIMIT)),
        );
        true
    }

    pub fn resize_focused_height(&mut self, delta: f32) -> bool {
        if !delta.is_finite() {
            return false;
        }
        let Some(id) = self.focused else {
            return false;
        };
        let Ok(item) = self.item_mut(id) else {
            return false;
        };
        let axis = &mut item.rules.axes[1];
        axis.size = (axis.size + delta).clamp(axis.min, axis.max);
        true
    }

    pub fn toggle_maximized(&mut self) -> bool {
        let Some(column) = self.focused.and_then(|id| self.column(id)) else {
            return false;
        };
        self.columns[column].maximized = !self.columns[column].maximized;
        true
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = if self.fullscreen == self.focused {
            None
        } else {
            self.focused
        };
    }

    pub fn solve(&mut self, usable: Rectangle, output: Rectangle) -> Result<Solution, LayoutError> {
        if !self.valid() {
            return Err(LayoutError::InvalidState);
        }
        self.apply_influence_order();
        let gap = self.gap.min(usable.size()[0].min(usable.size()[1]) / 4.0);
        let inner = [usable.size()[0] - gap * 2.0, usable.size()[1] - gap * 2.0];
        if self.mode == WorkspaceLayout::ScrollingColumns {
            self.split_overflow(inner[1], gap);
        }
        let columns: Vec<_> = self.columns.iter().filter_map(|column| {
            let mut column = column.clone();
            column.items.retain(|id| !self.minimized.contains(id));
            (!column.items.is_empty()).then_some(column)
        }).collect();
        let entries: BTreeMap<_, _> = self.items.iter().filter(|item| !self.minimized.contains(&item.id)).map(|item| (item.id, item)).collect();
        let mut solved = Vec::with_capacity(self.items.len());
        let mut extent = inner;
        match self.mode {
            WorkspaceLayout::Free => {
                for item in self.items.iter().filter(|item| !item.floating && !self.minimized.contains(&item.id)) {
                    solved.push(SolvedItem {
                        id: item.id,
                        rectangle: free_rectangle(item),
                        floating: false,
                    });
                }
            }
            WorkspaceLayout::ScrollingColumns => {
                let mut x = gap;
                for column in &columns {
                    let requested = if column.maximized {
                        inner[0]
                    } else {
                        column.width.resolve(inner[0])
                    };
                    let width = column
                        .items
                        .iter()
                        .map(|id| {
                            let axis = entries[id].rules.axes[0];
                            (requested * self.influences.get(id).copied().unwrap_or_default().scale)
                                .clamp(f64::from(axis.min), f64::from(axis.max))
                        })
                        .fold(1.0, f64::max);
                    let heights = distribute(
                        column.items.iter().map(|id| {
                            (
                                entries[id],
                                self.influences.get(id).copied().unwrap_or_default().scale,
                            )
                        }),
                        inner[1] - gap * column.items.len().saturating_sub(1) as f64,
                        1,
                    );
                    let mut y = gap;
                    for (id, height) in column.items.iter().zip(heights) {
                        let axis = entries[id].rules.axes[0];
                        let size = [
                            width.clamp(f64::from(axis.min), f64::from(axis.max)),
                            height,
                        ];
                        solved.push(SolvedItem {
                            id: *id,
                            rectangle: Rectangle::new(
                                [x + usable.origin()[0], y + usable.origin()[1]],
                                size,
                            )
                            .unwrap(),
                            floating: false,
                        });
                        y += height + gap;
                    }
                    extent[1] = extent[1].max(y);
                    x += width + gap;
                }
                extent[0] = extent[0].max(x);
            }
            WorkspaceLayout::Row | WorkspaceLayout::Column | WorkspaceLayout::Grid => {
                let ids: Vec<_> = columns
                    .iter()
                    .flat_map(|column| &column.items)
                    .copied()
                    .collect();
                let count = ids.len().max(1);
                let columns = match self.mode {
                    WorkspaceLayout::Column => 1,
                    WorkspaceLayout::Grid => (count as f64).sqrt().ceil() as usize,
                    _ => count,
                };
                let rows = count.div_ceil(columns);
                let cell = [
                    (inner[0] - gap * columns.saturating_sub(1) as f64) / columns as f64,
                    (inner[1] - gap * rows.saturating_sub(1) as f64) / rows as f64,
                ];
                let mut widths = vec![1.0_f64; columns];
                let mut heights = vec![1.0_f64; rows];
                let mut sizes = Vec::with_capacity(ids.len());
                for (index, id) in ids.iter().enumerate() {
                    let item = entries[id];
                    let scale = self.influences.get(id).copied().unwrap_or_default().scale;
                    let size: [f64; 2] = std::array::from_fn(|axis| {
                        (cell[axis] * scale).clamp(
                            f64::from(item.rules.axes[axis].min),
                            f64::from(item.rules.axes[axis].max),
                        )
                    });
                    widths[index % columns] = widths[index % columns].max(size[0]);
                    heights[index / columns] = heights[index / columns].max(size[1]);
                    sizes.push(size);
                }
                for (axis, lengths) in [&mut widths, &mut heights].into_iter().enumerate() {
                    let mut offset = gap;
                    for length in lengths {
                        let next = offset + *length + gap;
                        *length = offset;
                        offset = next;
                    }
                    extent[axis] = extent[axis].max(offset);
                }
                for (index, (id, size)) in ids.iter().zip(sizes).enumerate() {
                    solved.push(SolvedItem {
                        id: *id,
                        rectangle: Rectangle::new(
                            [
                                widths[index % columns] + usable.origin()[0],
                                heights[index / columns] + usable.origin()[1],
                            ],
                            size,
                        )
                        .unwrap(),
                        floating: false,
                    });
                }
            }
        }
        for axis in 0..2 {
            self.scroll[axis] =
                self.scroll[axis].clamp(0.0, (extent[axis] - usable.size()[axis]).max(0.0));
        }
        if self.mode != WorkspaceLayout::Free {
            for item in &mut solved {
                item.rectangle = Rectangle::new(
                    std::array::from_fn(|axis| item.rectangle.origin()[axis] - self.scroll[axis]),
                    item.rectangle.size(),
                )
                .unwrap();
            }
        }
        for item in self.items.iter().filter(|item| item.floating && !self.minimized.contains(&item.id)) {
            solved.push(SolvedItem {
                id: item.id,
                rectangle: free_rectangle(item),
                floating: true,
            });
        }
        if let Some(id) = self.fullscreen {
            solved.retain(|item| item.id == id);
            if let Some(item) = solved.first_mut() {
                item.rectangle = output;
            }
        }
        Ok(Solution {
            items: solved,
            scroll: self.scroll,
            extent,
            focused: self.focused,
        })
    }

    #[cfg(test)]
    fn item(&self, id: PlacementId) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    fn item_mut(&mut self, id: PlacementId) -> Result<&mut Item, LayoutError> {
        self.items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(LayoutError::MissingItem)
    }

    fn column(&self, id: PlacementId) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.items.contains(&id))
    }

    fn adjacent_column(&self, column: usize, direction: Direction) -> Option<usize> {
        let visible = |column: &usize| self.columns[*column].items.iter().any(|id| !self.minimized.contains(id));
        match direction {
            Direction::Left => (0..column).rev().find(visible),
            Direction::Right => (column + 1..self.columns.len()).find(visible),
            _ => None,
        }
    }

    fn remove_from_columns(&mut self, id: PlacementId) {
        for column in &mut self.columns {
            column.items.retain(|item| *item != id);
        }
        self.columns.retain(|column| !column.items.is_empty());
    }

    fn split_overflow(&mut self, height: f64, gap: f64) {
        let minimums: BTreeMap<_, _> = self
            .items
            .iter()
            .map(|item| (item.id, f64::from(item.rules.axes[1].min)))
            .collect();
        let mut index = 0;
        while index < self.columns.len() {
            let column = &self.columns[index];
            let mut used = 0.0;
            let mut visible = 0;
            let split = column.items.iter().enumerate().find_map(|(row, id)| {
                if self.minimized.contains(id) { return None; }
                used += minimums[id] + if visible == 0 { 0.0 } else { gap };
                visible += 1;
                (visible > 1 && used > height).then_some(row)
            });
            if let Some(split) = split {
                let items = self.columns[index].items.split_off(split);
                let width = self.columns[index].width;
                self.columns.insert(
                    index + 1,
                    Column {
                        items,
                        width,
                        maximized: false,
                    },
                );
            }
            index += 1;
        }
    }

    fn apply_influence_order(&mut self) {
        let rank = |id: &PlacementId| {
            self.influences
                .get(id)
                .and_then(|influence| influence.order)
        };
        for column in &mut self.columns {
            let positions: Vec<_> = column
                .items
                .iter()
                .enumerate()
                .filter(|(_, id)| rank(id).is_some())
                .map(|(index, _)| index)
                .collect();
            let mut ordered: Vec<_> = positions.iter().map(|index| column.items[*index]).collect();
            ordered.sort_by_key(|id| (rank(id), *id));
            for (position, id) in positions.into_iter().zip(ordered) {
                column.items[position] = id;
            }
        }
        let positions: Vec<_> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.items.iter().any(|id| rank(id).is_some()))
            .map(|(index, _)| index)
            .collect();
        let mut ordered: Vec<_> = positions
            .iter()
            .map(|index| self.columns[*index].clone())
            .collect();
        ordered.sort_by_key(|column| column.items.iter().filter_map(rank).min());
        for (position, column) in positions.into_iter().zip(ordered) {
            self.columns[position] = column;
        }
    }
}

fn new_column(id: PlacementId, width: Width) -> Column {
    Column {
        items: vec![id],
        width,
        maximized: false,
    }
}

fn free_rectangle(item: &Item) -> Rectangle {
    let size = std::array::from_fn(|axis| {
        item.free.size[axis].clamp(
            f64::from(item.rules.axes[axis].min),
            f64::from(item.rules.axes[axis].max),
        )
    });
    Rectangle::new(item.free.origin, size).unwrap()
}

fn center_of(rectangle: Rectangle) -> [f64; 2] {
    std::array::from_fn(|axis| rectangle.origin()[axis] + rectangle.size()[axis] * 0.5)
}

fn overlap(a: Rectangle, b: Rectangle, axis: usize) -> bool {
    a.origin()[axis] < b.origin()[axis] + b.size()[axis]
        && b.origin()[axis] < a.origin()[axis] + a.size()[axis]
}

fn distribute<'a>(
    items: impl Iterator<Item = (&'a Item, f64)>,
    available: f64,
    axis: usize,
) -> Vec<f64> {
    let (rules, weights): (Vec<_>, Vec<_>) = items
        .map(|(item, scale)| {
            (
                item.rules.axes[axis],
                f64::from(item.rules.axes[axis].size) * scale,
            )
        })
        .unzip();
    let mut sizes: Vec<_> = rules.iter().map(|rule| f64::from(rule.min)).collect();
    let mut remaining = (available - sizes.iter().sum::<f64>()).max(0.0);
    let mut capacities: Vec<_> = rules
        .iter()
        .zip(&weights)
        .map(|(rule, weight)| (f64::from(rule.max - rule.min) / *weight, *weight))
        .collect();
    capacities.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut growing = capacities.iter().map(|capacity| capacity.1).sum::<f64>();
    let mut level = 0.0;
    for (capacity, weight) in capacities {
        let needed = (capacity - level) * growing;
        if remaining <= needed {
            level += remaining / growing;
            break;
        }
        remaining -= needed;
        level = capacity;
        growing -= weight;
    }
    for ((rule, size), weight) in rules.iter().zip(&mut sizes).zip(weights) {
        *size += (level * weight).min(f64::from(rule.max - rule.min));
    }
    sizes
}

#[cfg(test)]
mod tests;
