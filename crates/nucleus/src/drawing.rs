use serde::{Deserialize, Serialize};

/// Maximum number of points across a drawing.
pub const MAX_POINTS: usize = 16_384;
/// Maximum number of strokes in a drawing.
pub const MAX_STROKES: usize = 512;

const MAX_RENDER_COST: f64 = 32_000_000.0;
const SIZE_ERROR: &str = "Drawing exceeds its size or stroke limit.";

/// A path drawn in one color and width.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    /// Canvas coordinates of the stroke, in path order.
    pub points: Vec<[f32; 2]>,
    /// Red, green, blue, and alpha channels.
    pub color: [u8; 4],
    /// Stroke width in canvas pixels.
    pub width: f32,
}

/// An ordered drawing on a bounded canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawing {
    /// Horizontal canvas size in pixels.
    pub width: u32,
    /// Vertical canvas size in pixels.
    pub height: u32,
    /// Strokes in drawing order.
    pub strokes: Vec<Stroke>,
}

impl Default for Drawing {
    fn default() -> Self {
        Self {
            width: 640,
            height: 400,
            strokes: Vec::new(),
        }
    }
}

impl Drawing {
    /// Checks the dimensions, points, and total work for drawing the strokes.
    ///
    /// # Errors
    ///
    /// Returns a size, stroke, or rendering limit diagnostic in that order.
    pub fn validate(&self) -> Result<(), String> {
        let [Ok(width), Ok(height)] = [self.width, self.height].map(u16::try_from) else {
            return Err(SIZE_ERROR.into());
        };
        if !(64..=2048).contains(&width)
            || !(64..=2048).contains(&height)
            || self.strokes.len() > MAX_STROKES
            || self
                .strokes
                .iter()
                .try_fold(MAX_POINTS, |remaining, stroke| {
                    remaining.checked_sub(stroke.points.len())
                })
                .is_none()
        {
            return Err(SIZE_ERROR.into());
        }

        let horizontal = 0.0..=f32::from(width);
        let vertical = 0.0..=f32::from(height);
        let mut cost = 0.0_f64;
        for stroke in &self.strokes {
            if !(1.0..=64.0).contains(&stroke.width)
                || stroke.points.is_empty()
                || stroke
                    .points
                    .iter()
                    .any(|[x, y]| !horizontal.contains(x) || !vertical.contains(y))
            {
                return Err("Drawing contains an invalid stroke.".into());
            }
            let points = u16::try_from(stroke.points.len()).ok().ok_or(SIZE_ERROR)?;
            let length = stroke
                .points
                .array_windows()
                .map(|&[[x, y], [next_x, next_y]]| f64::from((next_x - x).hypot(next_y - y)))
                .sum::<f64>()
                + f64::from(points);
            cost += length * f64::from(stroke.width + 2.0).powi(2);
        }
        if cost > MAX_RENDER_COST {
            return Err("Drawing exceeds its rendering limit.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_cover_coordinates_memory_and_rendering_work() {
        let drawing = |width, points| Drawing {
            strokes: vec![Stroke {
                points,
                color: [0, 0, 0, 255],
                width,
            }],
            ..Drawing::default()
        };
        assert!(
            drawing(4.0, vec![[1.0, 2.0], [50.0, 70.0]])
                .validate()
                .is_ok()
        );
        assert!(
            drawing(4.0, vec![[f32::NAN, 2.0], [50.0, 70.0]])
                .validate()
                .is_err()
        );
        assert!(
            drawing(4.0, vec![[1.0, 2.0]; MAX_POINTS + 1])
                .validate()
                .is_err()
        );
        assert!(
            drawing(64.0, [[0.0, 0.0], [640.0, 400.0]].repeat(100))
                .validate()
                .is_err()
        );
    }
}
