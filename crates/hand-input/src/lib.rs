use image::RgbImage;
use std::path::Path;
use tract_onnx::prelude::*;

pub type Point = [f32; 2];
type Model = std::sync::Arc<TypedRunnableModel>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandObservation {
    pub center: Point,
    pub pointer: Point,
    pub pinched: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerGesture {
    Hover(Point),
    Begin(Point),
    Update(Point),
    End(Point),
    Cancel,
}

#[derive(Default)]
pub struct PointerTracker {
    active: bool,
    blocked: bool,
    previous: Option<Point>,
    candidate: Option<(bool, u8)>,
}

impl PointerTracker {
    pub fn disarm(&mut self) {
        let _ = self.cancel();
        self.blocked = true;
    }

    pub fn cancel(&mut self) -> Option<PointerGesture> {
        let active = std::mem::take(&mut self.active);
        self.blocked |= active || self.candidate.is_some_and(|(pinched, _)| pinched);
        self.previous = None;
        self.candidate = None;
        active.then_some(PointerGesture::Cancel)
    }

    pub fn update(&mut self, hands: &[HandObservation]) -> Option<PointerGesture> {
        let hand = hands.iter().filter(|hand| {
            hand.center.iter().chain(hand.pointer.iter()).all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        }).max_by(|a, b| a.center[0].total_cmp(&b.center[0]));
        let Some(hand) = hand else { return self.cancel(); };
        if self.active && self.previous.is_some_and(|point| (point[0] - hand.center[0]).hypot(point[1] - hand.center[1]) > 0.15) {
            return self.cancel();
        }
        if self.blocked {
            if hand.pinched { return None; }
            self.blocked = false;
        }
        self.previous = Some(hand.center);
        let active = self.active;
        if hand.pinched == active {
            self.candidate = None;
        } else {
            let count = self.candidate.filter(|(state, _)| *state == hand.pinched)
                .map_or(1, |(_, count)| count.saturating_add(1));
            self.candidate = Some((hand.pinched, count));
            if count >= 2 {
                self.active = hand.pinched;
                self.candidate = None;
            }
        }
        Some(match (active, self.active) {
            (false, false) => PointerGesture::Hover(hand.pointer),
            (false, true) => PointerGesture::Begin(hand.pointer),
            (true, true) => PointerGesture::Update(hand.pointer),
            (true, false) => PointerGesture::End(hand.pointer),
        })
    }
}

pub struct HandTracker {
    palm: Model,
    hand: Model,
    face: Model,
}

impl HandTracker {
    pub fn load(directory: &Path) -> Result<Self, String> {
        Ok(Self {
            palm: load(&directory.join("palm_detection_mediapipe_2023feb.onnx"))?,
            hand: load(&directory.join("handpose_estimation_mediapipe_2023feb.onnx"))?,
            face: load(&directory.join("face_detection_yunet_2023mar.onnx"))?,
        })
    }

    pub fn head(&self, image: &RgbImage) -> Result<Option<Point>, String> {
        if image.width() == 0 || image.height() == 0 {
            return Err("Empty camera frame".into());
        }
        let resized = image::imageops::resize(image, 320, 320, image::imageops::FilterType::Triangle);
        let mut pixels = vec![0.0f32; 3 * 320 * 320];
        for (index, pixel) in resized.pixels().enumerate() {
            pixels[index] = f32::from(pixel[2]);
            pixels[320 * 320 + index] = f32::from(pixel[1]);
            pixels[2 * 320 * 320 + index] = f32::from(pixel[0]);
        }
        let input = Tensor::from_shape(&[1, 3, 320, 320], &pixels).map_err(|error| error.to_string())?;
        let output = self.face.run(tvec!(input.into())).map_err(|error| error.to_string())?;
        let values: Vec<Vec<f32>> = output.iter().map(|value| {
            value.try_as_plain_ram().and_then(|value| value.as_slice::<f32>().map(|slice| slice.to_vec())).map_err(|error| error.to_string())
        }).collect::<Result<_, _>>()?;
        decode_head(&values)
    }

    pub fn open_hand(&self, image: &RgbImage) -> Result<Option<Point>, String> {
        Ok(self.open_hands(image)?.into_iter().next())
    }

    pub fn open_hands(&self, image: &RgbImage) -> Result<Vec<Point>, String> {
        Ok(self.observe_hands(image)?.into_iter().filter(|hand| !hand.pinched).map(|hand| hand.center).collect())
    }

    pub fn observe_hands(&self, image: &RgbImage) -> Result<Vec<HandObservation>, String> {
        if image.width() == 0 || image.height() == 0 {
            return Err("Empty camera frame".into());
        }
        let side = image.width().max(image.height()) as f32;
        let center = [image.width() as f32 * 0.5, image.height() as f32 * 0.5];
        let outputs = run(&self.palm, crop(image, center, [1.0, 0.0], side, 192), 192)?;
        if outputs.len() != 2 || outputs[0].len() != 2016 * 18 || outputs[1].len() != 2016 {
            return Err("Unexpected palm model outputs".into());
        }
        let mut candidates: Vec<_> = outputs[1]
            .iter()
            .enumerate()
            .filter(|(_, score)| score.is_finite() && **score > 0.8473)
            .collect();
        candidates.sort_by(|(_, a), (_, b)| b.total_cmp(a));
        let mut regions: Vec<(Point, f32)> = Vec::new();
        let mut hands = Vec::new();
        for (index, _) in candidates {
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
                continue;
            };
            if regions.iter().any(|(other, size)| {
                (center[0] - other[0]).hypot(center[1] - other[1]) < side.min(*size) * 0.35
            }) {
                continue;
            }
            regions.push((center, side));
            if regions.len() > 8 {
                break;
            }
            let outputs = run(&self.hand, crop(image, center, right, side, 224), 224)?;
            if outputs.len() != 4 || outputs[0].len() != 63 || outputs[1].len() != 1 || outputs[2].len() != 1 {
                return Err("Unexpected hand model outputs".into());
            }
            if !outputs[1][0].is_finite() || outputs[1][0] < 0.8 {
                continue;
            }
            let mut landmarks = [[0.0; 3]; 21];
            for (landmark, values) in landmarks.iter_mut().zip(outputs[0].chunks_exact(3)) {
                landmark.copy_from_slice(values);
            }
            let pinched = is_pinched(&landmarks);
            if !(is_open(&landmarks) || pinched) || !palm_facing_camera(&landmarks, outputs[2][0]) {
                continue;
            }
            let palm_center = [0, 5, 9, 13, 17].map(|i| landmarks[i]);
            let x = palm_center.iter().map(|p| p[0]).sum::<f32>() / 5.0;
            let y = palm_center.iter().map(|p| p[1]).sum::<f32>() / 5.0;
            let down = [-right[1], right[0]];
            let map = |x: f32, y: f32| [
                (center[0] + (x / 224.0 - 0.5) * side * right[0] + (y / 224.0 - 0.5) * side * down[0]) / image.width() as f32,
                (center[1] + (x / 224.0 - 0.5) * side * right[1] + (y / 224.0 - 0.5) * side * down[1]) / image.height() as f32,
            ];
            let point = map(x, y);
            let pointer = map(landmarks[8][0], landmarks[8][1]);
            if point.iter().chain(pointer.iter()).all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                hands.push(HandObservation { center: point, pointer, pinched });
                if hands.len() == 2 {
                    break;
                }
            }
        }
        Ok(hands)
    }
}

fn decode_head(values: &[Vec<f32>]) -> Result<Option<Point>, String> {
    if values.len() != 12 { return Err("Unexpected face model outputs".into()); }
    let mut best: Option<(f32, Point)> = None;
    for (level, stride) in [8usize, 16, 32].into_iter().enumerate() {
        let grid = 320 / stride;
        let count = grid * grid;
        if values[level].len() != count || values[level + 3].len() != count || values[level + 6].len() != count * 4 {
            return Err("Unexpected face model output shapes".into());
        }
        for index in 0..count {
            let cls = values[level][index];
            let object = values[level + 3][index];
            if !cls.is_finite() || !object.is_finite() { continue; }
            let score = (cls.clamp(0.0, 1.0) * object.clamp(0.0, 1.0)).sqrt();
            if score < 0.85 || best.as_ref().is_some_and(|(old, _)| *old >= score) { continue; }
            let bbox = &values[level + 6][index * 4..index * 4 + 4];
            let point = [((index % grid) as f32 + bbox[0]) * stride as f32 / 320.0, ((index / grid) as f32 + bbox[1]) * stride as f32 / 320.0];
            if point.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) {
                best = Some((score, point));
            }
        }
    }
    Ok(best.map(|(_, point)| point))
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

fn palm_facing_camera(points: &[[f32; 3]; 21], handedness: f32) -> bool {
    if !handedness.is_finite() || !(0.0..=1.0).contains(&handedness)
        || (0.2..0.8).contains(&handedness)
        || points.iter().flatten().any(|value| !value.is_finite())
    {
        return false;
    }
    let index = vector(points[0], points[5]);
    let little = vector(points[0], points[17]);
    let cross = index[0] * little[1] - index[1] * little[0];
    let length = norm(index) * norm(little);
    length > 1.0 && cross.abs() / length > 0.25 && cross * (handedness - 0.5) > 0.0
}

fn is_pinched(points: &[[f32; 3]; 21]) -> bool {
    if points.iter().flatten().any(|value| !value.is_finite()) { return false; }
    let width = norm(vector(points[5], points[17]));
    width > 1.0
        && norm(vector(points[4], points[8])) < width * 0.35
        && [9, 13, 17].into_iter().all(|base| {
            let proximal = vector(points[base], points[base + 1]);
            let distal = vector(points[base + 1], points[base + 3]);
            let length = norm(proximal) * norm(distal);
            length > 1.0 && dot(proximal, distal) / length > 0.75
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
pub struct HeadRelativePan {
    previous: Option<(Point, f64)>,
}

impl HeadRelativePan {
    pub fn reset(&mut self) {
        self.previous = None;
    }

    pub fn update(&mut self, head: Option<Point>, hands: &[Point], seconds: f64) -> Point {
        let valid = |point: &Point| {
            point
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        };
        let Some(head) = head.filter(valid) else {
            self.reset();
            return [0.0; 2];
        };
        let Some(hand) = hands
            .iter()
            .filter(|point| valid(point))
            .max_by(|a, b| a[0].total_cmp(&b[0]))
        else {
            self.reset();
            return [0.0; 2];
        };
        if !seconds.is_finite() {
            self.reset();
            return [0.0; 2];
        }
        let axis = |v: f32| {
            let magnitude = ((v.abs() - 0.08) / 0.25).clamp(0.0, 1.0);
            v.signum() * magnitude
        };
        let target = [axis(hand[0] - head[0]), axis(hand[1] - head[1])];
        let Some((previous, timestamp)) = self.previous else {
            self.previous = Some((target, seconds));
            return [0.0; 2];
        };
        let dt = seconds - timestamp;
        if dt <= 0.0 || dt > 0.35 {
            self.reset();
            return [0.0; 2];
        }
        let alpha = (1.0 - (-dt / 0.09).exp()) as f32;
        let filtered = [
            previous[0] + (target[0] - previous[0]) * alpha,
            previous[1] + (target[1] - previous[1]) * alpha,
        ];
        self.previous = Some((filtered, seconds));
        [filtered[0] * dt as f32 * 0.6, filtered[1] * dt as f32 * 0.6]
    }
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
