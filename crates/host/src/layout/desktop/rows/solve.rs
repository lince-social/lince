// non-lean-yet

#[cfg(feature = "layout-telemetry")]
use lince_instrumentation::{Span, add};

use super::*;

struct Visible<'a> {
    member: &'a Member,
    entry: &'a Item,
    compact: bool,
    maximized: bool,
}

impl Workspace {
    pub(in crate::layout::desktop) fn solve_rows(
        &self,
        usable: Rectangle,
        inner: [f64; 2],
        gap: f64,
        entries: &BTreeMap<PlacementId, &Item>,
        hidden: &BTreeSet<PlacementId>,
    ) -> (Vec<SolvedItem>, [f64; 2]) {
        #[cfg(feature = "layout-telemetry")]
        let _solve = Span::new("desktop-row-solve");
        let compact: BTreeSet<_> = self.rows.compact.iter().map(|group| group.active).collect();
        let mut visible = Vec::new();
        let mut solved = Vec::with_capacity(entries.len());
        let mut extent = inner;
        let mut y = gap;
        #[cfg(feature = "layout-telemetry")]
        let mut prepared = 0;
        for row in &self.rows.rows {
            visible.clear();
            let mut row_compact = false;
            let mut row_maximized = false;
            for member in &row.items {
                if hidden.contains(&member.id) {
                    continue;
                }
                let item = Visible {
                    member,
                    entry: entries[&member.id],
                    compact: compact.contains(&member.id),
                    maximized: self.maximized.contains(&member.id),
                };
                row_compact |= item.compact;
                row_maximized |= item.maximized;
                visible.push(item);
            }
            #[cfg(feature = "layout-telemetry")]
            {
                prepared += visible.len() as u64;
            }
            if visible.is_empty() {
                continue;
            }
            let height = visible
                .iter()
                .filter(|item| !item.compact)
                .map(|item| f64::from(item.entry.rules.axes[1].min))
                .fold(
                    if row_maximized {
                        inner[1]
                    } else {
                        row.height.spaced(inner[1], gap)
                    },
                    f64::max,
                )
                .max(if row_compact {
                    COMPACT_HEADER_HEIGHT + 1.0
                } else {
                    1.0
                });
            let mut row_height = height;
            let mut x = gap;
            for item in &visible {
                let id = item.member.id;
                let rules = item.entry.rules;
                let scale = if item.maximized {
                    1.0
                } else {
                    self.influences.get(&id).copied().unwrap_or_default().scale
                };
                let requested_width = if item.maximized {
                    inner[0]
                } else {
                    item.member.width.spaced(inner[0], gap)
                };
                let width = (requested_width * scale)
                    .clamp(f64::from(rules.axes[0].min), f64::from(rules.axes[0].max));
                let size = [
                    width,
                    ((if item.compact {
                        (height - COMPACT_HEADER_HEIGHT).max(1.0)
                    } else {
                        height
                    }) * scale)
                        .clamp(f64::from(rules.axes[1].min), f64::from(rules.axes[1].max)),
                ];
                if !item.compact {
                    row_height = row_height.max(size[1]);
                }
                solved.push(SolvedItem {
                    id,
                    rectangle: Rectangle::new(
                        [
                            usable.origin()[0] + x,
                            usable.origin()[1]
                                + y
                                + if item.compact {
                                    COMPACT_HEADER_HEIGHT
                                } else {
                                    0.0
                                },
                        ],
                        size,
                    )
                    .unwrap(),
                    floating: false,
                });
                extent[1] = extent[1].max(y + size[1] + gap);
                x += if item.compact {
                    requested_width + gap
                } else {
                    width + gap
                };
            }
            extent[0] = extent[0].max(x);
            y += row_height + gap;
        }
        extent[1] = extent[1].max(y);
        #[cfg(feature = "layout-telemetry")]
        {
            add("desktop-row-solve-rows", self.rows.rows.len() as u64);
            add("desktop-row-solve-items-prepared", prepared);
            add("desktop-row-solve-compact-ids", compact.len() as u64);
        }
        (solved, extent)
    }
}
