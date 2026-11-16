use super::*;
use crate::native::{area::AreaShape, icons::Tooltip, tokens::Token};
use bevy::math::DVec2;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap};

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Category {
    key: String,
    title: String,
    order: String,
    missing: bool,
}

fn category(row: &Value, axis: Option<&GroupAxis>) -> Category {
    let Some(axis) = axis else {
        return Category {
            key: String::new(),
            title: String::new(),
            order: String::new(),
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
        people.sort_by_cached_key(|(uid, name)| (name.to_lowercase(), uid.clone()));
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
            .map(|a| lince_interface::records::display_list(std::slice::from_ref(a)))
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
    let order = if missing {
        "unset".into()
    } else {
        title.to_lowercase()
    };
    Category {
        key: if missing {
            "null".into()
        } else {
            format!("value:{key}")
        },
        title: if missing { "Unset".into() } else { title },
        order,
        missing,
    }
}

fn decimal_order(a: &str, b: &str) -> Ordering {
    let negative_a = a.starts_with('-');
    let negative_b = b.starts_with('-');
    fn parts(value: &str) -> (&str, &str) {
        let (integer, fraction) = value
            .trim_start_matches('-')
            .split_once('.')
            .unwrap_or((value.trim_start_matches('-'), ""));
        (
            integer.trim_start_matches('0'),
            fraction.trim_end_matches('0'),
        )
    }
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
        "quantity" | "estimate_min" | "spent_seconds"
    ) {
        decimal_order(&a.title, &b.title)
    } else {
        a.order.cmp(&b.order)
    }
    .then(a.key.cmp(&b.key));
    if axis.descending {
        order.reverse()
    } else {
        order
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
    // Only the displayed page needs a full ordering. The source index keeps
    // equal categories in their original order through both partitions.
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
    let mut line_offsets = BTreeMap::new();
    let mut previous_band = None;
    let mut offset = 0.0;
    for (&(y, line), height) in &heights {
        if previous_band != Some(y) {
            previous_band = Some(y);
            offset = 0.0;
        }
        line_offsets.insert((y, line), offset);
        offset += height + gap;
        *band_heights.entry(y).or_insert(48.0) += height + gap;
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
    let mut targets: [HashMap<usize, Vec<(Entity, DVec2)>>; 2] = default();
    for (entity, cell, _) in rows {
        let line = cell.slot / columns;
        let height = heights[&(cell.y, line)];
        let y = line_offsets[&(cell.y, line)];
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
        targets[0]
            .entry(cell.x)
            .or_default()
            .push((*entity, position));
        targets[1]
            .entry(cell.y)
            .or_default()
            .push((*entity, position));
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
        .map(|(e, g)| (e, g.horizontal, g.index))
        .collect();
    let owner_placement = crate::native::topology::spatial(world, owner);
    let owner_center = world
        .get::<InfluenceArea>(owner)
        .map_or(DVec2::ZERO, |area| DVec2::from_array(area.center));
    for (entity, horizontal, index) in groups {
        if rows.is_empty() {
            continue;
        }
        let (start, size) = if horizontal {
            (
                DVec2::new(180.0 + x_start(index), min_y),
                DVec2::new(cell_width, total_height + 48.0),
            )
        } else {
            (
                DVec2::new(min_x, 48.0 + y_start(index)),
                DVec2::new(total_width + 180.0, band_heights[&index]),
            )
        };
        let center = origin + start + size * 0.5;
        let local = center - owner_center;
        let point = owner_placement.position(owner_center)
            + owner_placement.rotation() * bevy::math::DVec3::new(local.x, 0.0, local.y);
        let world_center = DVec2::new(point.x, point.z);
        let placement = crate::native::topology::Spatial {
            elevation: point.y,
            rotation: owner_placement.rotation,
            ..default()
        };
        if world.get::<crate::native::topology::Spatial>(entity) != Some(&placement) {
            world.entity_mut(entity).insert(placement);
        }
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
        let group = GeneratedGroup {
            owner,
            horizontal,
            index,
            targets: targets[usize::from(!horizontal)]
                .get(&index)
                .into_iter()
                .flatten()
                .map(|(entity, position)| (*entity, world_center + *position - center))
                .collect(),
        };
        if world.get::<GeneratedGroup>(entity) != Some(&group) {
            world.entity_mut(entity).insert(group);
        }
    }
}
