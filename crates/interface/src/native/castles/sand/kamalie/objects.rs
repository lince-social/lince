use super::data::{Catalog, Kind, Random};
use bevy::prelude::*;

#[derive(Clone)]
pub(super) struct Part {
    pub cell: IVec2,
    pub integrity: f32,
    pub separated: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Mount {
    pub direction: Vec2,
    pub anchor: IVec2,
}

pub(super) struct Thing {
    pub id: u64,
    pub kind: Kind,
    pub parts: Vec<Part>,
    pub position: Vec2,
    pub angle: f32,
    pub previous_position: Vec2,
    pub previous_angle: f32,
    pub velocity: Vec2,
    pub spin: f32,
    pub mount: Option<Mount>,
    pub thrust: f32,
    pub scarred: bool,
    pub scatter: Vec2,
    pub scatter_strength: f32,
    pub impact_cooldown: f32,
}

pub(super) fn rotate(point: Vec2, angle: f32) -> Vec2 {
    Vec2::from_angle(angle).rotate(point)
}

pub(super) fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl Thing {
    pub fn new(
        id: u64,
        kind: Kind,
        position: Vec2,
        random: &mut Random,
        catalog: &Catalog,
    ) -> Self {
        let material = catalog.material(kind);
        let cells: Vec<_> = match kind {
            Kind::Stick => (0..(7 + random.index(6)) as i32)
                .map(|x| IVec2::new(x, 0))
                .collect(),
            Kind::Bush => {
                let radius = 2 + random.index(2) as i32;
                (-radius..=radius)
                    .flat_map(|y| (-radius..=radius).map(move |x| IVec2::new(x, y)))
                    .filter(|cell| cell.length_squared() <= radius * radius + 1)
                    .collect()
            }
        };
        Self {
            id,
            kind,
            position,
            parts: cells
                .into_iter()
                .map(|cell| Part {
                    cell,
                    integrity: material.integrity,
                    separated: false,
                })
                .collect(),
            angle: 0.0,
            previous_position: position,
            previous_angle: 0.0,
            velocity: Vec2::ZERO,
            spin: 0.0,
            mount: None,
            thrust: 0.0,
            scarred: false,
            scatter: Vec2::ZERO,
            scatter_strength: 0.0,
            impact_cooldown: 0.0,
        }
    }

    pub fn point(&self, cell: IVec2, size: f32) -> Vec2 {
        self.position + rotate(cell.as_vec2() * size, self.angle)
    }

    pub fn previous_point(&self, cell: IVec2, size: f32) -> Vec2 {
        self.previous_position + rotate(cell.as_vec2() * size, self.previous_angle)
    }

    pub fn center(&self, size: f32) -> Vec2 {
        let sum: Vec2 = self
            .parts
            .iter()
            .map(|part| self.point(part.cell, size))
            .sum();
        sum / self.parts.len().max(1) as f32
    }

    pub fn radius(&self, size: f32) -> f32 {
        let center = self.center(size);
        self.parts
            .iter()
            .map(|part| self.point(part.cell, size).distance(center))
            .fold(size * 0.5, f32::max)
            + size
    }

    pub fn settle_motion(&mut self) {
        self.previous_position = self.position;
        self.previous_angle = self.angle;
        self.velocity = Vec2::ZERO;
        self.spin = 0.0;
        self.impact_cooldown = 0.1;
    }

    pub fn recenter(&mut self, size: f32) {
        let local_center: Vec2 = self.parts.iter().map(|part| part.cell.as_vec2()).sum();
        let origin = (local_center / self.parts.len().max(1) as f32)
            .round()
            .as_ivec2();
        self.position += rotate(origin.as_vec2() * size, self.angle);
        self.previous_position += rotate(origin.as_vec2() * size, self.previous_angle);
        for part in &mut self.parts {
            part.cell -= origin;
        }
        if let Some(mount) = &mut self.mount {
            mount.anchor -= origin;
        }
    }
}
