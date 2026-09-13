use super::*;
use crate::{area::AreaShape, icons::Tooltip, tokens::Token};
use bevy::math::DVec2;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap};

pub const PROPERTIES: [&str; 10] = [
    "head",
    "slug",
    "kind",
    "quantity_exact",
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Category {
    key: String,
    title: String,
    missing: bool,
}

fn category(row: &Value, axis: Option<&GroupAxis>) -> Category {
    let Some(axis) = axis else {
        return Category {
            key: String::new(),
            title: String::new(),
            missing: false,
        };
    };
    let value = &row[&axis.property];
    let (key, title) = if axis.property == "assignees" {
        let mut people: Vec<_> = value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|person| {
                Some((
                    person["uid"].as_str()?.to_string(),
                    person["head"].as_str().unwrap_or("Person").to_string(),
                ))
            })
            .collect();
        people.sort_by(|a, b| a.0.cmp(&b.0));
        people.dedup_by(|a, b| a.0 == b.0);
        let key = serde_json::to_string(&people.iter().map(|p| &p.0).collect::<Vec<_>>()).unwrap();
        people.sort_by(|a, b| {
            a.1.to_lowercase()
                .cmp(&b.1.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        (
            key,
            people
                .iter()
                .map(|p| p.1.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )
    } else if axis.property == "assertions" {
        let mut assertions: Vec<_> = value
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                serde_json::json!([a["predicate_uid"], a["object"], a["quantity"], a["unit"]])
                    .to_string()
            })
            .collect();
        assertions.sort();
        assertions.dedup();
        let mut titles: Vec<_> = value
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| rows::display(&Value::Array(vec![a.clone()])))
            .collect();
        titles.sort();
        titles.dedup();
        (
            serde_json::to_string(&assertions).unwrap(),
            titles.join(", "),
        )
    } else {
        let title = rows::display(value);
        (title.clone(), title)
    };
    let missing = value.is_null() || title.trim().is_empty();
    Category {
        key: if missing {
            "null".into()
        } else {
            format!("value:{key}")
        },
        title: if missing { "Unset".into() } else { title },
        missing,
    }
}

fn decimal_order(a: &str, b: &str) -> Ordering {
    let negative_a = a.starts_with('-');
    let negative_b = b.starts_with('-');
    let parts = |value: &str| {
        let (integer, fraction) = value
            .trim_start_matches('-')
            .split_once('.')
            .unwrap_or((value.trim_start_matches('-'), ""));
        (
            integer.trim_start_matches('0').to_string(),
            fraction.trim_end_matches('0').to_string(),
        )
    };
    let a = parts(a);
    let b = parts(b);
    let negative_a = negative_a && (!a.0.is_empty() || !a.1.is_empty());
    let negative_b = negative_b && (!b.0.is_empty() || !b.1.is_empty());
    let order =
        a.0.len()
            .cmp(&b.0.len())
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1));
    match (negative_a, negative_b) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => order.reverse(),
        _ => order,
    }
}

fn compare(a: &Category, b: &Category, axis: Option<&GroupAxis>) -> Ordering {
    let Some(axis) = axis else {
        return Ordering::Equal;
    };
    if a.missing != b.missing {
        return a.missing.cmp(&b.missing);
    }
    let order = if matches!(
        axis.property.as_str(),
        "quantity_exact" | "estimate_min" | "spent_seconds"
    ) {
        decimal_order(&a.title, &b.title)
    } else {
        a.title.to_lowercase().cmp(&b.title.to_lowercase())
    }
    .then(a.key.cmp(&b.key));
    if axis.descending {
        order.reverse()
    } else {
        order
    }
}

pub(super) fn page(data: &[Value], config: &Config, page: usize) -> Vec<Value> {
    if !config.grouping.active() {
        return data.iter().skip(page * 200).take(200).cloned().collect();
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
    indices.sort_by(|a, b| {
        compare(&a.2, &b.2, config.grouping.vertical.as_ref())
            .then_with(|| compare(&a.1, &b.1, config.grouping.horizontal.as_ref()))
            .then(a.0.cmp(&b.0))
    });
    indices
        .into_iter()
        .skip(page * 200)
        .take(200)
        .map(|(index, _, _)| data[index].clone())
        .collect()
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
    state: &mut State,
    rows: &[Value],
    config: &Config,
) -> HashMap<String, Cell> {
    let mut axes = [Vec::new(), Vec::new()];
    for (horizontal, axis) in [
        (true, config.grouping.horizontal.as_ref()),
        (false, config.grouping.vertical.as_ref()),
    ] {
        let mut categories: BTreeMap<String, Category> = rows
            .iter()
            .map(|row| {
                let category = category(row, axis);
                (category.key.clone(), category)
            })
            .collect();
        let values = &mut axes[usize::from(!horizontal)];
        values.extend(std::mem::take(&mut categories).into_values());
        values.sort_by(|a, b| compare(a, b, axis));
        if axis.is_none() {
            continue;
        }
        for (index, category) in values.iter().enumerate() {
            let key = (horizontal, category.key.clone());
            let existing = state
                .groups
                .get(&key)
                .copied()
                .filter(|e| world.get_entity(*e).is_ok());
            let entity = existing.unwrap_or_else(|| {
                let root = world.get::<ChildOf>(owner).unwrap().parent();
                let workspace = world
                    .get::<crate::workspace::WorkspaceMember>(owner)
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
                let entity = crate::area::spawn_area(world, root, workspace, area).unwrap();
                world.entity_mut(entity).insert((
                    Node {
                        border: UiRect::all(px(1)),
                        overflow: Overflow::visible(),
                        ..default()
                    },
                    crate::token_style::border(Token::Accent),
                    Tooltip(category.title.clone()),
                ));
                let label = crate::edit_mode::label(world, entity, &category.title, 16.0);
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
                    if let Some(mut text) = world.get_mut::<Text>(child) {
                        text.0 = category.title.clone();
                    }
                }
            }
            world.entity_mut(entity).insert((
                GeneratedGroup {
                    owner,
                    horizontal,
                    index,
                    targets: HashMap::new(),
                },
                Tooltip(category.title.clone()),
            ));
            let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
            area.name = category.title.trim().chars().take(80).collect();
            area.strength = config.grouping.strength;
            state.groups.insert(key, entity);
        }
    }
    let stale: Vec<_> = state
        .groups
        .keys()
        .filter(|(horizontal, key)| {
            let axis = if *horizontal {
                &config.grouping.horizontal
            } else {
                &config.grouping.vertical
            };
            axis.is_none()
                || !axes[usize::from(!*horizontal)]
                    .iter()
                    .any(|c| &c.key == key)
        })
        .cloned()
        .collect();
    for key in stale {
        if let Some(entity) = state.groups.remove(&key) {
            let _ = world.despawn(entity);
        }
    }
    let mut counts = HashMap::new();
    rows.iter()
        .filter_map(|row| {
            let uid = row["uid"].as_str()?;
            let horizontal = category(row, config.grouping.horizontal.as_ref());
            let vertical = category(row, config.grouping.vertical.as_ref());
            let x = axes[0]
                .iter()
                .position(|c| c.key == horizontal.key)
                .unwrap_or(0);
            let y = axes[1]
                .iter()
                .position(|c| c.key == vertical.key)
                .unwrap_or(0);
            let slot = counts.entry((x, y)).or_insert(0);
            let cell = Cell { x, y, slot: *slot };
            *slot += 1;
            Some((uid.to_string(), cell))
        })
        .collect()
}

pub(super) fn layout(
    world: &mut World,
    owner: Entity,
    config: &Config,
    rows: &[(Entity, Cell, f32)],
) {
    let Some(area) = world.get::<InfluenceArea>(owner) else {
        return;
    };
    let origin = DVec2::from_array(area.center) - DVec2::from_array(area.size) * 0.5;
    let reach = area.reach;
    let columns = config.columns;
    let gap = config.gap as f64;
    let cell_width = columns as f64 * (config.width as f64 + gap) - gap + 24.0;
    let mut heights = BTreeMap::<(usize, usize), f64>::new();
    for (_, cell, height) in rows {
        let value = heights.entry((cell.y, cell.slot / columns)).or_default();
        *value = value.max(f64::from(*height));
    }
    let mut band_heights = BTreeMap::<usize, f64>::new();
    for ((y, _), height) in &heights {
        *band_heights.entry(*y).or_insert(48.0) += height + gap;
    }
    let ys: BTreeMap<_, _> = band_heights
        .iter()
        .scan(0.0, |sum, (y, height)| {
            let start = *sum;
            *sum += height + 24.0;
            Some((*y, start))
        })
        .collect();
    let reverse_x = config
        .grouping
        .horizontal
        .as_ref()
        .is_some_and(|a| a.reverse);
    let reverse_y = config.grouping.vertical.as_ref().is_some_and(|a| a.reverse);
    let x_start = |x: usize| {
        if reverse_x {
            -(x as f64 + 1.0) * (cell_width + gap)
        } else {
            x as f64 * (cell_width + gap)
        }
    };
    let y_start = |y: usize| {
        if reverse_y {
            -ys[&y] - band_heights[&y]
        } else {
            ys[&y]
        }
    };
    let mut targets = HashMap::new();
    for (entity, cell, _) in rows {
        let line = cell.slot / columns;
        let height = heights[&(cell.y, line)];
        let y: f64 = heights
            .range((cell.y, 0)..(cell.y, line))
            .map(|(_, height)| height + gap)
            .sum();
        let position = origin
            + DVec2::new(
                180.0
                    + x_start(cell.x)
                    + 12.0
                    + (cell.slot % columns) as f64 * (config.width as f64 + gap)
                    + config.width as f64 / 2.0,
                48.0 + y_start(cell.y) + 40.0 + y + height / 2.0,
            );
        rows::place(
            world,
            *entity,
            position,
            Vec2::new(config.width, height as f32),
        );
        targets.insert(*entity, position);
    }
    let nx = rows.iter().map(|(_, c, _)| c.x).max().map_or(0, |x| x + 1);
    let min_x = if reverse_x {
        -(nx as f64) * (cell_width + gap)
    } else {
        0.0
    };
    let total_width = (nx as f64 * (cell_width + gap) - gap).max(1.0);
    let total_height =
        band_heights.values().sum::<f64>() + band_heights.len().saturating_sub(1) as f64 * 24.0;
    let min_y = if reverse_y { -total_height } else { 0.0 };
    let groups: Vec<_> = world
        .query::<(Entity, &GeneratedGroup)>()
        .iter(world)
        .filter(|(_, g)| g.owner == owner)
        .map(|(e, g)| (e, g.clone()))
        .collect();
    for (entity, mut group) in groups {
        if rows.is_empty() {
            continue;
        }
        let (start, size) = if group.horizontal {
            (
                DVec2::new(180.0 + x_start(group.index), min_y),
                DVec2::new(cell_width, total_height + 48.0),
            )
        } else {
            (
                DVec2::new(min_x, 48.0 + y_start(group.index)),
                DVec2::new(total_width + 180.0, band_heights[&group.index]),
            )
        };
        let center = origin + start + size * 0.5;
        let owner_placement = crate::topology::spatial(world, owner);
        let owner_center = world.get::<InfluenceArea>(owner).map_or(DVec2::ZERO, |area| DVec2::from_array(area.center));
        let local = center - owner_center;
        let point = owner_placement.position(owner_center) + owner_placement.rotation() * bevy::math::DVec3::new(local.x, 0.0, local.y);
        let world_center = DVec2::new(point.x, point.z);
        world.entity_mut(entity).insert(crate::topology::Spatial { elevation: point.y, rotation: owner_placement.rotation, ..default() });
        if let Some(mut area) = world.get_mut::<InfluenceArea>(entity) {
            if area.center != world_center.to_array()
                || area.size != size.to_array()
                || area.reach != reach
            {
                area.center = world_center.to_array();
                area.size = size.to_array();
                area.reach = reach;
            }
        }
        group.targets = rows
            .iter()
            .filter(|(_, cell, _)| {
                if group.horizontal {
                    cell.x == group.index
                } else {
                    cell.y == group.index
                }
            })
            .map(|(e, _, _)| (*e, world_center + targets[e] - center))
            .collect();
        if world.get::<GeneratedGroup>(entity) != Some(&group) {
            world.entity_mut(entity).insert(group);
        }
    }
}
