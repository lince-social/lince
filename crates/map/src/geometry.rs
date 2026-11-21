// non-lean-yet
// Scope: const MAX_LATITUDE, const MAX_DATA_ZOOM, const MAX_DISPLAY_ZOOM, const WORLD_METERS.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 132, 133, 134, 135, 141, 142, 150, 237, 240, 253.
// - [Clippy: clippy::cast_possible_truncation] Prove the bounds or use a checked conversion. Sites
//   132, 133, 134, 135, 141, 142, 150, 237, 240.
// - [Clippy: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 142, 150, 237, 240.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 225, 266.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 5,
//   6, 7, 8, 11, 12, 13, 17, 24, 35, 47, 48, 49, 65, 71, 75, 84, 95, 101, 111, 123, 149, 156, 189,
//   190, 191, 192, 193, 197, 211, 225, 266, 281, 282, 283, 284, 288.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites pyramid: String
//   (266), tiles: String (225).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_LATITUDE: f64 = 85.0511287798066;
pub const MAX_DATA_ZOOM: u8 = 14;
pub const MAX_DISPLAY_ZOOM: f64 = 20.0;
pub const WORLD_METERS: f64 = 40_075_016.68557849;
pub const MAX_AREA_TILES: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinate {
    pub fn valid(self) -> bool {
        self.latitude.is_finite()
            && self.longitude.is_finite()
            && self.latitude.abs() <= 90.0
            && self.longitude.abs() <= 180.0
    }

    pub fn normalized(self) -> [f64; 2] {
        let latitude = self
            .latitude
            .clamp(-MAX_LATITUDE, MAX_LATITUDE)
            .to_radians();
        [
            (self.longitude + 180.0) / 360.0,
            (1.0 - latitude.tan().asinh() / std::f64::consts::PI) / 2.0,
        ]
    }

    pub fn from_normalized(x: f64, y: f64) -> Self {
        Self {
            latitude: (std::f64::consts::PI * (1.0 - 2.0 * y.clamp(0.0, 1.0)))
                .sinh()
                .atan()
                .to_degrees(),
            longitude: (x * 360.0).rem_euclid(360.0) - 180.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub center: Coordinate,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Coordinate {
                latitude: 0.0,
                longitude: 0.0,
            },
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn valid(self) -> bool {
        self.center.valid()
            && self.zoom.is_finite()
            && (0.0..=MAX_DISPLAY_ZOOM).contains(&self.zoom)
    }

    pub fn resolution(self) -> f64 {
        WORLD_METERS / (256.0 * 2.0_f64.powf(self.zoom))
    }

    pub fn coordinate(self, point: [f64; 2], size: [f64; 2]) -> Coordinate {
        let [x, y] = self.center.normalized();
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        Coordinate::from_normalized(
            x + (point[0] - size[0] / 2.0) / pixels,
            y + (point[1] - size[1] / 2.0) / pixels,
        )
    }

    pub fn screen(self, point: Coordinate, size: [f64; 2]) -> [f64; 2] {
        let [x, y] = self.center.normalized();
        let [px, py] = point.normalized();
        let dx = (px - x + 0.5).rem_euclid(1.0) - 0.5;
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        [
            size[0] / 2.0 + dx * pixels,
            size[1] / 2.0 + (py - y) * pixels,
        ]
    }

    pub fn pan(&mut self, delta: [f64; 2]) {
        let [x, y] = self.center.normalized();
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        self.center = Coordinate::from_normalized(x - delta[0] / pixels, y - delta[1] / pixels);
    }

    pub fn zoom_at(&mut self, amount: f64, point: [f64; 2], size: [f64; 2]) {
        let anchor = self.coordinate(point, size).normalized();
        self.zoom = (self.zoom + amount).clamp(0.0, MAX_DISPLAY_ZOOM);
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        self.center = Coordinate::from_normalized(
            anchor[0] - (point[0] - size[0] / 2.0) / pixels,
            anchor[1] - (point[1] - size[1] / 2.0) / pixels,
        );
    }

    pub fn area(self, a: [f64; 2], b: [f64; 2], size: [f64; 2]) -> Bounds {
        let left = self.coordinate([a[0].min(b[0]), a[1].min(b[1])], size);
        let right = self.coordinate([a[0].max(b[0]), a[1].max(b[1])], size);
        let entire_world = (b[0] - a[0]).abs() >= 256.0 * 2.0_f64.powf(self.zoom);
        Bounds {
            west: if entire_world { -180.0 } else { left.longitude },
            east: if entire_world { 180.0 } else { right.longitude },
            south: right.latitude,
            north: left.latitude,
        }
    }

    pub fn visible_tiles(self, size: [f64; 2], max_zoom: u8) -> Vec<Tile> {
        let zoom = self.data_zoom(max_zoom);
        let count = 1_i64 << zoom;
        let [x, y] = self.center.normalized();
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        let half = [
            size[0].clamp(1.0, 4096.0) / pixels / 2.0,
            size[1].clamp(1.0, 4096.0) / pixels / 2.0,
        ];
        let left = ((x - half[0]) * count as f64).floor() as i64;
        let right = ((x + half[0]) * count as f64).floor() as i64;
        let top = (((y - half[1]) * count as f64).floor() as i64).clamp(0, count - 1);
        let bottom = (((y + half[1]) * count as f64).floor() as i64).clamp(0, count - 1);
        let mut result = BTreeSet::new();
        for ty in top..=bottom {
            for tx in left..=right {
                result.insert(Tile {
                    z: zoom,
                    x: tx.rem_euclid(count) as u32,
                    y: ty as u32,
                });
            }
        }
        result.into_iter().collect()
    }

    pub fn data_zoom(self, max_zoom: u8) -> u8 {
        (self.zoom - 1.01_f64.log2())
            .ceil()
            .max(0.0)
            .min(f64::from(max_zoom.min(MAX_DATA_ZOOM))) as u8
    }

    pub fn area_screen(self, bounds: Bounds, size: [f64; 2]) -> [[f64; 2]; 2] {
        let [center, _] = self.center.normalized();
        let west = (bounds.west + 180.0) / 360.0;
        let east =
            (bounds.east + 180.0) / 360.0 + if bounds.east < bounds.west { 1.0 } else { 0.0 };
        let shift = (center - (west + east) / 2.0).round();
        let pixels = 256.0 * 2.0_f64.powf(self.zoom);
        [
            [
                size[0] / 2.0 + (west + shift - center) * pixels,
                self.screen(
                    Coordinate {
                        latitude: bounds.north,
                        longitude: bounds.west,
                    },
                    size,
                )[1],
            ],
            [
                size[0] / 2.0 + (east + shift - center) * pixels,
                self.screen(
                    Coordinate {
                        latitude: bounds.south,
                        longitude: bounds.east,
                    },
                    size,
                )[1],
            ],
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Bounds {
    pub fn between(a: Coordinate, b: Coordinate) -> Self {
        let (west, east) = if (a.longitude - b.longitude).abs() > 180.0 {
            (a.longitude.max(b.longitude), a.longitude.min(b.longitude))
        } else {
            (a.longitude.min(b.longitude), a.longitude.max(b.longitude))
        };
        Self {
            west,
            east,
            south: a.latitude.min(b.latitude),
            north: a.latitude.max(b.latitude),
        }
    }

    pub fn valid(self) -> bool {
        Coordinate {
            latitude: self.south,
            longitude: self.west,
        }
        .valid()
            && Coordinate {
                latitude: self.north,
                longitude: self.east,
            }
            .valid()
            && self.south <= self.north
    }

    pub fn tiles(self, zoom: u8) -> Result<Vec<Tile>, String> {
        if !self.valid() || zoom > MAX_DATA_ZOOM {
            return Err("Choose a valid area and a data zoom between 0 and 14".into());
        }
        let count = 1_u32 << zoom;
        let index = |latitude, longitude| {
            let point = Coordinate {
                latitude,
                longitude,
            }
            .normalized();
            (
                (point[0] * f64::from(count))
                    .floor()
                    .clamp(0.0, f64::from(count - 1)) as u32,
                (point[1] * f64::from(count))
                    .floor()
                    .clamp(0.0, f64::from(count - 1)) as u32,
            )
        };
        let (left, top) = index(self.north, self.west);
        let (right, bottom) = index(self.south, self.east);
        let ranges = if self.west <= self.east {
            vec![(left, right)]
        } else {
            vec![(left, count - 1), (0, right)]
        };
        let width: u64 = ranges.iter().map(|(a, b)| u64::from(b - a + 1)).sum();
        if width * u64::from(bottom - top + 1) > MAX_AREA_TILES as u64 {
            return Err(
                "This area needs too many tiles. Select a smaller area or less detail".into(),
            );
        }
        Ok(ranges
            .into_iter()
            .flat_map(|(a, b)| {
                (a..=b).flat_map(move |x| (top..=bottom).map(move |y| Tile { z: zoom, x, y }))
            })
            .collect())
    }

    pub fn pyramid(self, zoom: u8) -> Result<Vec<Tile>, String> {
        let mut result = Vec::new();
        for z in 0..=zoom {
            result.extend(self.tiles(z)?);
            if result.len() > MAX_AREA_TILES {
                return Err(
                    "This area needs too many tiles. Select a smaller area or less detail".into(),
                );
            }
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl Tile {
    pub fn valid(self) -> bool {
        self.z <= MAX_DATA_ZOOM && self.x < (1_u32 << self.z) && self.y < (1_u32 << self.z)
    }
}
