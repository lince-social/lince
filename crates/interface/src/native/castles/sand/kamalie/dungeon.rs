use super::data::{HEIGHT, Random, TILE, WIDTH};
use bevy::prelude::*;
use std::collections::VecDeque;

pub(super) struct Dungeon {
    pub floor: Vec<bool>,
    pub entrance: Vec2,
    pub exit: IVec2,
}

impl Dungeon {
    pub fn generate(random: &mut Random) -> Self {
        let mut dungeon = Self {
            floor: vec![false; WIDTH * HEIGHT],
            entrance: Vec2::splat(5.5 * TILE),
            exit: IVec2::ZERO,
        };
        let columns = (WIDTH - 5) / 4;
        let rows = (HEIGHT - 5) / 4;
        let point = |index: usize| {
            IVec2::new(
                3 + (index % columns) as i32 * 4,
                3 + (index / columns) as i32 * 4,
            )
        };
        let mut visited = vec![false; columns * rows];
        let first = random.index(visited.len());
        let mut stack = vec![first];
        visited[first] = true;
        while let Some(&current) = stack.last() {
            let x = current % columns;
            let y = current / columns;
            let neighbors = [
                (x > 0).then(|| current - 1),
                (x + 1 < columns).then_some(current + 1),
                (y > 0).then(|| current - columns),
                (y + 1 < rows).then_some(current + columns),
            ];
            let choices: Vec<_> = neighbors
                .into_iter()
                .flatten()
                .filter(|index| !visited[*index])
                .collect();
            if choices.is_empty() {
                stack.pop();
                continue;
            }
            let next = choices[random.index(choices.len())];
            visited[next] = true;
            dungeon.corridor(point(current), point(next), random.index(3) as i32);
            stack.push(next);
        }
        for current in 0..visited.len() {
            if random.unit() < 0.22 {
                let next = if random.unit() < 0.5 && current % columns + 1 < columns {
                    current + 1
                } else if current / columns + 1 < rows {
                    current + columns
                } else {
                    current
                };
                dungeon.corridor(point(current), point(next), random.index(3) as i32);
            }
            if random.unit() < 0.16 {
                let center = point(current);
                let half = IVec2::new(1 + random.index(3) as i32, 1 + random.index(3) as i32);
                dungeon.carve(center - half, center + half);
            }
        }
        dungeon.carve(IVec2::ONE, IVec2::splat(10));
        let distances = dungeon.distances(IVec2::splat(5));
        let farthest = distances
            .iter()
            .copied()
            .filter(|distance| *distance != usize::MAX)
            .max()
            .unwrap_or(1);
        let candidates: Vec<_> = distances
            .iter()
            .enumerate()
            .filter(|(_, distance)| **distance != usize::MAX && **distance >= farthest * 3 / 4)
            .map(|(index, _)| index)
            .collect();
        let exit = candidates[random.index(candidates.len())];
        dungeon.exit = IVec2::new((exit % WIDTH) as i32, (exit / WIDTH) as i32);
        dungeon
    }

    pub fn open(&self, cell: IVec2) -> bool {
        cell.x >= 0
            && cell.y >= 0
            && cell.x < WIDTH as i32
            && cell.y < HEIGHT as i32
            && self.floor[cell.y as usize * WIDTH + cell.x as usize]
    }

    pub fn clear_circle(&self, center: Vec2, radius: f32) -> bool {
        let low = ((center - Vec2::splat(radius)) / TILE).floor().as_ivec2();
        let high = ((center + Vec2::splat(radius)) / TILE).floor().as_ivec2();
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                let cell = IVec2::new(x, y);
                if self.open(cell) {
                    continue;
                }
                let min = cell.as_vec2() * TILE;
                if center.distance_squared(center.clamp(min, min + Vec2::splat(TILE)))
                    < radius * radius
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn at_exit(&self, center: Vec2) -> bool {
        (center / TILE).floor().as_ivec2() == self.exit
    }

    pub fn move_circle(&self, from: Vec2, delta: Vec2, radius: f32) -> Vec2 {
        let mut position = from;
        let steps = (delta.length() / (TILE * 0.25)).ceil().max(1.0) as usize;
        let step = delta / steps as f32;
        for _ in 0..steps {
            let next = position + Vec2::new(step.x, 0.0);
            if self.clear_circle(next, radius) {
                position = next;
            }
            let next = position + Vec2::new(0.0, step.y);
            if self.clear_circle(next, radius) {
                position = next;
            }
        }
        position
    }

    fn carve(&mut self, low: IVec2, high: IVec2) {
        for y in low.y.max(1)..=high.y.min(HEIGHT as i32 - 2) {
            for x in low.x.max(1)..=high.x.min(WIDTH as i32 - 2) {
                self.floor[y as usize * WIDTH + x as usize] = true;
            }
        }
    }

    fn corridor(&mut self, from: IVec2, to: IVec2, width: i32) {
        let negative = IVec2::splat(width / 2);
        let positive = IVec2::splat((width + 1) / 2);
        self.carve(from.min(to) - negative, from.max(to) + positive);
    }

    fn distances(&self, start: IVec2) -> Vec<usize> {
        let index = |cell: IVec2| cell.y as usize * WIDTH + cell.x as usize;
        let mut distances = vec![usize::MAX; WIDTH * HEIGHT];
        distances[index(start)] = 0;
        let mut pending = VecDeque::from([start]);
        while let Some(cell) = pending.pop_front() {
            for direction in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                let next = cell + direction;
                if self.open(next) && distances[index(next)] == usize::MAX {
                    distances[index(next)] = distances[index(cell)] + 1;
                    pending.push_back(next);
                }
            }
        }
        distances
    }
}
