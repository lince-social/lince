use super::*;

mod sizing;
mod transfers;
mod compact;
mod scratch;
pub(super) use scratch::ScratchHome;
pub use compact::{CompactGroup, COMPACT_HEADER_HEIGHT};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SizeIntent {
    sizing: crate::layout::Sizing,
    size: f32,
}

impl SizeIntent {
    pub fn capture(rules: Rules) -> [Self; 2] {
        rules.axes.map(|axis| Self {
            sizing: axis.sizing,
            size: axis.size,
        })
    }

    fn valid(self) -> bool {
        self.size.is_finite() && (1.0..=LIMIT).contains(&self.size)
    }

    pub fn restore(intents: [Self; 2], rules: &mut Rules) {
        for (intent, axis) in intents.into_iter().zip(&mut rules.axes) {
            axis.sizing = intent.sizing;
            axis.size = intent.size.clamp(axis.min, axis.max);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    id: PlacementId,
    width: Width,
    floating: Option<Geometry>,
    floating_width: Option<FreeWidth>,
    floating_sizing: Option<[SizeIntent; 2]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: u64,
    height: Width,
    items: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Home {
    item: Member,
    row: u64,
    row_index: usize,
    index: usize,
    before: Option<PlacementId>,
    after: Option<PlacementId>,
    height: Width,
    free: Geometry,
    free_width: Option<FreeWidth>,
    free_sizing: [SizeIntent; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rows {
    rows: Vec<Row>,
    homes: Vec<Home>,
    #[serde(default)]
    compact: Vec<CompactGroup>,
    next_id: u64,
    pub scroll: [f64; 2],
    pub last_focus: Option<PlacementId>,
    free_view: Option<([f64; 2], f64)>,
}

impl Default for Rows {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            homes: Vec::new(),
            compact: Vec::new(),
            next_id: 1,
            scroll: [0.0; 2],
            last_focus: None,
            free_view: None,
        }
    }
}

impl Rows {
    fn location(&self, id: PlacementId) -> Option<(usize, usize)> {
        self.rows.iter().enumerate().find_map(|(row, entry)| {
            entry
                .items
                .iter()
                .position(|item| item.id == id)
                .map(|index| (row, index))
        })
    }

    pub fn contains(&self, id: PlacementId) -> bool {
        self.location(id).is_some()
    }

    fn fresh_id(&mut self) -> u64 {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.rows.iter().any(|row| row.id == id)
                && !self.homes.iter().any(|home| home.row == id)
            {
                return id;
            }
        }
    }

    pub fn valid(&self, items: &[Item]) -> bool {
        let all: BTreeMap<_, _> = items.iter().map(|item| (item.id, item.floating)).collect();
        let mut expected: BTreeSet<_> = items
            .iter()
            .filter(|item| !item.floating)
            .map(|item| item.id)
            .collect();
        let mut grouped = BTreeSet::new();
        let groups_valid = self.compact.iter().all(|group| {
            group.members.len() >= 2 && group.members.len() <= MAX_ITEMS
                && group.members.contains(&group.active)
                && group.members.iter().all(|id| {
                    all.get(id) == Some(&false) && grouped.insert(*id)
                        && self.homes.iter().filter(|home| home.item.id == *id).count() == 1
                })
                && self.location(group.active).is_some()
                && group.members.iter().filter(|id| **id != group.active).all(|id| self.location(*id).is_none())
        });
        for group in &self.compact {
            for id in group.members.iter().filter(|id| **id != group.active) { expected.remove(id); }
        }
        let mut arranged = BTreeSet::new();
        let mut row_ids = BTreeSet::new();
        let mut floating = BTreeSet::new();
        let member_valid = |member: &Member| {
            member.width.valid()
                && member.floating_width.is_none_or(FreeWidth::valid)
                && member
                    .floating_sizing
                    .is_none_or(|sizing| sizing.into_iter().all(SizeIntent::valid))
                && member
                    .floating
                    .is_none_or(|geometry| geometry.rectangle().is_some())
        };
        self.next_id != 0
            && groups_valid
            && self.rows.len() <= items.len()
            && self.homes.len() <= items.len()
            && self
                .scroll
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
            && self.rows.iter().all(|row| {
                row.id != 0
                    && row_ids.insert(row.id)
                    && row.height.valid()
                    && !row.items.is_empty()
                    && row
                        .items
                        .iter()
                        .all(|item| member_valid(item) && arranged.insert(item.id))
            })
            && arranged == expected
            && self.homes.iter().all(|home| {
                (all.get(&home.item.id) == Some(&true) || grouped.contains(&home.item.id))
                    && floating.insert(home.item.id)
                    && home.row != 0
                    && home.index <= MAX_ITEMS
                    && home.row_index <= MAX_ITEMS
                    && home.height.valid()
                    && member_valid(&home.item)
                    && home.free.rectangle().is_some()
                    && home.free_width.is_none_or(FreeWidth::valid)
                    && home.free_sizing.into_iter().all(SizeIntent::valid)
                    && [home.before, home.after]
                        .into_iter()
                        .flatten()
                        .all(|id| id != home.item.id && id.0 != [0; 16])
            })
            && self.last_focus.is_none_or(|id| arranged.contains(&id))
            && self.free_view.is_none_or(|(center, zoom)| {
                center
                    .into_iter()
                    .all(|value| value.is_finite() && value.abs() <= 1_000_000_000.0)
                    && zoom.is_finite()
                    && zoom > 0.0
            })
    }

    pub fn insert(&mut self, id: PlacementId, near: Option<PlacementId>) {
        let member = Member {
            id,
            width: Width::Half,
            floating: None,
            floating_width: None,
            floating_sizing: None,
        };
        if let Some((row, index)) = near
            .or(self.last_focus)
            .and_then(|near| self.location(near))
        {
            self.rows[row].items.insert(index + 1, member);
        } else if let Some(row) = self.rows.last_mut() {
            row.items.push(member);
        } else {
            let row_id = self.fresh_id();
            self.rows.push(Row {
                id: row_id,
                height: Width::Full,
                items: vec![member],
            });
        }
        self.last_focus = Some(id);
    }

    pub fn forget(
        &mut self,
        id: PlacementId,
        minimized: &BTreeSet<PlacementId>,
    ) -> Option<PlacementId> {
        let survivor = self.retire_compact_member(id);
        let neighbor = survivor.or_else(|| self.location(id).and_then(|(row, index)| {
            self.rows[row].items[index + 1..]
                .iter()
                .chain(self.rows[row].items[..index].iter().rev())
                .chain(
                    self.rows[..row]
                        .iter()
                        .rev()
                        .flat_map(|row| row.items.iter().rev()),
                )
                .chain(self.rows[row + 1..].iter().flat_map(|row| row.items.iter()))
                .map(|item| item.id)
                .find(|candidate| !minimized.contains(candidate))
        }));
        for row in &mut self.rows {
            row.items.retain(|item| item.id != id);
        }
        self.rows.retain(|row| !row.items.is_empty());
        self.homes.retain(|home| home.item.id != id);
        if self.last_focus == Some(id) {
            self.last_focus = neighbor;
        }
        neighbor
    }

    pub fn detach(
        &mut self,
        id: PlacementId,
        free: Geometry,
        free_width: Option<FreeWidth>,
        free_sizing: [SizeIntent; 2],
    ) -> Option<(Geometry, Option<FreeWidth>, [SizeIntent; 2])> {
        let (row_index, index) = self.location(id)?;
        let row = &mut self.rows[row_index];
        let before = index.checked_sub(1).map(|index| row.items[index].id);
        let after = row.items.get(index + 1).map(|item| item.id);
        let item = row.items.remove(index);
        let geometry = item.floating.map(|geometry| {
            (
                geometry,
                item.floating_width,
                item.floating_sizing.unwrap_or(free_sizing),
            )
        });
        self.homes.push(Home {
            item,
            row: row.id,
            row_index,
            index,
            before,
            after,
            height: row.height,
            free,
            free_width,
            free_sizing,
        });
        if self.last_focus == Some(id) {
            self.last_focus = before.or(after).or_else(|| {
                self.rows
                    .iter()
                    .flat_map(|row| &row.items)
                    .map(|item| item.id)
                    .next()
            });
        }
        self.rows.retain(|row| !row.items.is_empty());
        geometry
    }

    pub fn restore(
        &mut self,
        id: PlacementId,
        near: Option<PlacementId>,
        geometry: Geometry,
        free_width: Option<FreeWidth>,
        free_sizing: [SizeIntent; 2],
    ) -> Option<(Geometry, Option<FreeWidth>, [SizeIntent; 2])> {
        let Some(index) = self.homes.iter().position(|home| home.item.id == id) else {
            self.insert(id, near);
            return None;
        };
        let mut home = self.homes.remove(index);
        let restored = (home.free, home.free_width, home.free_sizing);
        home.item.floating = Some(geometry);
        home.item.floating_width = free_width;
        home.item.floating_sizing = Some(free_sizing);
        self.insert_home(home);
        self.last_focus = Some(id);
        Some(restored)
    }

    fn insert_home(&mut self, home: Home) {
        let row = self
            .rows
            .iter()
            .position(|row| row.id == home.row)
            .or_else(|| {
                home.before
                    .and_then(|id| self.location(id))
                    .map(|(row, _)| row)
            })
            .or_else(|| {
                home.after
                    .and_then(|id| self.location(id))
                    .map(|(row, _)| row)
            });
        let row_index = row.unwrap_or_else(|| {
            let index = home.row_index.min(self.rows.len());
            self.rows.insert(
                index,
                Row {
                    id: home.row,
                    height: home.height,
                    items: Vec::new(),
                },
            );
            index
        });
        let row = &mut self.rows[row_index];
        let index = home
            .after
            .and_then(|id| row.items.iter().position(|item| item.id == id))
            .or_else(|| {
                home.before
                    .and_then(|id| row.items.iter().position(|item| item.id == id))
                    .map(|index| index + 1)
            })
            .unwrap_or(home.index.min(row.items.len()));
        row.items.insert(index, home.item);
    }

    pub fn authored_free(
        &mut self,
        id: PlacementId,
        geometry: Geometry,
        free_width: Option<FreeWidth>,
        free_sizing: [SizeIntent; 2],
    ) {
        if let Some(home) = self.homes.iter_mut().find(|home| home.item.id == id) {
            home.free = geometry;
            home.free_width = free_width;
            home.free_sizing = free_sizing;
        }
    }

    pub fn order(&mut self, influences: &BTreeMap<PlacementId, Influence>) {
        let rank = |id: &PlacementId| influences.get(id).and_then(|influence| influence.order);
        for row in &mut self.rows {
            let slots: Vec<_> = row
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| rank(&item.id).is_some())
                .map(|(index, _)| index)
                .collect();
            let mut ordered: Vec<_> = slots
                .iter()
                .map(|index| row.items[*index].clone())
                .collect();
            ordered.sort_by_key(|item| (rank(&item.id), item.id));
            for (index, item) in slots.into_iter().zip(ordered) {
                row.items[index] = item;
            }
        }
        let slots: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.items.iter().any(|item| rank(&item.id).is_some()))
            .map(|(index, _)| index)
            .collect();
        let mut ordered: Vec<_> = slots
            .iter()
            .map(|index| self.rows[*index].clone())
            .collect();
        ordered.sort_by_key(|row| row.items.iter().filter_map(|item| rank(&item.id)).min());
        for (index, row) in slots.into_iter().zip(ordered) {
            self.rows[index] = row;
        }
    }

    pub fn solve(
        &self,
        usable: Rectangle,
        inner: [f64; 2],
        gap: f64,
        entries: &BTreeMap<PlacementId, &Item>,
        minimized: &BTreeSet<PlacementId>,
        influences: &BTreeMap<PlacementId, Influence>,
    ) -> (Vec<SolvedItem>, [f64; 2]) {
        let mut solved = Vec::with_capacity(entries.len());
        let mut extent = inner;
        let mut y = gap;
        for row in &self.rows {
            let visible: Vec<_> = row
                .items
                .iter()
                .filter(|item| !minimized.contains(&item.id))
                .collect();
            if visible.is_empty() {
                continue;
            }
            let height = visible
                .iter()
                .filter(|item| !self.compact.iter().any(|group| group.active == item.id))
                .map(|item| f64::from(entries[&item.id].rules.axes[1].min))
                .fold(row.height.spaced(inner[1], gap), f64::max)
                .max(if visible.iter().any(|item| self.compact.iter().any(|group| group.active == item.id)) { COMPACT_HEADER_HEIGHT + 1.0 } else { 1.0 });
            let mut row_height = height;
            let mut x = gap;
            for item in visible {
                let compact = self.compact.iter().any(|group| group.active == item.id);
                let rules = entries[&item.id].rules;
                let scale = influences.get(&item.id).copied().unwrap_or_default().scale;
                let requested_width = item.width.spaced(inner[0], gap);
                let width = (requested_width * scale)
                    .clamp(f64::from(rules.axes[0].min), f64::from(rules.axes[0].max));
                let size = [
                    width,
                    ((if compact { (height - COMPACT_HEADER_HEIGHT).max(1.0) } else { height }) * scale)
                        .clamp(f64::from(rules.axes[1].min), f64::from(rules.axes[1].max)),
                ];
                if !compact { row_height = row_height.max(size[1]); }
                solved.push(SolvedItem {
                    id: item.id,
                    rectangle: Rectangle::new(
                        [usable.origin()[0] + x, usable.origin()[1] + y + if compact { COMPACT_HEADER_HEIGHT } else { 0.0 }],
                        size,
                    )
                    .unwrap(),
                    floating: false,
                });
                extent[1] = extent[1].max(y + size[1] + gap);
                x += if compact { requested_width + gap } else { width + gap };
            }
            extent[0] = extent[0].max(x);
            y += row_height + gap;
        }
        extent[1] = extent[1].max(y);
        (solved, extent)
    }
}

impl Workspace {
    pub(super) fn gesture_row(&self, id: PlacementId) -> Option<(u64, Width, Width)> {
        let (row, index) = self.rows.location(id)?;
        let row = &self.rows.rows[row];
        Some((row.id, row.items[index].width, row.height))
    }

    pub(super) fn preceding_row(&self, id: PlacementId) -> Option<PlacementId> {
        let (row, _) = self.rows.location(id)?;
        self.rows.rows[..row].iter().rev().find_map(|row| {
            row.items
                .iter()
                .find(|item| !self.hidden(item.id))
                .map(|item| item.id)
        })
    }

    pub fn remember_free_view(&mut self, center: [f64; 2], zoom: f64) -> Result<(), LayoutError> {
        if !center
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 1_000_000_000.0)
            || !zoom.is_finite()
            || zoom <= 0.0
        {
            return Err(LayoutError::InvalidState);
        }
        self.rows.free_view = Some((center, zoom));
        Ok(())
    }

    pub fn free_view(&self) -> Option<([f64; 2], f64)> {
        self.rows.free_view
    }

    pub fn tiled_rows(&self) -> Vec<Vec<PlacementId>> {
        self.rows
            .rows
            .iter()
            .map(|row| row.items.iter().map(|item| item.id).collect())
            .collect()
    }

    pub(super) fn place_item(
        &mut self,
        id: PlacementId,
        anchor: PlacementId,
        side: Direction,
    ) -> Result<(), LayoutError> {
        if id == anchor {
            return Ok(());
        }
        let (source, index) = self.rows.location(id).ok_or(LayoutError::MissingItem)?;
        let (target, _) = self.rows.location(anchor).ok_or(LayoutError::MissingItem)?;
        let target_id = self.rows.rows[target].id;
        let height = self.rows.rows[source].height;
        let member = self.rows.rows[source].items.remove(index);
        self.rows.rows.retain(|row| !row.items.is_empty());
        let row = self
            .rows
            .rows
            .iter()
            .position(|row| row.id == target_id)
            .ok_or(LayoutError::MissingItem)?;
        if matches!(side, Direction::Up | Direction::Down) {
            let row_id = self.rows.fresh_id();
            self.rows.rows.insert(
                row + usize::from(side == Direction::Down),
                Row {
                    id: row_id,
                    height,
                    items: vec![member],
                },
            );
        } else {
            let index = self.rows.rows[row]
                .items
                .iter()
                .position(|member| member.id == anchor)
                .ok_or(LayoutError::MissingItem)?;
            self.rows.rows[row]
                .items
                .insert(index + usize::from(side == Direction::Right), member);
        }
        Ok(())
    }

    pub fn item_width(&self, id: PlacementId) -> Option<Width> {
        let (row, index) = self.rows.location(id)?;
        Some(self.rows.rows[row].items[index].width)
    }

    pub fn set_item_width(&mut self, id: PlacementId, width: Width) -> Result<(), LayoutError> {
        if !width.valid() {
            return Err(LayoutError::InvalidItem);
        }
        let (row, index) = self.rows.location(id).ok_or(LayoutError::MissingItem)?;
        self.rows.rows[row].items[index].width = width;
        Ok(())
    }

    pub(super) fn focus_row_direction(
        &mut self,
        direction: Direction,
        usable: Rectangle,
        solution: &Solution,
    ) -> Result<bool, LayoutError> {
        let Some((row, index)) = self.focused.and_then(|id| self.rows.location(id)) else {
            return Ok(false);
        };
        let visible = |item: &Member| !self.hidden(item.id);
        let target = match direction {
            Direction::Left => self.rows.rows[row].items[..index]
                .iter()
                .rev()
                .find(|item| visible(item))
                .map(|item| item.id),
            Direction::Right => self.rows.rows[row].items[index + 1..]
                .iter()
                .find(|item| visible(item))
                .map(|item| item.id),
            Direction::Up | Direction::Down => {
                let target = if direction == Direction::Up {
                    self.rows.rows[..row]
                        .iter()
                        .rev()
                        .find(|row| row.items.iter().any(visible))
                } else {
                    self.rows.rows[row + 1..]
                        .iter()
                        .find(|row| row.items.iter().any(visible))
                };
                let current = solution
                    .items
                    .iter()
                    .find(|item| Some(item.id) == self.focused)
                    .map(|item| center_of(item.rectangle)[0]);
                target.and_then(|row| {
                    current.and_then(|x| {
                        row.items
                            .iter()
                            .filter_map(|item| {
                                solution
                                    .items
                                    .iter()
                                    .find(|solved| solved.id == item.id)
                                    .map(|solved| {
                                        (item.id, (center_of(solved.rectangle)[0] - x).abs())
                                    })
                            })
                            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
                            .map(|(id, _)| id)
                    })
                })
            }
        };
        if let Some(target) = target {
            self.focus(target, usable)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn move_row_item(&mut self, direction: Direction, usable: Rectangle) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        let Some((row, index)) = self.rows.location(id) else {
            return false;
        };
        if matches!(direction, Direction::Left | Direction::Right) {
            let target = if direction == Direction::Left {
                (0..index).rev().find(|index| {
                    !self.hidden(self.rows.rows[row].items[*index].id)
                })
            } else {
                (index + 1..self.rows.rows[row].items.len()).find(|index| {
                    !self.hidden(self.rows.rows[row].items[*index].id)
                })
            };
            let Some(target) = target else {
                return false;
            };
            self.rows.rows[row].items.swap(index, target);
            return true;
        }
        let fullscreen = self.fullscreen.take();
        let solution = self.solve(usable, usable);
        self.fullscreen = fullscreen;
        let Ok(solution) = solution else {
            return false;
        };
        let Some((row, index)) = self.rows.location(id) else {
            return false;
        };
        let target = if direction == Direction::Up {
            (0..row).rev().find(|row| {
                self.rows.rows[*row]
                    .items
                    .iter()
                    .any(|item| !self.hidden(item.id))
            })
        } else {
            (row + 1..self.rows.rows.len()).find(|row| {
                self.rows.rows[*row]
                    .items
                    .iter()
                    .any(|item| !self.hidden(item.id))
            })
        };
        let target = if let Some(target) = target {
            self.rows.rows[target].id
        } else {
            if self.rows.rows[row].items.len() == 1 {
                return false;
            }
            let id = self.rows.fresh_id();
            let height = self.rows.rows[row].height;
            let at = row + usize::from(direction == Direction::Down);
            self.rows.rows.insert(
                at,
                Row {
                    id,
                    height,
                    items: Vec::new(),
                },
            );
            id
        };
        let Some(current) = solution.items.iter().find(|item| item.id == id) else {
            return false;
        };
        let x = center_of(current.rectangle)[0];
        let width = current.rectangle.size()[0];
        let target_row = self
            .rows
            .rows
            .iter()
            .position(|row| row.id == target)
            .unwrap();
        let mut slots = Vec::new();
        for (index, item) in self.rows.rows[target_row].items.iter().enumerate() {
            if let Some(item) = solution.items.iter().find(|solved| solved.id == item.id) {
                if slots.is_empty() {
                    slots.push((index, item.rectangle.origin()[0] + width * 0.5));
                }
                slots.push((
                    index + 1,
                    item.rectangle.origin()[0] + item.rectangle.size()[0] + self.gap + width * 0.5,
                ));
            }
        }
        let slot = slots
            .into_iter()
            .min_by(|a, b| {
                (a.1 - x)
                    .abs()
                    .total_cmp(&(b.1 - x).abs())
                    .then_with(|| a.0.cmp(&b.0))
            })
            .map_or(0, |(index, _)| index);
        let (row, _) = self.rows.location(id).unwrap();
        let item = self.rows.rows[row].items.remove(index);
        self.rows
            .rows
            .retain(|row| !row.items.is_empty() || row.id == target);
        let target = self
            .rows
            .rows
            .iter_mut()
            .find(|row| row.id == target)
            .unwrap();
        target.items.insert(slot.min(target.items.len()), item);
        true
    }
}

#[cfg(test)]
mod tests;
