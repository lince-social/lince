use super::{Arrangement, Rules, Sizing};
use bevy::prelude::*;

pub(super) struct Item {
    pub rules: Rules,
    pub equal_axes: bool,
    pub parent: Option<usize>,
    pub offset: Vec2,
    pub intrinsic: Vec2,
    pub available: Vec2,
    pub size: Vec2,
    pub content: Vec2,
    pub children: Vec<usize>,
}

fn keep_shape(item: &mut Item) {
    if item.equal_axes {
        let minimum = item.rules.axes[0].min.max(item.rules.axes[1].min);
        let maximum = item.rules.axes[0]
            .max
            .min(item.rules.axes[1].max)
            .max(minimum);
        item.size = Vec2::splat(item.size.max_element().clamp(minimum, maximum));
    }
}

fn arrange(items: &mut [Item], index: usize) -> Vec2 {
    let rules = items[index].rules;
    let children = items[index].children.clone();
    let mut extent = items[index].intrinsic;
    let mut cursor = Vec2::splat(rules.padding);
    let columns = usize::from(rules.columns);
    let mut widths = vec![0.0_f32; columns];
    let mut heights = vec![0.0_f32; children.len().div_ceil(columns)];
    if rules.arrangement == Arrangement::Grid {
        for (n, child) in children.iter().enumerate() {
            widths[n % columns] = widths[n % columns].max(items[*child].size.x);
            heights[n / columns] = heights[n / columns].max(items[*child].size.y);
        }
        for lengths in [&mut widths, &mut heights] {
            let mut offset = rules.padding;
            for length in lengths {
                let next = offset + *length + rules.gap;
                *length = offset;
                offset = next;
            }
        }
    }
    for (n, child) in children.into_iter().enumerate() {
        let size = items[child].size;
        let offset = match rules.arrangement {
            Arrangement::Free => items[child].offset.max(Vec2::splat(rules.padding)),
            Arrangement::Row => {
                let offset = cursor;
                cursor.x += size.x + rules.gap;
                offset
            }
            Arrangement::Column => {
                let offset = cursor;
                cursor.y += size.y + rules.gap;
                offset
            }
            Arrangement::Grid => Vec2::new(widths[n % columns], heights[n / columns]),
        };
        items[child].offset = offset;
        extent = extent.max(offset + size + Vec2::splat(rules.padding));
    }
    extent.max(Vec2::splat(rules.padding * 2.0))
}

pub(super) fn solve(items: &mut [Item], order: &[usize]) {
    for &index in order.iter().rev() {
        let content = arrange(items, index);
        items[index].content = content;
        for axis in 0..2 {
            let rule = items[index].rules.axes[axis];
            let available = if items[index].parent.is_none() {
                items[index].available[axis]
            } else {
                rule.min
            };
            items[index].size[axis] = rule.resolve(content[axis], available);
        }
        keep_shape(&mut items[index]);
    }
    for &index in order {
        let rules = items[index].rules;
        let children = items[index].children.clone();
        let available = (items[index].size - Vec2::splat(rules.padding * 2.0)).max(Vec2::ZERO);
        let main_axis = match rules.arrangement {
            Arrangement::Row => Some(0),
            Arrangement::Column => Some(1),
            _ => None,
        };
        for axis in 0..2 {
            let filling = children
                .iter()
                .filter(|child| items[**child].rules.axes[axis].sizing == Sizing::Fill)
                .count();
            let fixed: f32 = children
                .iter()
                .filter(|child| items[**child].rules.axes[axis].sizing != Sizing::Fill)
                .map(|child| items[*child].size[axis])
                .sum();
            for child in &children {
                let rule = items[*child].rules.axes[axis];
                if rule.sizing == Sizing::Fill {
                    let space = if rules.axes[axis].sizing == Sizing::Fit {
                        rule.min
                    } else if main_axis == Some(axis) {
                        ((available[axis]
                            - fixed
                            - rules.gap * children.len().saturating_sub(1) as f32)
                            / filling.max(1) as f32)
                            .max(0.0)
                    } else if rules.arrangement == Arrangement::Grid {
                        let count = if axis == 0 {
                            usize::from(rules.columns)
                        } else {
                            children.len().div_ceil(usize::from(rules.columns)).max(1)
                        };
                        ((available[axis] - rules.gap * count.saturating_sub(1) as f32)
                            / count as f32)
                            .max(0.0)
                    } else if rules.arrangement == Arrangement::Free {
                        (items[index].size[axis] - items[*child].offset[axis] - rules.padding)
                            .max(0.0)
                    } else {
                        available[axis]
                    };
                    items[*child].size[axis] = rule.resolve(0.0, space);
                }
            }
        }
        for child in &children {
            keep_shape(&mut items[*child]);
        }
        items[index].content = arrange(items, index);
    }
    for &index in order.iter().rev() {
        let content = arrange(items, index);
        items[index].content = content;
        for axis in 0..2 {
            let rule = items[index].rules.axes[axis];
            if rule.sizing == Sizing::Fit {
                items[index].size[axis] = rule.resolve(content[axis], rule.min);
            }
        }
        keep_shape(&mut items[index]);
    }
}
