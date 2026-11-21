// non-lean-yet
// Scope: mod layout, const PROPERTIES, struct GroupAxis, struct Grouping.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 107, 142, 331,
//   361, 408.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   91, 242, 247, 249, 320, 324, 435.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (106, 137, 337, 340, 352, 353, 410).

mod categories;
mod layout;
pub(super) use layout::Layout;

use super::*;
use crate::native::{area::AreaShape, icons::Tooltip, tokens::Token};
use bevy::math::DVec2;
use categories::{Category, category, compare};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROPERTIES: [&str; 10] = [
    "head",
    "slug",
    "kind",
    "quantity",
    "assignees",
    "assertions",
    "start_date",
    "due_date",
    "estimate_min",
    "spent_seconds",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupAxis {
    pub property: String,
    pub descending: bool,
    pub reverse: bool,
}

impl GroupAxis {
    pub fn new(property: &str) -> Self {
        Self {
            property: property.into(),
            descending: false,
            reverse: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grouping {
    pub horizontal: Option<GroupAxis>,
    pub vertical: Option<GroupAxis>,
    pub strength: f64,
}

impl Default for Grouping {
    fn default() -> Self {
        Self {
            horizontal: None,
            vertical: None,
            strength: 100.0,
        }
    }
}

impl Grouping {
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        [&self.horizontal, &self.vertical]
            .into_iter()
            .flatten()
            .map(|axis| axis.property.as_str())
    }
    pub fn active(&self) -> bool {
        self.horizontal.is_some() || self.vertical.is_some()
    }
    pub fn valid(&self) -> bool {
        self.fields().all(|key| PROPERTIES.contains(&key))
            && self.strength.is_finite()
            && (0.0..=1_000_000.0).contains(&self.strength)
    }
}

pub(super) fn page<'a>(data: &'a [Value], config: &Config, page: usize) -> Vec<&'a Value> {
    let start = page.saturating_mul(200);
    if start >= data.len() {
        return Vec::new();
    }
    if !config.grouping.active() {
        return data.iter().skip(start).take(200).collect();
    }
    let mut indices: Vec<_> = data
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                index,
                category(row, config.grouping.horizontal.as_ref()),
                category(row, config.grouping.vertical.as_ref()),
            )
        })
        .collect();
    let order = |a: &(usize, Category, Category), b: &(usize, Category, Category)| {
        compare(&a.2, &b.2, config.grouping.vertical.as_ref())
            .then_with(|| compare(&a.1, &b.1, config.grouping.horizontal.as_ref()))
            .then(a.0.cmp(&b.0))
    };
    if start > 0 {
        indices.select_nth_unstable_by(start, order);
    }
    let remaining = &mut indices[start..];
    let count = remaining.len().min(200);
    if count < remaining.len() {
        remaining.select_nth_unstable_by(count, order);
    }
    let visible = &mut remaining[..count];
    visible.sort_unstable_by(order);
    visible.iter().map(|(index, _, _)| &data[*index]).collect()
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub x: usize,
    pub y: usize,
    pub slot: usize,
}

#[derive(Component, Clone, PartialEq)]
pub(crate) struct GeneratedGroup {
    pub owner: Entity,
    pub horizontal: bool,
    pub(crate) index: usize,
    pub targets: HashMap<Entity, DVec2>,
}

impl GeneratedGroup {
    pub(crate) fn force(&self, area: &InfluenceArea, sand: Entity, point: DVec2) -> DVec2 {
        let Some(target) = self.targets.get(&sand) else {
            return DVec2::ZERO;
        };
        if !area.reaches(point) {
            return DVec2::ZERO;
        }
        let delta = *target - point;
        let distance = if self.horizontal { delta.x } else { delta.y };
        let force = if distance.abs() < 0.5 {
            0.0
        } else {
            distance.clamp(-1.0, 1.0) * area.strength
        };
        if self.horizontal {
            DVec2::new(force, 0.0)
        } else {
            DVec2::new(0.0, force)
        }
    }
}

pub(super) fn prepare(
    world: &mut World,
    owner: Entity,
    groups: &mut HashMap<(bool, String), Entity>,
    rows: &[&Value],
    config: &Config,
) -> HashMap<String, Cell> {
    if !config.grouping.active() {
        for (_, entity) in groups.drain() {
            let _ = world.despawn(entity);
        }
        return HashMap::new();
    }
    let categories: Vec<_> = rows
        .iter()
        .map(|row| {
            [
                category(row, config.grouping.horizontal.as_ref()),
                category(row, config.grouping.vertical.as_ref()),
            ]
        })
        .collect();
    let mut axes = [Vec::new(), Vec::new()];
    for (horizontal, axis) in [
        (true, config.grouping.horizontal.as_ref()),
        (false, config.grouping.vertical.as_ref()),
    ] {
        let unique: BTreeMap<&str, &Category> = categories
            .iter()
            .map(|categories| {
                let category = &categories[usize::from(!horizontal)];
                (category.key.as_str(), category)
            })
            .collect();
        let values = &mut axes[usize::from(!horizontal)];
        values.extend(unique.into_values());
        values.sort_by(|a, b| compare(a, b, axis));
        if axis.is_none() {
            continue;
        }
        for (index, category) in values.iter().enumerate() {
            let key = (horizontal, category.key.clone());
            let existing = groups
                .get(&key)
                .copied()
                .filter(|e| world.get_entity(*e).is_ok());
            let entity = existing.unwrap_or_else(|| {
                let root = world.get::<ChildOf>(owner).unwrap().parent();
                let workspace = world
                    .get::<crate::native::workspace::WorkspaceMember>(owner)
                    .unwrap()
                    .0;
                let shape = AreaShape::Polygon(vec![
                    [-0.5, -0.5],
                    [0.5, -0.5],
                    [0.5, 0.5],
                    [-0.5, 0.5],
                    [-0.5, -0.5],
                ]);
                let mut area = InfluenceArea::new(shape, DVec2::ZERO, DVec2::splat(100.0));
                area.name = category.title.trim().chars().take(80).collect();
                area.strength = config.grouping.strength;
                area.reach = world.get::<InfluenceArea>(owner).unwrap().reach;
                let entity = crate::native::area::spawn_area(world, root, workspace, area).unwrap();
                world.entity_mut(entity).insert((
                    Node {
                        border: UiRect::all(px(1)),
                        overflow: Overflow::visible(),
                        ..default()
                    },
                    crate::native::token_style::border(Token::Accent),
                    Tooltip(category.title.clone()),
                ));
                let label = crate::native::edit_mode::label(world, entity, &category.title, 16.0);
                world.entity_mut(label).insert((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(8),
                        top: px(8),
                        width: if horizontal { percent(95) } else { px(156) },
                        max_height: px(32),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
                entity
            });
            if let Some(children) = world.get::<Children>(entity) {
                let children: Vec<_> = children.iter().collect();
                for child in children {
                    if let Some(mut text) = world.get_mut::<Text>(child)
                        && text.0 != category.title
                    {
                        text.0.clone_from(&category.title);
                    }
                }
            }
            if let Some(mut group) = world.get_mut::<GeneratedGroup>(entity) {
                if group.owner != owner || group.horizontal != horizontal || group.index != index {
                    group.owner = owner;
                    group.horizontal = horizontal;
                    group.index = index;
                }
            } else {
                world.entity_mut(entity).insert(GeneratedGroup {
                    owner,
                    horizontal,
                    index,
                    targets: HashMap::new(),
                });
            }
            if world
                .get::<Tooltip>(entity)
                .is_none_or(|tooltip| tooltip.0 != category.title)
            {
                world
                    .entity_mut(entity)
                    .insert(Tooltip(category.title.clone()));
            }
            let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
            let name: String = category.title.trim().chars().take(80).collect();
            if area.name != name {
                area.name = name;
            }
            if area.strength != config.grouping.strength {
                area.strength = config.grouping.strength;
            }
            groups.insert(key, entity);
        }
    }
    let positions = axes.each_ref().map(|axis| {
        axis.iter()
            .enumerate()
            .map(|(index, category)| (category.key.as_str(), index))
            .collect::<HashMap<_, _>>()
    });
    let stale: Vec<_> = groups
        .keys()
        .filter(|(horizontal, key)| {
            let axis = if *horizontal {
                &config.grouping.horizontal
            } else {
                &config.grouping.vertical
            };
            axis.is_none() || !positions[usize::from(!*horizontal)].contains_key(key.as_str())
        })
        .cloned()
        .collect();
    for key in stale {
        if let Some(entity) = groups.remove(&key) {
            let _ = world.despawn(entity);
        }
    }
    let mut counts = HashMap::new();
    rows.iter()
        .zip(&categories)
        .filter_map(|(row, categories)| {
            let uid = row["uid"].as_str()?;
            let x = positions[0]
                .get(categories[0].key.as_str())
                .copied()
                .unwrap_or(0);
            let y = positions[1]
                .get(categories[1].key.as_str())
                .copied()
                .unwrap_or(0);
            let slot = counts.entry((x, y)).or_insert(0);
            let cell = Cell { x, y, slot: *slot };
            *slot += 1;
            Some((uid.to_string(), cell))
        })
        .collect()
}
