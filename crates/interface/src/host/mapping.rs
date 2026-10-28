use super::{OutputId, OutputViewport, Rectangle, raster::RasterMapping};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuarterTurn {
    #[default]
    Normal,
    Clockwise90,
    UpsideDown,
    Clockwise270,
}

impl QuarterTurn {
    pub fn swaps_axes(self) -> bool { matches!(self, Self::Clockwise90 | Self::Clockwise270) }

    pub fn source_size(self, physical: [u32; 2]) -> [u32; 2] {
        if self.swaps_axes() { [physical[1], physical[0]] } else { physical }
    }

    pub fn inverse(self) -> Self {
        match self { Self::Clockwise90 => Self::Clockwise270, Self::Clockwise270 => Self::Clockwise90, other => other }
    }

    pub fn point(self, point: [f64; 2], size: [u32; 2]) -> Option<[f64; 2]> {
        if point.iter().any(|value| !value.is_finite()) || size.contains(&0) { return None; }
        let [width, height] = size.map(f64::from);
        let [x, y] = point;
        Some(match self {
            Self::Normal => [x, y], Self::Clockwise90 => [height - y, x],
            Self::UpsideDown => [width - x, height - y], Self::Clockwise270 => [y, width - x],
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    revision: u64,
    output: OutputId,
    logical_bounds: Rectangle,
    physical_size: [u32; 2],
    raster: RasterMapping,
    rotation: QuarterTurn,
    magnification: f64,
    forward: [[f64; 3]; 2],
    inverse: [[f64; 3]; 2],
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    revision: u64,
    outputs: BTreeMap<OutputId, Mapping>,
}

impl Snapshot {
    pub fn new(revision: u64, outputs: impl IntoIterator<Item = Mapping>) -> Result<Self, &'static str> {
        if revision == 0 { return Err("An output mapping snapshot needs a nonzero revision."); }
        let mut mappings = BTreeMap::new();
        let mut pixels = 0_u64;
        for mapping in outputs {
            if mapping.revision > revision { return Err("An output mapping cannot be newer than its published snapshot."); }
            pixels = pixels.saturating_add(u64::from(mapping.physical_size[0]) * u64::from(mapping.physical_size[1]));
            let output = mapping.output;
            if mappings.insert(output, mapping).is_some() || mappings.len() > 16 || pixels > 64 * 1024 * 1024 {
                return Err("The output mapping snapshot has duplicate or excessive outputs.");
            }
        }
        Ok(Self { revision, outputs: mappings })
    }

    pub fn revision(&self) -> u64 { self.revision }

    pub fn outputs(&self) -> &BTreeMap<OutputId, Mapping> { &self.outputs }

    pub fn get(&self, output: OutputId) -> Option<&Mapping> { self.outputs.get(&output) }
}

impl Mapping {
    pub fn revision(&self) -> u64 { self.revision }

    pub fn output(&self) -> OutputId { self.output }

    pub fn logical_bounds(&self) -> Rectangle { self.logical_bounds }

    pub fn physical_size(&self) -> [u32; 2] { self.physical_size }

    pub fn raster(&self) -> RasterMapping { self.raster }

    pub fn rotation(&self) -> QuarterTurn { self.rotation }

    pub fn source_size(&self) -> [u32; 2] { self.rotation.source_size(self.physical_size) }

    pub fn magnification(&self) -> f64 { self.magnification }

    pub fn same_mapping(&self, other: &Self) -> bool {
        self.output == other.output && self.logical_bounds == other.logical_bounds && self.physical_size == other.physical_size
            && self.raster == other.raster && self.rotation == other.rotation && self.magnification == other.magnification
            && self.forward == other.forward && self.inverse == other.inverse
    }

    pub fn logical_to_displayed(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        self.physical_to_displayed(self.logical_to_physical(point)?)
    }

    pub fn displayed_to_logical(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        self.physical_to_logical(self.displayed_to_physical(point)?)
    }

    pub fn physical_to_displayed(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        let point = self.rotation.inverse().point(point, self.physical_size)?;
        Some(self.raster.logical_point(point))
    }

    pub fn displayed_to_physical(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        self.rotation.point(self.raster.point(point), self.source_size())
    }

    pub fn displayed_rectangle(&self, rectangle: Rectangle) -> Option<Rectangle> {
        self.physical_rectangle_to_displayed(self.map_rectangle(rectangle)?)
    }

    pub fn visible_displayed_rectangle(&self, rectangle: Rectangle) -> Option<Rectangle> {
        self.physical_rectangle_to_displayed(self.visible_rectangle(rectangle)?)
    }

    fn physical_rectangle_to_displayed(&self, physical: Rectangle) -> Option<Rectangle> {
        let origin = physical.origin();
        let end = std::array::from_fn(|axis| origin[axis] + physical.size()[axis]);
        let points = [origin, [end[0], origin[1]], end, [origin[0], end[1]]].map(|point| self.physical_to_displayed(point));
        let [Some(a), Some(b), Some(c), Some(d)] = points else { return None; };
        let points = [a, b, c, d];
        let low = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
        let high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
        Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
    }

    pub fn composition_raster(&self, target: RasterMapping) -> Option<RasterMapping> {
        if self.magnification == 1.0 { return Some(target); }
        let origin = target.point(self.logical_to_displayed([0.0; 2])?);
        let scale: [f64; 2] = std::array::from_fn(|axis| target.scale[axis] * self.magnification);
        if origin.iter().any(|value| !value.is_finite()) || scale.iter().any(|value| !value.is_finite() || *value <= 0.0) { return None; }
        Some(RasterMapping { origin: std::array::from_fn(|axis| -origin[axis] / scale[axis]), scale })
    }

    pub fn new(output: &OutputViewport, rotation: QuarterTurn, magnification: f64, anchor: [f64; 2], revision: u64) -> Result<Self, &'static str> {
        let source = rotation.source_size(output.physical_size).map(f64::from);
        if revision == 0 || !output.valid_raster() || output.rotation != rotation || output.physical_size.iter().any(|value| !(1..=16384).contains(value))
            || !magnification.is_finite() || !(1.0..=4.0).contains(&magnification)
            || anchor.iter().any(|value| !value.is_finite())
            || output.raster.scale.iter().any(|scale| !scale.is_finite() || !(0.5..=4.0).contains(scale))
            || output.raster.scale[0] != output.raster.scale[1]
            || output.raster.origin != output.bounds.origin()
            || (0..2).any(|axis| (output.bounds.size()[axis] * output.raster.scale[axis]).round() != source[axis])
        { return Err("Invalid final output mapping."); }
        let pivot = output.raster.point(anchor);
        let viewport = source.map(|size| size / magnification);
        let crop: [f64; 2] = std::array::from_fn(|axis| (pivot[axis] - viewport[axis] * 0.5).clamp(0.0, (source[axis] - viewport[axis]).max(0.0)));
        let base = [
            [output.raster.scale[0] * magnification, 0.0, (-output.raster.origin[0] * output.raster.scale[0] - crop[0]) * magnification],
            [0.0, output.raster.scale[1] * magnification, (-output.raster.origin[1] * output.raster.scale[1] - crop[1]) * magnification],
        ];
        let physical = output.physical_size.map(f64::from);
        let rotation_matrix = match rotation {
            QuarterTurn::Normal => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            QuarterTurn::Clockwise90 => [[0.0, -1.0, physical[0]], [1.0, 0.0, 0.0]],
            QuarterTurn::UpsideDown => [[-1.0, 0.0, physical[0]], [0.0, -1.0, physical[1]]],
            QuarterTurn::Clockwise270 => [[0.0, 1.0, 0.0], [-1.0, 0.0, physical[1]]],
        };
        let forward = compose(rotation_matrix, base);
        let inverse = invert(forward).ok_or("The final output mapping is singular or nonfinite.")?;
        Ok(Self { revision, output: output.id, logical_bounds: output.bounds, physical_size: output.physical_size,
            raster: output.raster, rotation, magnification, forward, inverse })
    }

    pub fn logical_to_physical(&self, point: [f64; 2]) -> Option<[f64; 2]> { point_in(self.forward, point) }

    pub fn physical_to_logical(&self, point: [f64; 2]) -> Option<[f64; 2]> { point_in(self.inverse, point) }

    pub fn forward(&self) -> [[f64; 3]; 2] { self.forward }

    pub fn inverse(&self) -> [[f64; 3]; 2] { self.inverse }

    pub fn map_rectangle(&self, rectangle: Rectangle) -> Option<Rectangle> { rectangle_in(self.forward, rectangle) }

    pub fn logical_rectangle(&self, rectangle: Rectangle) -> Option<Rectangle> { rectangle_in(self.inverse, rectangle) }

    pub fn visible_rectangle(&self, rectangle: Rectangle) -> Option<Rectangle> {
        let mapped = self.map_rectangle(rectangle)?;
        let low = mapped.origin().map(|value| value.max(0.0));
        let high: [f64; 2] = std::array::from_fn(|axis| (mapped.origin()[axis] + mapped.size()[axis]).min(f64::from(self.physical_size[axis])));
        Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
    }
}

fn compose(outer: [[f64; 3]; 2], inner: [[f64; 3]; 2]) -> [[f64; 3]; 2] {
    std::array::from_fn(|row| [
        outer[row][0] * inner[0][0] + outer[row][1] * inner[1][0],
        outer[row][0] * inner[0][1] + outer[row][1] * inner[1][1],
        outer[row][0] * inner[0][2] + outer[row][1] * inner[1][2] + outer[row][2],
    ])
}

fn invert(matrix: [[f64; 3]; 2]) -> Option<[[f64; 3]; 2]> {
    if matrix.iter().flatten().any(|value| !value.is_finite()) { return None; }
    let determinant = matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0];
    if !determinant.is_finite() || determinant.abs() < f64::EPSILON { return None; }
    let linear = [[matrix[1][1] / determinant, -matrix[0][1] / determinant], [-matrix[1][0] / determinant, matrix[0][0] / determinant]];
    let inverse = std::array::from_fn(|row| [linear[row][0], linear[row][1], -linear[row][0] * matrix[0][2] - linear[row][1] * matrix[1][2]]);
    inverse.iter().flatten().all(|value| value.is_finite()).then_some(inverse)
}

fn point_in(matrix: [[f64; 3]; 2], point: [f64; 2]) -> Option<[f64; 2]> {
    if point.iter().any(|value| !value.is_finite()) { return None; }
    let result = std::array::from_fn(|axis| matrix[axis][0] * point[0] + matrix[axis][1] * point[1] + matrix[axis][2]);
    result.iter().all(|value| value.is_finite()).then_some(result)
}

fn rectangle_in(matrix: [[f64; 3]; 2], rectangle: Rectangle) -> Option<Rectangle> {
    let low = rectangle.origin(); let high: [f64; 2] = std::array::from_fn(|axis| low[axis] + rectangle.size()[axis]);
    let points = [low, [high[0], low[1]], high, [low[0], high[1]]].map(|point| point_in(matrix, point));
    let points = points.into_iter().collect::<Option<Vec<_>>>()?;
    let low = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
    let high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
    Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
}

#[cfg(test)]
mod tests;
