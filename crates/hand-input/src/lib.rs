use image::RgbImage;
use std::path::Path;
use tract_onnx::prelude::*;

pub type Point = [f32; 2];
type Model = std::sync::Arc<TypedRunnableModel>;

pub struct HandTracker {
    palm: Model,
    hand: Model,
}

impl HandTracker {
    pub fn load(directory: &Path) -> Result<Self, String> {
        Ok(Self {
            palm: load(&directory.join("palm_detection_mediapipe_2023feb.onnx"))?,
            hand: load(&directory.join("handpose_estimation_mediapipe_2023feb.onnx"))?,
        })
    }

    pub fn open_hand(&self, image: &RgbImage) -> Result<Option<Point>, String> {
        if image.width() == 0 || image.height() == 0 {
            return Err("Empty camera frame".into());
        }
        let side = image.width().max(image.height()) as f32;
        let center = [image.width() as f32 * 0.5, image.height() as f32 * 0.5];
        let outputs = run(&self.palm, crop(image, center, [1.0, 0.0], side, 192), 192)?;
        if outputs.len() != 2 || outputs[0].len() != 2016 * 18 || outputs[1].len() != 2016 {
            return Err("Unexpected palm model outputs".into());
        }
        let best = outputs[1]
            .iter()
            .enumerate()
            .filter(|(_, score)| score.is_finite() && **score > 0.8473)
            .max_by(|(_, a), (_, b)| a.total_cmp(b));
        let Some((index, _)) = best else {
            return Ok(None);
        };
        let anchor = anchor(index);
        let values = &outputs[0][index * 18..(index + 1) * 18];
        let mut palm = [[0.0; 2]; 7];
        for (landmark, pair) in palm.iter_mut().zip(values[4..].chunks_exact(2)) {
            *landmark = [
                (anchor[0] + pair[0] / 192.0 - 0.5) * side + center[0],
                (anchor[1] + pair[1] / 192.0 - 0.5) * side + center[1],
            ];
        }
        let Some((center, right, side)) = hand_crop(&palm) else {
            return Ok(None);
        };
        let outputs = run(&self.hand, crop(image, center, right, side, 224), 224)?;
        if outputs.len() != 4 || outputs[0].len() != 63 || outputs[1].len() != 1 {
            return Err("Unexpected hand model outputs".into());
        }
        if !outputs[1][0].is_finite() || outputs[1][0] < 0.8 {
            return Ok(None);
        }
        let mut landmarks = [[0.0; 3]; 21];
        for (landmark, values) in landmarks.iter_mut().zip(outputs[0].chunks_exact(3)) {
            landmark.copy_from_slice(values);
        }
        if !is_open(&landmarks) {
            return Ok(None);
        }
        let palm_center = [0, 5, 9, 13, 17].map(|i| landmarks[i]);
        let x = palm_center.iter().map(|p| p[0]).sum::<f32>() / 5.0;
        let y = palm_center.iter().map(|p| p[1]).sum::<f32>() / 5.0;
        let down = [-right[1], right[0]];
        let point = [
            (center[0] + (x / 224.0 - 0.5) * side * right[0] + (y / 224.0 - 0.5) * side * down[0])
                / image.width() as f32,
            (center[1] + (x / 224.0 - 0.5) * side * right[1] + (y / 224.0 - 0.5) * side * down[1])
                / image.height() as f32,
        ];
        Ok(point
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .then_some(point))
    }
}

fn load(path: &Path) -> Result<Model, String> {
    if !path.is_file() {
        return Err("Hand models are missing. Run cargo xtask hand-models.".into());
    }
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|model| model.into_optimized())
        .and_then(|model| model.into_runnable())
        .map_err(|error| format!("Could not load hand model: {error}"))
}

fn run(model: &Model, pixels: Vec<f32>, size: usize) -> Result<Vec<Vec<f32>>, String> {
    let tensor =
        Tensor::from_shape(&[1, size, size, 3], &pixels).map_err(|error| error.to_string())?;
    model
        .run(tvec!(tensor.into()))
        .map_err(|error| error.to_string())?
        .iter()
        .map(|value| {
            value
                .try_as_plain_ram()
                .and_then(|value| value.as_slice::<f32>().map(|slice| slice.to_vec()))
                .map_err(|e| e.to_string())
        })
        .collect()
}

fn anchor(index: usize) -> Point {
    let (index, grid, repeats) = if index < 1152 {
        (index, 24, 2)
    } else {
        (index - 1152, 12, 6)
    };
    let cell = index / repeats;
    [
        (cell % grid) as f32 / grid as f32 + 0.5 / grid as f32,
        (cell / grid) as f32 / grid as f32 + 0.5 / grid as f32,
    ]
}

fn hand_crop(palm: &[Point; 7]) -> Option<(Point, Point, f32)> {
    let direction = [palm[2][0] - palm[0][0], palm[2][1] - palm[0][1]];
    let length = direction[0].hypot(direction[1]);
    if !length.is_finite() || length < 1.0 {
        return None;
    }
    let up = [direction[0] / length, direction[1] / length];
    let right = [-up[1], up[0]];
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for point in palm {
        let rotated = [
            point[0] * right[0] + point[1] * right[1],
            point[0] * up[0] + point[1] * up[1],
        ];
        for axis in 0..2 {
            min[axis] = min[axis].min(rotated[axis]);
            max[axis] = max[axis].max(rotated[axis]);
        }
    }
    let width = max[0] - min[0];
    let height = max[1] - min[1];
    let x = (min[0] + max[0]) * 0.5;
    let y = (min[1] + max[1]) * 0.5 + height * 0.4;
    let side = width.max(height) * 3.0;
    (side.is_finite() && side >= 8.0).then_some((
        [right[0] * x + up[0] * y, right[1] * x + up[1] * y],
        right,
        side,
    ))
}

fn crop(image: &RgbImage, center: Point, right: Point, side: f32, size: usize) -> Vec<f32> {
    let down = [-right[1], right[0]];
    let mut pixels = Vec::with_capacity(size * size * 3);
    for y in 0..size {
        for x in 0..size {
            let u = ((x as f32 + 0.5) / size as f32 - 0.5) * side;
            let v = ((y as f32 + 0.5) / size as f32 - 0.5) * side;
            let px = center[0] + u * right[0] + v * down[0] - 0.5;
            let py = center[1] + u * right[1] + v * down[1] - 0.5;
            let ix = px.floor() as i32;
            let iy = py.floor() as i32;
            let mut color = [0.0; 3];
            for (dx, wx) in [
                (0, 1.0 - px.fract().rem_euclid(1.0)),
                (1, px.fract().rem_euclid(1.0)),
            ] {
                for (dy, wy) in [
                    (0, 1.0 - py.fract().rem_euclid(1.0)),
                    (1, py.fract().rem_euclid(1.0)),
                ] {
                    let x = ix + dx;
                    let y = iy + dy;
                    if x >= 0 && y >= 0 && x < image.width() as i32 && y < image.height() as i32 {
                        let pixel = image.get_pixel(x as u32, y as u32);
                        for channel in 0..3 {
                            color[channel] += f32::from(pixel[channel]) / 255.0 * wx * wy;
                        }
                    }
                }
            }
            pixels.extend(color);
        }
    }
    pixels
}

fn is_open(points: &[[f32; 3]; 21]) -> bool {
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return false;
    }
    [5, 9, 13, 17].into_iter().all(|base| {
        let a = vector(points[base], points[base + 1]);
        let b = vector(points[base + 1], points[base + 3]);
        let denominator = norm(a) * norm(b);
        denominator > 1.0
            && dot(a, b) / denominator > 0.75
            && norm(vector(points[0], points[base + 3]))
                > norm(vector(points[0], points[base + 1])) * 1.15
    })
}

fn vector(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

#[derive(Default)]
pub struct PanTracker {
    previous: Option<(Point, Point, Point, f64)>,
}

impl PanTracker {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    pub fn update(&mut self, hand: Option<Point>, seconds: f64) -> Point {
        let Some(point) = hand.filter(|point| {
            seconds.is_finite()
                && point
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        }) else {
            self.reset();
            return [0.0; 2];
        };
        let Some((raw, filtered, anchor, timestamp)) = self.previous else {
            self.previous = Some((point, point, point, seconds));
            return [0.0; 2];
        };
        let gap = seconds - timestamp;
        if gap <= 0.0 || gap > 0.35 || (point[0] - raw[0]).hypot(point[1] - raw[1]) > 0.15 {
            self.previous = Some((point, point, point, seconds));
            return [0.0; 2];
        }
        let alpha = 1.0 - (-gap / 0.045).exp() as f32;
        let filtered = [
            filtered[0] + (point[0] - filtered[0]) * alpha,
            filtered[1] + (point[1] - filtered[1]) * alpha,
        ];
        let delta = [filtered[0] - anchor[0], filtered[1] - anchor[1]];
        if delta[0].hypot(delta[1]) < 0.003 {
            self.previous = Some((point, filtered, anchor, seconds));
            return [0.0; 2];
        }
        self.previous = Some((point, filtered, filtered, seconds));
        delta
    }
}

#[cfg(test)]
mod tests;
