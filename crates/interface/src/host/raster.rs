use super::Rectangle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterMapping {
    pub origin: [f64; 2],
    pub scale: [f64; 2],
}

impl Default for RasterMapping {
    fn default() -> Self {
        Self {
            origin: [0.0; 2],
            scale: [1.0; 2],
        }
    }
}

impl RasterMapping {
    pub fn point(self, point: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|axis| (point[axis] - self.origin[axis]) * self.scale[axis])
    }

    pub fn logical_point(self, point: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|axis| point[axis] / self.scale[axis] + self.origin[axis])
    }

    pub fn map_rectangle(self, rectangle: Rectangle) -> Option<Rectangle> {
        Rectangle::new(
            self.point(rectangle.origin()),
            std::array::from_fn(|axis| rectangle.size()[axis] * self.scale[axis]),
        )
    }

    pub fn logical_rectangle(self, rectangle: Rectangle) -> Option<Rectangle> {
        Rectangle::new(
            self.logical_point(rectangle.origin()),
            std::array::from_fn(|axis| rectangle.size()[axis] / self.scale[axis]),
        )
    }

    pub fn pixel_region(self, rectangle: Rectangle) -> Option<([i32; 2], [u32; 2])> {
        let low = self.point(rectangle.origin()).map(f64::round);
        let high = self
            .point(std::array::from_fn(|axis| {
                rectangle.origin()[axis] + rectangle.size()[axis]
            }))
            .map(f64::round);
        if low
            .into_iter()
            .chain(high)
            .any(|value| !value.is_finite() || value.abs() > 1_048_576.0)
            || (0..2).any(|axis| high[axis] <= low[axis])
        {
            return None;
        }
        Some((
            low.map(|value| value as i32),
            std::array::from_fn(|axis| (high[axis] - low[axis]) as u32),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_bounds_and_odd_physical_modes_round_trip_without_reconstructing_nominal_scale() {
        let bounds = Rectangle::new([1025.0, 40.0], [1025.0, 577.0]).unwrap();
        let mapping = RasterMapping {
            origin: bounds.origin(),
            scale: [1.25; 2],
        };
        assert_eq!(mapping.point([1025.0, 40.0]), [0.0; 2]);
        assert_eq!(mapping.point([2050.0, 617.0]), [1281.25, 721.25]);
        assert_eq!(mapping.pixel_region(bounds), Some(([0, 0], [1281, 721])));
        for point in [[1025.0, 40.0], [1432.25, 450.5], [2050.0, 617.0]] {
            let restored = mapping.logical_point(mapping.point(point));
            assert!((0..2).all(|axis| (point[axis] - restored[axis]).abs() < 1e-10));
        }
        assert_eq!(
            mapping.logical_rectangle(mapping.map_rectangle(bounds).unwrap()),
            Some(bounds)
        );
    }

    #[test]
    fn adjacent_logical_regions_share_the_same_physical_boundary() {
        let bounds = Rectangle::new([500.0, 200.0], [853.0, 481.0]).unwrap();
        let mapping = RasterMapping {
            origin: bounds.origin(),
            scale: [1.5; 2],
        };
        let left = mapping
            .pixel_region(Rectangle::new([500.0, 200.0], [301.0, 481.0]).unwrap())
            .unwrap();
        let right = mapping
            .pixel_region(Rectangle::new([801.0, 200.0], [552.0, 481.0]).unwrap())
            .unwrap();
        assert_eq!(left.0[0] + left.1[0] as i32, right.0[0]);
        assert_eq!(left.1[0] + right.1[0], 1280);
        assert_eq!(right.0[1], 0);
        assert_eq!(right.1[1], 722);
        assert!(
            mapping
                .pixel_region(Rectangle::new([500.0, 200.0], [0.0001; 2]).unwrap())
                .is_none()
        );
    }

    #[test]
    fn output_regions_preserve_nominal_scale_and_clip_rounded_edges_to_the_real_mode() {
        for scale in [0.5, 1.0, 1.25, 1.5, 2.0, 4.0] {
            for pixels in [[801_u32, 601], [803, 603]] {
                let bounds = Rectangle::new(
                    [1000.0, 50.0],
                    pixels.map(|value| (f64::from(value) / scale).round()),
                )
                .unwrap();
                let output = super::super::OutputViewport { rotation: Default::default(),
                    id: super::super::OutputId(1),
                    bounds,
                    usable: Some(bounds),
                    physical_size: pixels,
                    raster: RasterMapping {
                        origin: bounds.origin(),
                        scale: [scale; 2],
                    },
                    workspace: 1,
                };
                assert!(output.valid_raster());
                assert_eq!(output.physical_region(bounds), Some(([0; 2], pixels)));
                let usable =
                    Rectangle::new([1000.0, 83.0], [bounds.size()[0], bounds.size()[1] - 33.0])
                        .unwrap();
                let (origin, size) = output.physical_region(usable).unwrap();
                assert_eq!(origin[1], (33.0 * scale).round() as i32);
                assert_eq!(origin[1] as u32 + size[1], pixels[1]);
                assert_eq!(size[0], pixels[0]);
                let cropped = RasterMapping {
                    origin: output.raster.logical_point(origin.map(f64::from)),
                    scale: [scale; 2],
                };
                let point = [1075.0, 90.0];
                let restored = cropped.logical_point(cropped.point(point));
                assert!((0..2).all(|axis| (point[axis] - restored[axis]).abs() < 1e-9));
            }
        }
    }
}
