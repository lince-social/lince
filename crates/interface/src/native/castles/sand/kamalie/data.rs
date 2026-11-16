use bevy::prelude::*;
use serde::Deserialize;
use std::sync::OnceLock;

pub const TILE: f32 = 8.0;
pub const WIDTH: usize = 51;
pub const HEIGHT: usize = 37;
pub const PLAYER_RADIUS: f32 = 3.0;
pub const PLAYER_SPEED: f32 = 48.0;
pub const STEP: f32 = 1.0 / 120.0;
pub const MAX_STEPS: usize = 8;
pub const MAX_OBJECTS: usize = 192;
pub const MAX_ATTACHMENTS: usize = 8;
pub const GRAB_REACH: f32 = 30.0;
pub const MIN_IMPACT_SPEED: f32 = 24.0;
pub const FLOOR: Color = Color::srgb(0.075, 0.14, 0.095);
pub const WALL: Color = Color::srgb(0.18, 0.25, 0.14);
pub const EXIT: Color = Color::srgb(0.018, 0.028, 0.02);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Stick,
    Bush,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Material {
    pub id: String,
    pub responsive: bool,
    pub integrity: f32,
    pub impact_transfer: f32,
    pub wear: f32,
    pub part_size: f32,
    pub color: [f32; 3],
    pub spawn_weight: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    pub revision: u32,
    pub stick: Material,
    pub bush: Material,
}

impl Catalog {
    pub fn load() -> Result<&'static Self, &'static str> {
        static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
        CATALOG
            .get_or_init(|| {
                let catalog: Catalog = serde_json::from_str(include_str!("materials.json"))
                    .map_err(|error| error.to_string())?;
                if catalog.revision != 1
                    || catalog.stick.id != "stick"
                    || catalog.bush.id != "bush"
                    || !catalog.stick.responsive
                    || catalog.bush.responsive
                    || catalog.stick.integrity <= catalog.bush.integrity
                {
                    return Err("Unsupported Kamalie material catalog".into());
                }
                for material in [&catalog.stick, &catalog.bush] {
                    if [
                        material.integrity,
                        material.impact_transfer,
                        material.wear,
                        material.part_size,
                    ]
                    .iter()
                    .any(|value| !value.is_finite() || *value <= 0.0)
                        || material.part_size > TILE * 0.5
                        || material.spawn_weight == 0
                        || material.spawn_weight > 10_000
                        || material
                            .color
                            .iter()
                            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                    {
                        return Err(format!("Invalid Kamalie material: {}", material.id));
                    }
                }
                Ok(catalog)
            })
            .as_ref()
            .map_err(String::as_str)
    }

    pub fn material(&self, kind: Kind) -> &Material {
        match kind {
            Kind::Stick => &self.stick,
            Kind::Bush => &self.bush,
        }
    }
}

pub(super) struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }

    pub fn index(&mut self, count: usize) -> usize {
        ((u128::from(self.next()) * count as u128) >> 64) as usize
    }

    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}
