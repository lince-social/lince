use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod formula;
pub use formula::Formula;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "placement", rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    Fixed,
    Cursor,
    Camera,
    Item(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpatialBinding {
    pub anchor: Anchor,
    pub position: [f64; 3],
    pub offset: [f64; 3],
    pub rotation: [f64; 4],
    pub follow_orientation: bool,
    pub camera_floor: bool,
}

impl Default for SpatialBinding {
    fn default() -> Self {
        Self {
            anchor: Anchor::Fixed,
            position: [0.0; 3],
            offset: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            follow_orientation: false,
            camera_floor: true,
        }
    }
}

impl SpatialBinding {
    pub fn translate(&mut self, delta: [f64; 3]) {
        for (position, delta) in self.position.iter_mut().zip(delta) {
            *position += delta;
        }
        if self.anchor == Anchor::Fixed {
            return;
        }
        let mut local = delta;
        if self.follow_orientation {
            let q = [-self.rotation[0], -self.rotation[1], -self.rotation[2]];
            let cross = |a: [f64; 3], b: [f64; 3]| {
                [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ]
            };
            let first = cross(q, delta);
            let second = cross(q, first);
            for i in 0..3 {
                local[i] += 2.0 * (self.rotation[3] * first[i] + second[i]);
            }
        }
        for (offset, delta) in self.offset.iter_mut().zip(local) {
            *offset += delta;
        }
    }

    pub fn same_attachment(&self, other: &Self) -> bool {
        self.anchor == other.anchor
            && self.offset == other.offset
            && self.follow_orientation == other.follow_orientation
            && self.camera_floor == other.camera_floor
            && (self.follow_orientation || self.rotation == other.rotation)
            && (self.anchor != Anchor::Fixed || self.position == other.position)
    }

    pub fn valid(&self) -> bool {
        self.position
            .iter()
            .chain(&self.offset)
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
            && self.rotation.iter().all(|v| v.is_finite())
            && (self.rotation.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-6
            && match &self.anchor {
                Anchor::Item(id) => crate::valid_uid(id, "placement"),
                _ => true,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    #[default]
    Hill,
    Valley,
    Slope,
    Ring,
    Formula,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TopologyEffect {
    pub name: String,
    pub enabled: bool,
    pub preset: Preset,
    pub amplitude: f64,
    pub extent: [f64; 2],
    pub rotation: f64,
    pub falloff: f64,
    pub formula: String,
}

impl Default for TopologyEffect {
    fn default() -> Self {
        Self {
            name: "Hill".into(),
            enabled: true,
            preset: Preset::Hill,
            amplitude: 100.0,
            extent: [500.0; 2],
            rotation: 0.0,
            falloff: 3.0,
            formula: "exp(-4*(x*x+y*y))".into(),
        }
    }
}

impl TopologyEffect {
    pub fn same_surface(&self, other: &Self) -> bool {
        self.enabled == other.enabled
            && self.preset == other.preset
            && self.amplitude == other.amplitude
            && self.extent == other.extent
            && self.rotation == other.rotation
            && self.falloff == other.falloff
            && (self.preset != Preset::Formula || self.formula == other.formula)
    }

    pub fn compile(&self) -> Result<Formula, String> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || !self.amplitude.is_finite()
            || self.amplitude.abs() > 100_000.0
            || self
                .extent
                .iter()
                .any(|v| !v.is_finite() || !(32.0..=100_000.0).contains(v))
            || !self.rotation.is_finite()
            || !self.falloff.is_finite()
            || !(2.0..=16.0).contains(&self.falloff)
        {
            return Err(
                "Use a name, finite height, radii from 32 to 100000, and falloff from 2 to 16."
                    .into(),
            );
        }
        Formula::compile(match self.preset {
            Preset::Hill => "1",
            Preset::Valley => "-1",
            Preset::Slope => "x",
            Preset::Ring => "exp(-pow((r-0.6)/0.15,2))",
            Preset::Formula => &self.formula,
        })
    }

    pub fn height(&self, compiled: &Formula, local: [f64; 2]) -> Result<f64, String> {
        if !self.enabled || self.amplitude == 0.0 {
            return Ok(0.0);
        }
        let (sine, cosine) = self.rotation.sin_cos();
        let x = (cosine * local[0] + sine * local[1]) / self.extent[0];
        let y = (-sine * local[0] + cosine * local[1]) / self.extent[1];
        let radius_squared = x * x + y * y;
        if radius_squared >= 1.0 {
            return Ok(0.0);
        }
        let height =
            self.amplitude * compiled.evaluate(x, y)? * (1.0 - radius_squared).powf(self.falloff);
        if height.is_finite() && height.abs() <= 1_000_000.0 {
            Ok(height)
        } else {
            Err("Formula produced a non-finite or excessive height.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_opposite_effects_cancel_and_edges_are_smooth() {
        let hill = TopologyEffect::default();
        let valley = TopologyEffect {
            preset: Preset::Valley,
            ..hill.clone()
        };
        let a = hill.compile().unwrap();
        let b = valley.compile().unwrap();
        for point in [
            [0.0, 0.0],
            [250.0, 100.0],
            [499.0, 0.0],
            [500.0, 0.0],
            [800.0, 0.0],
        ] {
            assert_eq!(
                hill.height(&a, point).unwrap() + valley.height(&b, point).unwrap(),
                0.0
            );
        }
        assert!(hill.height(&a, [499.0, 0.0]).unwrap() < 0.001);
    }

    #[test]
    fn bindings_and_effects_reject_invalid_persisted_numbers() {
        let mut binding = SpatialBinding::default();
        binding.offset[0] = f64::NAN;
        assert!(!binding.valid());
        let mut effect = TopologyEffect::default();
        effect.extent[0] = 0.0;
        assert!(effect.compile().is_err());
    }
}
