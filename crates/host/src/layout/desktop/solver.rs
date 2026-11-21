//! Projects desktop layout into visible rectangles and scroll bounds.
// non-lean-yet

#[cfg(feature = "layout-telemetry")]
use lince_instrumentation::{Span, add};

use std::{
    array::from_fn,
    cell::LazyCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::{Rectangle, WorkspaceLayout};

use super::{
    COMPACT_HEADER_HEIGHT, Column, Item, LayoutError, PlacementId, Solution, SolvedItem, Workspace,
    free_rectangle, ordering::InfluenceOrder,
};

impl Workspace {
    pub fn solve(&mut self, usable: Rectangle, output: Rectangle) -> Result<Solution, LayoutError> {
        if !self.valid() {
            return Err(LayoutError::InvalidState);
        }
        self.resolve_free_widths(usable);
        self.apply_influence_order();
        let hidden = self.hidden_items();
        let gap = self.gap.min(usable.size()[0].min(usable.size()[1]) / 4.0);
        let inner = [usable.size()[0] - gap * 2.0, usable.size()[1] - gap * 2.0];
        if self.mode == WorkspaceLayout::ScrollingColumns {
            self.split_overflow(inner[1], gap, &hidden);
        }
        let source_columns = &self.columns;
        let columns = LazyCell::new(|| {
            #[cfg(feature = "layout-telemetry")]
            add("desktop-solver-columns-prepared", 1);
            source_columns
                .iter()
                .filter_map(|column| {
                    let mut column = column.clone();
                    column.items.retain(|id| !hidden.contains(id));
                    (!column.items.is_empty()).then_some(column)
                })
                .collect::<Vec<_>>()
        });
        let source_items = &self.items;
        let entries = LazyCell::new(|| {
            #[cfg(feature = "layout-telemetry")]
            add("desktop-solver-indexes-prepared", 1);
            source_items
                .iter()
                .filter(|item| !hidden.contains(&item.id))
                .map(|item| (item.id, item))
                .collect::<BTreeMap<_, _>>()
        });
        let mut solved = Vec::new();
        if self.mode != WorkspaceLayout::Tiled {
            solved.reserve(self.items.len());
        }
        let mut extent = inner;
        match self.mode {
            WorkspaceLayout::Tiled => {
                let (items, size) = self.solve_rows(usable, inner, gap, &entries, &hidden);
                solved = items;
                extent = size;
            }
            WorkspaceLayout::Free => {
                for item in self
                    .items
                    .iter()
                    .filter(|item| !item.floating && !hidden.contains(&item.id))
                {
                    solved.push(SolvedItem {
                        id: item.id,
                        rectangle: free_rectangle(item),
                        floating: false,
                    });
                }
            }
            WorkspaceLayout::ScrollingColumns => {
                let mut x = gap;
                for column in columns.iter() {
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
        for ((scroll, extent), size) in self.scroll.iter_mut().zip(extent).zip(usable.size()) {
            *scroll = scroll.clamp(0.0, (extent - size).max(0.0));
        }
        if self.mode != WorkspaceLayout::Free {
            for item in &mut solved {
                item.rectangle = Rectangle::new(
                    from_fn(|axis| item.rectangle.origin()[axis] - self.scroll[axis]),
                    item.rectangle.size(),
                )
                .unwrap();
            }
        }
        for item in self
            .items
            .iter()
            .filter(|item| item.floating && !hidden.contains(&item.id))
        {
            solved.push(SolvedItem {
                id: item.id,
                rectangle: free_rectangle(item),
                floating: true,
            });
        }
        {
            #[cfg(feature = "layout-telemetry")]
            let _maximized = Span::new("desktop-maximized-placement");
            #[cfg(feature = "layout-telemetry")]
            let mut candidates = 0;
            #[cfg(feature = "layout-telemetry")]
            let mut applied = 0;
            for item in &mut solved {
                if matches!(
                    self.mode,
                    WorkspaceLayout::Tiled | WorkspaceLayout::ScrollingColumns
                ) && !item.floating
                {
                    continue;
                }
                #[cfg(feature = "layout-telemetry")]
                {
                    candidates += 1;
                }
                if !self.maximized.contains(&item.id) {
                    continue;
                }
                let rules = entries[&item.id].rules;
                let header = if self.mode == WorkspaceLayout::Tiled
                    && self.compact_group(item.id).is_some()
                {
                    COMPACT_HEADER_HEIGHT
                } else {
                    0.0
                };
                let size = std::array::from_fn(|axis| {
                    (inner[axis] - if axis == 1 { header } else { 0.0 })
                        .max(1.0)
                        .clamp(
                            f64::from(rules.axes[axis].min),
                            f64::from(rules.axes[axis].max),
                        )
                });
                item.rectangle = Rectangle::new(
                    [usable.origin()[0] + gap, usable.origin()[1] + gap + header],
                    size,
                )
                .unwrap();
                item.floating = false;
                #[cfg(feature = "layout-telemetry")]
                {
                    applied += 1;
                }
            }
            #[cfg(feature = "layout-telemetry")]
            {
                add("desktop-maximized-candidates", candidates);
                add("desktop-maximized-overrides", applied);
            }
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

    fn split_overflow(&mut self, height: f64, gap: f64, hidden: &BTreeSet<PlacementId>) {
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
                if hidden.contains(id) {
                    return None;
                }
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
        #[cfg(feature = "layout-telemetry")]
        let _ordering = Span::new("desktop-influence-order");
        if !self
            .influences
            .values()
            .any(|influence| influence.order.is_some())
        {
            return;
        }
        let mut ordering = InfluenceOrder::default();
        self.rows.order(&self.influences, &mut ordering);
        let influences = &self.influences;
        let rank = |id: &PlacementId| influences.get(id).and_then(|influence| influence.order);
        for column in &mut self.columns {
            ordering.members(&mut column.items, |id| rank(id).map(|rank| (rank, *id)));
        }
        ordering.groups(&mut self.columns, |column| {
            column.items.iter().filter_map(rank).min()
        });
        ordering.record();
    }
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
