//! Logical rectangles and transforms between canvas and surface coordinates.

use super::{ItemId, OutputId};
use std::array::from_fn;

mod viewport;
pub use viewport::OutputViewport;

/// Finite logical bounds with positive width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangle {
    origin: [f64; 2],
    size: [f64; 2],
}

impl Rectangle {
    /// Intersects the two rectangles, returning `None` for an empty overlap.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let low = from_fn(|axis| self.origin[axis].max(other.origin[axis]));
        let high: [f64; 2] = from_fn(|axis| {
            (self.origin[axis] + self.size[axis]).min(other.origin[axis] + other.size[axis])
        });
        Self::new(low, from_fn(|axis| high[axis] - low[axis]))
    }

    /// Rejects non-finite bounds or non-positive dimensions.
    pub fn new(origin: [f64; 2], size: [f64; 2]) -> Option<Self> {
        origin
            .iter()
            .zip(size)
            .all(|(origin, size)| size > 0.0 && (origin + size).is_finite())
            .then_some(Self { origin, size })
    }

    pub fn origin(self) -> [f64; 2] {
        self.origin
    }

    pub fn size(self) -> [f64; 2] {
        self.size
    }

    /// Tests containment with inclusive low edges and exclusive high edges.
    pub fn contains(self, point: [f64; 2]) -> bool {
        (0..2).all(|axis| {
            point[axis] >= self.origin[axis] && point[axis] < self.origin[axis] + self.size[axis]
        })
    }
}

/// Converts coordinates between a rendered surface and its canvas placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentMapping {
    pub canvas: Rectangle,
    pub surface: Rectangle,
    pub corners: Option<[[f64; 2]; 4]>,
}

/// Caches a transform for multiple points on the same surface.
#[derive(Clone, Copy, Debug)]
pub struct CanvasProjection {
    canvas: Rectangle,
    surface: Rectangle,
    matrix: Option<[[f64; 3]; 3]>,
}

impl CanvasProjection {
    pub fn surface(&self) -> Rectangle {
        self.surface
    }

    pub fn point(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        let uv =
            from_fn(|axis| (point[axis] - self.surface.origin[axis]) / self.surface.size[axis]);
        if let Some(matrix) = self.matrix {
            project(matrix, uv)
        } else {
            Some(from_fn(|axis| {
                self.canvas.origin[axis] + uv[axis] * self.canvas.size[axis]
            }))
        }
    }
}

impl ContentMapping {
    /// Maps a canvas point only if it lands inside the rendered surface.
    pub fn surface_point(self, canvas_point: [f64; 2]) -> Option<[f64; 2]> {
        if !self.canvas.contains(canvas_point) {
            return None;
        }
        let point = self.surface_point_unbounded(canvas_point)?;
        self.surface.contains(point).then_some(point)
    }

    /// Maps a canvas point without clipping it to the surface bounds.
    pub fn surface_point_unbounded(self, point: [f64; 2]) -> Option<[f64; 2]> {
        let uv = if let Some(corners) = self.corners {
            project(inverse(homography(corners)?)?, point)?
        } else {
            from_fn(|axis| (point[axis] - self.canvas.origin[axis]) / self.canvas.size[axis])
        };
        Some(from_fn(|axis| {
            self.surface.origin[axis] + uv[axis] * self.surface.size[axis]
        }))
    }

    pub fn canvas_point(self, point: [f64; 2]) -> Option<[f64; 2]> {
        self.canvas_projection()?.point(point)
    }

    pub fn canvas_projection(self) -> Option<CanvasProjection> {
        let matrix = match self.corners {
            Some(corners) => Some(homography(corners)?),
            None => None,
        };
        Some(CanvasProjection {
            canvas: self.canvas,
            surface: self.surface,
            matrix,
        })
    }

    pub fn translated(self, delta: [f64; 2]) -> Option<Self> {
        Some(Self {
            canvas: Rectangle::new(
                from_fn(|axis| self.canvas.origin[axis] + delta[axis]),
                self.canvas.size,
            )?,
            corners: self
                .corners
                .map(|corners| corners.map(|point| from_fn(|axis| point[axis] + delta[axis]))),
            ..self
        })
    }
}

fn project(matrix: [[f64; 3]; 3], [x, y]: [f64; 2]) -> Option<[f64; 2]> {
    let [u, v, w] = matrix.map(|[a, b, c]| a * x + b * y + c);
    ([u, v, w].into_iter().all(f64::is_finite) && w.abs() >= 1e-12).then(|| [u / w, v / w])
}

fn homography(corners: [[f64; 2]; 4]) -> Option<[[f64; 3]; 3]> {
    let [a, b, c, d] = corners;
    let dx = [b[0] - c[0], d[0] - c[0], a[0] - b[0] + c[0] - d[0]];
    let dy = [b[1] - c[1], d[1] - c[1], a[1] - b[1] + c[1] - d[1]];
    let denominator = dx[0] * dy[1] - dx[1] * dy[0];
    if !denominator.is_finite() || denominator.abs() < 1e-12 {
        return None;
    }
    let g = (dx[2] * dy[1] - dx[1] * dy[2]) / denominator;
    let h = (dx[0] * dy[2] - dx[2] * dy[0]) / denominator;
    Some([
        [b[0] - a[0] + g * b[0], d[0] - a[0] + h * d[0], a[0]],
        [b[1] - a[1] + g * b[1], d[1] - a[1] + h * d[1], a[1]],
        [g, h, 1.0],
    ])
}

fn inverse(matrix: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let [[a, b, c], [d, e, f], [g, h, i]] = matrix;
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    Some(
        [
            [e * i - f * h, c * h - b * i, b * f - c * e],
            [f * g - d * i, a * i - c * g, c * d - a * f],
            [d * h - e * g, b * g - a * h, a * e - b * d],
        ]
        .map(|row| row.map(|value| value / determinant)),
    )
}

/// Placement and input geometry for one item on an output.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemGeometry {
    pub id: ItemId,
    pub output: OutputId,
    pub mapping: ContentMapping,
    pub floating: bool,
    /// When present, input must fall within at least one convex clip polygon.
    pub clip: Option<Vec<Vec<[f64; 2]>>>,
}

impl ItemGeometry {
    pub fn surface_point(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        if self.clip.as_ref().is_some_and(|polygons| {
            !polygons.iter().any(|polygon| {
                polygon.len() >= 3
                    && polygon
                        .iter()
                        .zip(polygon.iter().cycle().skip(1))
                        .take(polygon.len())
                        .all(|(a, b)| {
                            (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
                                >= -1e-6
                        })
            })
        }) {
            return None;
        }
        self.mapping.surface_point(point)
    }
}

/// Output and item geometry published together under one revision.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometrySnapshot {
    pub revision: u64,
    pub outputs: Vec<OutputViewport>,
    pub items: Vec<ItemGeometry>,
    pub focused: Option<ItemId>,
}

#[cfg(test)]
mod tests;
