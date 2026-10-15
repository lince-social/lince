use serde::{Deserialize, Serialize};

pub mod desktop;
pub mod outputs;

pub const LIMIT: f32 = 100_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    Fixed,
    Fit,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Clip,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    pub sizing: Sizing,
    pub size: f32,
    pub min: f32,
    pub max: f32,
    pub overflow: Overflow,
}

impl Axis {
    pub fn fixed(size: f32) -> Self {
        Self {
            sizing: Sizing::Fixed,
            size,
            min: 1.0,
            max: LIMIT,
            overflow: Overflow::Clip,
        }
    }

    pub fn valid(self) -> bool {
        [self.size, self.min, self.max]
            .iter()
            .all(|n| n.is_finite())
            && self.min >= 1.0
            && self.max <= LIMIT
            && self.min <= self.max
            && (self.min..=self.max).contains(&self.size)
    }

    pub fn resolve(self, content: f32, available: f32) -> f32 {
        match self.sizing {
            Sizing::Fixed => self.size,
            Sizing::Fit => content,
            Sizing::Fill => available,
        }
        .clamp(self.min, self.max)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrangement {
    Free,
    Row,
    Column,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    pub axes: [Axis; 2],
    pub arrangement: Arrangement,
    pub padding: f32,
    pub gap: f32,
    pub columns: u16,
    pub wrap: bool,
}

impl Rules {
    pub fn fixed(size: impl Into<[f32; 2]>) -> Self {
        let [width, height] = size.into();
        Self {
            axes: [Axis::fixed(width), Axis::fixed(height)],
            arrangement: Arrangement::Free,
            padding: 0.0,
            gap: 8.0,
            columns: 2,
            wrap: true,
        }
    }

    pub fn valid(self) -> bool {
        self.axes.iter().all(|axis| axis.valid())
            && self.padding.is_finite()
            && (0.0..=10_000.0).contains(&self.padding)
            && self.gap.is_finite()
            && (0.0..=10_000.0).contains(&self.gap)
            && (1..=128).contains(&self.columns)
    }
}
