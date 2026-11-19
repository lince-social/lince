use super::*;
use crate::host::performance::{self, Count, Stage};

#[derive(Default)]
struct Line {
    height: f64,
    offset: f64,
    current: bool,
}
struct Band {
    index: usize,
    height: f64,
    offset: f64,
}

pub(in crate::native::protein_area) struct Layout {
    query: QueryState<(Entity, &'static GeneratedGroup)>,
    groups: Vec<(Entity, bool, usize)>,
    lines: BTreeMap<(usize, usize), Line>,
    bands: Vec<Band>,
    targets: [HashMap<usize, Vec<(Entity, DVec2)>>; 2],
    used: bool,
}

impl FromWorld for Layout {
    fn from_world(world: &mut World) -> Self {
        Self {
            query: world.query(),
            groups: Vec::new(),
            lines: BTreeMap::new(),
            bands: Vec::new(),
            targets: default(),
            used: false,
        }
    }
}

impl Layout {
    pub(in crate::native::protein_area) fn begin_frame(&mut self) {
        self.used = false;
    }
    pub(in crate::native::protein_area) fn finish_frame(&mut self) {
        if !self.used {
            self.groups.shrink_to_fit();
            self.lines.clear();
            self.bands.shrink_to_fit();
            for targets in &mut self.targets {
                targets.clear();
                targets.shrink_to_fit();
            }
        }
    }
    fn prepare(&mut self, config: &Config, rows: &[(Entity, Cell, f32)]) {
        let _prepare = performance::Span::new(Stage::RecordGroupPrepare);
        performance::add(Count::RecordGroupRowsPrepared, rows.len() as u64);
        for (_, cell, height) in rows {
            let line = self
                .lines
                .entry((cell.y, cell.slot / config.columns))
                .or_default();
            line.height = line.height.max(f64::from(*height));
            line.current = true;
        }
        self.lines.retain(|_, line| line.current);
        let gap = config.gap as f64;
        let mut offset = 0.0;
        for (&(index, _), line) in &mut self.lines {
            if self.bands.last().is_none_or(|band| band.index != index) {
                self.bands.push(Band {
                    index,
                    height: 48.0,
                    offset: 0.0,
                });
                offset = 0.0;
            }
            line.offset = offset;
            offset += line.height + gap;
            self.bands.last_mut().unwrap().height += line.height + gap;
        }
        let mut offset = 0.0;
        for band in &mut self.bands {
            band.offset = offset;
            offset += band.height + 24.0;
        }
    }
    fn clear_pass(&mut self) {
        for line in self.lines.values_mut() {
            *line = Line::default();
        }
        self.bands.clear();
        for targets in &mut self.targets {
            targets.retain(|_, rows| !rows.is_empty());
            for rows in targets.values_mut() {
                rows.clear();
            }
        }
    }

    pub(in crate::native::protein_area) fn apply(
        &mut self,
        world: &mut World,
        owner: Entity,
        config: &Config,
        rows: &[(Entity, Cell, f32)],
    ) {
        let _layout = performance::Span::new(Stage::RecordGroupLayout);
        let Some(area) = world.get::<InfluenceArea>(owner) else {
            return;
        };
        self.used = true;
        let origin = DVec2::from_array(area.center) - DVec2::from_array(area.size) * 0.5;
        let reach = area.reach;
        let columns = config.columns;
        let gap = config.gap as f64;
        let cell_width = columns as f64 * (config.width as f64 + gap) - gap + 24.0;
        self.prepare(config, rows);
        let bands = &self.bands;
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
            let band = &bands[bands.binary_search_by_key(&y, |band| band.index).unwrap()];
            if reverse_y {
                -band.offset - band.height
            } else {
                band.offset
            }
        };
        let _apply = performance::Span::new(Stage::RecordGroupApply);
        for (entity, cell, _) in rows {
            let line = cell.slot / columns;
            let line = &self.lines[&(cell.y, line)];
            let height = line.height;
            let y = line.offset;
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
            self.targets[0]
                .entry(cell.x)
                .or_default()
                .push((*entity, position));
            self.targets[1]
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
        let total_height = bands.iter().map(|band| band.height).sum::<f64>()
            + bands.len().saturating_sub(1) as f64 * 24.0;
        let min_y = if reverse_y { -total_height } else { 0.0 };
        self.groups.extend(
            self.query
                .iter(world)
                .filter(|(_, group)| group.owner == owner)
                .map(|(entity, group)| (entity, group.horizontal, group.index)),
        );
        let owner_placement = crate::native::topology::spatial(world, owner);
        let owner_center = world
            .get::<InfluenceArea>(owner)
            .map_or(DVec2::ZERO, |area| DVec2::from_array(area.center));
        for (entity, horizontal, index) in self.groups.drain(..) {
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
                    DVec2::new(
                        total_width + 180.0,
                        bands[bands
                            .binary_search_by_key(&index, |band| band.index)
                            .unwrap()]
                        .height,
                    ),
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
            let targets = self.targets[usize::from(!horizontal)]
                .get(&index)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let unchanged = world.get::<GeneratedGroup>(entity).is_some_and(|group| {
                group.owner == owner
                    && group.horizontal == horizontal
                    && group.index == index
                    && group.targets.len() == targets.len()
                    && targets.iter().all(|(entity, position)| {
                        group.targets.get(entity) == Some(&(world_center + *position - center))
                    })
            });
            if !unchanged {
                performance::count(Count::RecordGroupTargetMaps);
                performance::add(Count::RecordGroupTargetsCopied, targets.len() as u64);
                world.entity_mut(entity).insert(GeneratedGroup {
                    owner,
                    horizontal,
                    index,
                    targets: targets
                        .iter()
                        .map(|(entity, position)| (*entity, world_center + *position - center))
                        .collect(),
                });
            }
        }
        self.clear_pass();
    }
}
