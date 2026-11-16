use super::{
    controls::Controls,
    data::*,
    dungeon::Dungeon,
    fracture,
    objects::{Mount, Thing, angle_delta, rotate},
};
use bevy::prelude::*;

pub(super) struct Run {
    pub dungeon: Dungeon,
    pub player: Vec2,
    pub things: Vec<Thing>,
    pub level: u64,
    pub geometry_revision: u64,
    pub catalog: &'static Catalog,
    pub paused: bool,
    random: Random,
    next_id: u64,
    accumulated: f32,
}

impl Run {
    pub fn new(seed: u64, catalog: &'static Catalog) -> Self {
        let mut random = Random::new(seed);
        let dungeon = Dungeon::generate(&mut random);
        let mut run = Self {
            player: dungeon.entrance,
            dungeon,
            things: Vec::new(),
            level: 1,
            geometry_revision: 1,
            catalog,
            paused: false,
            random,
            next_id: 1,
            accumulated: 0.0,
        };
        run.populate();
        run
    }

    pub fn suspend(&mut self) {
        self.accumulated = 0.0;
    }

    pub fn advance(&mut self, elapsed: f32, input: &mut Controls) {
        if self.paused {
            input.clear();
            self.suspend();
            return;
        }
        let aim = input.pointer.unwrap_or(self.player + Vec2::X * 30.0) - self.player;
        if std::mem::take(&mut input.grab) {
            self.grab(aim.normalize_or(Vec2::X));
        }
        if std::mem::take(&mut input.release) {
            self.release(aim.normalize_or(Vec2::X));
        }
        self.accumulated = (self.accumulated + elapsed.max(0.0)).min(STEP * MAX_STEPS as f32);
        while self.accumulated >= STEP {
            self.accumulated -= STEP;
            self.step(input);
        }
    }

    fn step(&mut self, input: &Controls) {
        let previous_player = self.player;
        let movement = input.movement() * PLAYER_SPEED * STEP;
        for axis in [Vec2::new(movement.x, 0.0), Vec2::new(0.0, movement.y)] {
            let next = self.dungeon.move_circle(self.player, axis, PLAYER_RADIUS);
            let translation = next - previous_player;
            let fits = self
                .things
                .iter()
                .filter(|thing| thing.mount.is_some())
                .all(|thing| {
                    let size = self.catalog.material(thing.kind).part_size;
                    thing.parts.iter().all(|part| {
                        self.dungeon
                            .clear_circle(thing.point(part.cell, size) + translation, size * 0.7)
                    })
                });
            if fits {
                self.player = next;
            }
        }
        if self.dungeon.at_exit(self.player) {
            self.next_level();
            return;
        }
        let player_translation = self.player - previous_player;
        let aim = input.pointer.unwrap_or(self.player + Vec2::X * 30.0) - self.player;
        let direction = aim.normalize_or(Vec2::X);
        for thing in &mut self.things {
            thing.previous_position = thing.position;
            thing.previous_angle = thing.angle;
            let previous_thrust = thing.thrust;
            thing.impact_cooldown = (thing.impact_cooldown - STEP).max(0.0);
            let material = self.catalog.material(thing.kind);
            if let Some(mount) = thing.mount {
                let activation = if input.activating && material.responsive {
                    ((mount.direction.dot(direction) - 0.15) / 0.85)
                        .clamp(0.0, 1.0)
                        .powi(2)
                } else {
                    0.0
                };
                let rest_angle = mount.direction.to_angle();
                let desired = if activation > 0.0 {
                    direction.to_angle()
                } else {
                    rest_angle
                };
                let angular_step = if activation > 0.0 {
                    9.0 * activation
                } else {
                    4.0
                } * STEP;
                thing.angle += angle_delta(thing.angle, desired).clamp(-angular_step, angular_step);
                let thrust = ((aim.length() - 12.0) * 0.55).clamp(0.0, 16.0) * activation;
                thing.thrust += (thrust - thing.thrust).clamp(-85.0 * STEP, 85.0 * STEP);
                let grip = self.player + mount.direction * (PLAYER_RADIUS + material.part_size);
                thing.position = grip + Vec2::from_angle(thing.angle) * thing.thrust
                    - rotate(mount.anchor.as_vec2() * material.part_size, thing.angle);
                if activation > 0.0 {
                    let motion = angle_delta(thing.previous_angle, thing.angle).abs() * 8.0
                        + thing.position.distance(thing.previous_position);
                    if let Some(part) = thing
                        .parts
                        .iter_mut()
                        .find(|part| part.cell == mount.anchor)
                    {
                        part.integrity -= motion * material.wear * 0.018;
                        thing.scarred |= part.integrity <= 0.0;
                    }
                }
            } else {
                thing.position += thing.velocity.clamp_length_max(130.0) * STEP;
                thing.angle += thing.spin.clamp(-8.0, 8.0) * STEP;
                thing.velocity *= (-3.5 * STEP).exp();
                thing.spin *= (-4.0 * STEP).exp();
                if thing.velocity.length_squared() < 0.01 {
                    thing.velocity = Vec2::ZERO;
                }
            }
            let wall_hit = thing.parts.iter().enumerate().find_map(|(index, part)| {
                let from = thing.previous_point(part.cell, material.part_size);
                let to = thing.point(part.cell, material.part_size);
                let distance = from.distance(to);
                let samples = (distance / (material.part_size * 0.5))
                    .ceil()
                    .clamp(1.0, 64.0) as usize;
                (1..=samples)
                    .any(|step| {
                        !self.dungeon.clear_circle(
                            from.lerp(to, step as f32 / samples as f32),
                            material.part_size * 0.7,
                        )
                    })
                    .then_some((index, (to - from) / STEP))
            });
            if let Some((part, velocity)) = wall_hit {
                if velocity.length() > MIN_IMPACT_SPEED && thing.impact_cooldown <= 0.0 {
                    fracture::damage(
                        thing,
                        part,
                        velocity.length().min(400.0) * material.impact_transfer * 0.4,
                        -velocity,
                        self.catalog,
                    );
                    thing.impact_cooldown = 0.08;
                }
                thing.position = thing.previous_position
                    + if thing.mount.is_some() {
                        player_translation
                    } else {
                        Vec2::ZERO
                    };
                thing.angle = thing.previous_angle;
                thing.thrust = previous_thrust;
                thing.velocity *= -0.2;
                thing.spin *= -0.2;
            }
        }
        fracture::contacts(&mut self.things, self.catalog);
        if fracture::split(&mut self.things, &mut self.next_id, self.catalog) {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
    }

    fn grab(&mut self, direction: Vec2) {
        if self
            .things
            .iter()
            .filter(|thing| thing.mount.is_some())
            .count()
            >= MAX_ATTACHMENTS
        {
            return;
        }
        let mut choice = None;
        let mut best = f32::MAX;
        for (index, thing) in self
            .things
            .iter()
            .enumerate()
            .filter(|(_, thing)| thing.mount.is_none())
        {
            let size = self.catalog.material(thing.kind).part_size;
            for part in &thing.parts {
                let offset = thing.point(part.cell, size) - self.player;
                let alignment = offset.normalize_or(direction).dot(direction);
                let score = offset.length() + (1.0 - alignment) * GRAB_REACH;
                if offset.length() <= GRAB_REACH
                    && alignment > 0.55
                    && score < best
                    && self.line_clear(self.player, thing.point(part.cell, size))
                {
                    choice = Some((index, part.cell));
                    best = score;
                }
            }
        }
        if let Some((index, anchor)) = choice {
            let occupied = self
                .things
                .iter()
                .filter_map(|thing| thing.mount)
                .any(|mount| mount.direction.dot(direction) > 0.96);
            if occupied {
                return;
            }
            let thing = &mut self.things[index];
            let size = self.catalog.material(thing.kind).part_size;
            let angle = direction.to_angle();
            let position = self.player + direction * (PLAYER_RADIUS + size)
                - rotate(anchor.as_vec2() * size, angle);
            if !thing.parts.iter().all(|part| {
                self.dungeon.clear_circle(
                    position + rotate(part.cell.as_vec2() * size, angle),
                    size * 0.7,
                )
            }) {
                return;
            }
            thing.mount = Some(Mount { direction, anchor });
            thing.position = position;
            thing.angle = angle;
            thing.thrust = 0.0;
            thing.settle_motion();
        }
    }

    fn release(&mut self, direction: Vec2) {
        let choice = self
            .things
            .iter()
            .enumerate()
            .filter_map(|(index, thing)| {
                let alignment = thing.mount?.direction.dot(direction);
                (alignment > 0.25).then_some((index, alignment))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((index, _)) = choice {
            let thing = &mut self.things[index];
            thing.mount = None;
            thing.velocity = direction * 32.0;
            thing.spin = 0.0;
        }
    }

    fn line_clear(&self, from: Vec2, to: Vec2) -> bool {
        let samples = (from.distance(to) / (TILE * 0.25)).ceil().max(1.0) as usize;
        (0..=samples).all(|index| {
            self.dungeon
                .clear_circle(from.lerp(to, index as f32 / samples as f32), 0.5)
        })
    }

    fn next_level(&mut self) {
        self.dungeon = Dungeon::generate(&mut self.random);
        self.player = self.dungeon.entrance;
        self.level = self.level.wrapping_add(1);
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.things.retain(|thing| thing.mount.is_some());
        for thing in &mut self.things {
            let mount = thing.mount.unwrap();
            let size = self.catalog.material(thing.kind).part_size;
            thing.angle = mount.direction.to_angle();
            thing.thrust = 0.0;
            thing.position = self.player + mount.direction * (PLAYER_RADIUS + size)
                - rotate(mount.anchor.as_vec2() * size, thing.angle);
            thing.settle_motion();
        }
        self.populate();
    }

    fn populate(&mut self) {
        if self.things.is_empty() {
            self.spawn(Kind::Stick, self.player + Vec2::new(13.0, -8.0));
            self.spawn(Kind::Bush, self.player + Vec2::new(23.0, 18.0));
        }
        let weight = self.catalog.stick.spawn_weight + self.catalog.bush.spawn_weight;
        for _ in 0..110 {
            if self.things.len() >= 38 {
                break;
            }
            let cell = IVec2::new(
                self.random.index(WIDTH) as i32,
                self.random.index(HEIGHT) as i32,
            );
            let position = (cell.as_vec2() + Vec2::splat(0.5)) * TILE;
            if !self.dungeon.open(cell)
                || cell == self.dungeon.exit
                || position.distance(self.player) < 48.0
            {
                continue;
            }
            let kind =
                if self.random.index(weight as usize) < self.catalog.stick.spawn_weight as usize {
                    Kind::Stick
                } else {
                    Kind::Bush
                };
            self.spawn(kind, position);
        }
    }

    fn spawn(&mut self, kind: Kind, position: Vec2) {
        let mut thing = Thing::new(self.next_id, kind, position, &mut self.random, self.catalog);
        if position.distance(self.player) >= 48.0 {
            thing.angle = self
                .random
                .range(-std::f32::consts::PI, std::f32::consts::PI);
        }
        let size = self.catalog.material(kind).part_size;
        if thing.parts.iter().any(|part| {
            let point = thing.point(part.cell, size);
            !self.dungeon.clear_circle(point, size * 0.7)
                || self.dungeon.at_exit(point)
                || self.things.iter().any(|other| {
                    let other_size = self.catalog.material(other.kind).part_size;
                    other.parts.iter().any(|part| {
                        other.point(part.cell, other_size).distance(point)
                            < (size + other_size) * 0.7
                    })
                })
        }) {
            return;
        }
        thing.settle_motion();
        self.next_id = self.next_id.wrapping_add(1);
        self.things.push(thing);
    }
}
