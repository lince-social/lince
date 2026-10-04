use serde::{Deserialize, Serialize};

pub const MAX_POINTS: usize = 16_384;
pub const MAX_STROKES: usize = 512;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 4],
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawing {
    pub width: u32,
    pub height: u32,
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
    pub fn validate(&self) -> Result<(), String> {
        if !(64..=2048).contains(&self.width)
            || !(64..=2048).contains(&self.height)
            || self.strokes.len() > MAX_STROKES
            || self
                .strokes
                .iter()
                .map(|stroke| stroke.points.len())
                .sum::<usize>()
                > MAX_POINTS
        {
            return Err("Drawing exceeds its size or stroke limit.".into());
        }
        let mut cost = 0.0_f64;
        for stroke in &self.strokes {
            if !stroke.width.is_finite()
                || !(1.0..=64.0).contains(&stroke.width)
                || stroke.points.is_empty()
                || stroke.points.iter().any(|point| {
                    !point[0].is_finite()
                        || !point[1].is_finite()
                        || point[0] < 0.0
                        || point[1] < 0.0
                        || point[0] > self.width as f32
                        || point[1] > self.height as f32
                })
            {
                return Err("Drawing contains an invalid stroke.".into());
            }
            let length = stroke
                .points
                .windows(2)
                .map(|pair| f64::from((pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])))
                .sum::<f64>()
                + stroke.points.len() as f64;
            cost += length * f64::from(stroke.width + 2.0).powi(2);
        }
        if cost > 32_000_000.0 {
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
        let mut drawing = Drawing::default();
        drawing.strokes.push(Stroke {
            points: vec![[1.0, 2.0], [50.0, 70.0]],
            color: [0, 0, 0, 255],
            width: 4.0,
        });
        assert!(drawing.validate().is_ok());
        drawing.strokes[0].points[0][0] = f32::NAN;
        assert!(drawing.validate().is_err());
        drawing.strokes[0].points = vec![[1.0, 2.0]; MAX_POINTS + 1];
        assert!(drawing.validate().is_err());
        drawing.strokes[0].points = vec![[0.0, 0.0], [640.0, 400.0]].repeat(100);
        drawing.strokes[0].width = 64.0;
        assert!(drawing.validate().is_err());
    }
}
