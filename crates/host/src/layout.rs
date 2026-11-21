//! Layout rules for desktop placement and output assignment.

use serde::{Deserialize, Serialize};

pub mod desktop;
pub mod outputs;

/// Largest supported dimension in logical units.
pub const LIMIT: f32 = 100_000.0;

/// Source of the requested size along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    /// Use the configured size.
    Fixed,
    /// Follow the measured content size.
    Fit,
    /// Use the available space.
    Fill,
}

/// How to present content that exceeds its allocated space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Clip,
    Scroll,
}

/// Sizing and overflow rules for a single axis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    pub sizing: Sizing,
    pub size: f32,
    pub min: f32,
    pub max: f32,
    pub overflow: Overflow,
}

impl Axis {
    /// Sets a fixed size with the default bounds and clipping.
    pub fn fixed(size: f32) -> Self {
        Self {
            sizing: Sizing::Fixed,
            size,
            min: 1.0,
            max: LIMIT,
            overflow: Overflow::Clip,
        }
    }

    /// Checks that the size and bounds are ordered and within the supported range.
    pub fn valid(self) -> bool {
        self.min >= 1.0 && self.max <= LIMIT && (self.min..=self.max).contains(&self.size)
    }

    /// Selects the requested size and clamps it to the configured bounds.
    ///
    /// # Panics
    ///
    /// Panics when the bounds are reversed or either bound is NaN.
    pub fn resolve(self, content: f32, available: f32) -> f32 {
        match self.sizing {
            Sizing::Fixed => self.size,
            Sizing::Fit => content,
            Sizing::Fill => available,
        }
        .clamp(self.min, self.max)
    }
}

/// Placement policy for children in a shared container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrangement {
    Free,
    Row,
    Column,
    Grid,
}

/// Size and placement settings for a layout container.
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
    /// Sets fixed width and height with the default free arrangement.
    pub fn fixed(size: impl Into<[f32; 2]>) -> Self {
        Self {
            axes: size.into().map(Axis::fixed),
            arrangement: Arrangement::Free,
            padding: 0.0,
            gap: 8.0,
            columns: 2,
            wrap: true,
        }
    }

    /// Checks both axes, spacing, and the allowed grid column count.
    pub fn valid(self) -> bool {
        self.axes.into_iter().all(Axis::valid)
            && (0.0..=10_000.0).contains(&self.padding)
            && (0.0..=10_000.0).contains(&self.gap)
            && (1..=128).contains(&self.columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_admits_inclusive_bounds_and_rejects_nonfinite_dimensions() {
        assert!(Rules::fixed([1.0, LIMIT]).valid());
        for dimension in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!Axis::fixed(dimension).valid(), "{dimension}");
            let mut axis = Axis::fixed(16.0);
            axis.min = dimension;
            assert!(!axis.valid(), "minimum {dimension}");
            axis = Axis::fixed(16.0);
            axis.max = dimension;
            assert!(!axis.valid(), "maximum {dimension}");

            let fixed = Rules::fixed([16.0; 2]);
            assert!(
                !Rules {
                    gap: dimension,
                    ..fixed
                }
                .valid(),
                "gap {dimension}"
            );
            assert!(
                !Rules {
                    padding: dimension,
                    ..fixed
                }
                .valid(),
                "padding {dimension}"
            );
        }
    }
}
