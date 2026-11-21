// non-lean-yet

use super::{OutputId, Rectangle, from_fn};
use crate::{mapping, raster};

#[derive(Clone, Debug, PartialEq)]
pub struct OutputViewport {
    pub id: OutputId,
    pub bounds: Rectangle,
    pub usable: Option<Rectangle>,
    pub physical_size: [u32; 2],
    pub rotation: mapping::QuarterTurn,
    pub raster: raster::RasterMapping,
    pub workspace: u64,
}

impl OutputViewport {
    pub fn source_size(&self) -> [u32; 2] {
        self.rotation.source_size(self.physical_size)
    }

    pub fn valid_raster(&self) -> bool {
        self.raster
            .origin
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 1_048_576.0)
            && self
                .raster
                .scale
                .iter()
                .all(|value| value.is_finite() && (0.5..=4.0).contains(value))
            && self.raster.scale[0] == self.raster.scale[1]
    }

    pub fn raster_mapping(&self) -> raster::RasterMapping {
        self.raster
    }

    pub fn physical_region(&self, rectangle: Rectangle) -> Option<([i32; 2], [u32; 2])> {
        let (origin, size) = self.raster_mapping().pixel_region(rectangle)?;
        let limit = self.source_size();
        let low = from_fn(|axis| {
            if rectangle.origin()[axis] <= self.bounds.origin()[axis] {
                0
            } else {
                origin[axis].max(0).min(limit[axis] as i32)
            }
        });
        let high: [i32; 2] = from_fn(|axis| {
            if rectangle.origin()[axis] + rectangle.size()[axis]
                >= self.bounds.origin()[axis] + self.bounds.size()[axis]
            {
                limit[axis] as i32
            } else {
                (i64::from(origin[axis]) + i64::from(size[axis])).clamp(0, i64::from(limit[axis]))
                    as i32
            }
        });
        (0..2)
            .all(|axis| high[axis] > low[axis])
            .then(|| (low, from_fn(|axis| (high[axis] - low[axis]) as u32)))
    }
}
