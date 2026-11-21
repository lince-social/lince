use glam::{DMat3, DVec2, DVec3};

use super::{Quad, Surface, text, valid_id};
use crate::canvas::{Pose, room};

const MIN_CONVEX_CROSS: f64 = 1e-6;
const MIN_DETERMINANT: f64 = 1e-10;
const MIN_CORNER_WEIGHT: f64 = 1e-6;
const MIN_POINT_WEIGHT: f64 = 1e-8;
const SURFACE_TOLERANCE: f64 = 1e-8;

fn normalized(point: &[f64; 2]) -> bool {
    point.iter().all(|value| (0.0..=1.0).contains(value))
}

impl Default for Quad {
    fn default() -> Self {
        Self([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    }
}

impl Quad {
    /// Computes the projective transform from normalized surface coordinates.
    ///
    /// # Errors
    ///
    /// Returns the first coordinate, corner order, or singularity diagnostic.
    pub fn matrix(&self) -> Result<DMat3, String> {
        if !self.0.iter().all(normalized) {
            return Err("Calibration corners must be finite normalized coordinates.".into());
        }
        let [a, b, c, d] = self.0.map(DVec2::from_array);
        let [ab, bc, cd, da] = [b - a, c - b, d - c, a - d];
        if [(ab, bc), (bc, cd), (cd, da), (da, ab)]
            .into_iter()
            .any(|(edge, next)| edge.perp_dot(next) <= MIN_CONVEX_CROSS)
        {
            return Err("Use four distinct convex corners in top-left, top-right, bottom-right, bottom-left order.".into());
        }
        let delta = a - b + c - d;
        let u = b - c;
        let v = d - c;
        let denominator = u.perp_dot(v);
        if denominator.abs() < MIN_DETERMINANT {
            return Err("The calibration is degenerate.".into());
        }
        let g = delta.perp_dot(v) / denominator;
        let h = u.perp_dot(delta) / denominator;
        let matrix = DMat3::from_cols(
            (ab + g * b).extend(g),
            (d - a + h * d).extend(h),
            a.extend(1.0),
        );
        if !matrix.is_finite()
            || matrix.determinant().abs() < MIN_DETERMINANT
            || [1.0, 1.0 + g, 1.0 + h, 1.0 + g + h]
                .iter()
                .any(|weight| *weight < MIN_CORNER_WEIGHT)
        {
            return Err("The calibration crosses a projective singularity.".into());
        }
        Ok(matrix)
    }

    /// Maps a normalized point through the calibration. Set `inverse` to map back to the surface.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for an invalid input, calibration, or mapped point.
    pub fn map(&self, point: [f64; 2], inverse: bool) -> Result<[f64; 2], String> {
        if !normalized(&point) {
            return Err("The point must be finite.".into());
        }
        let [x, y] = point;
        let matrix = self.matrix()?;
        let value = if inverse { matrix.inverse() } else { matrix } * DVec3::new(x, y, 1.0);
        if !value.is_finite() || value.z <= MIN_POINT_WEIGHT {
            return Err("The point is outside the calibrated plane.".into());
        }
        let output = (value.truncate() / value.z).to_array();
        if output
            .iter()
            .any(|value| *value < -SURFACE_TOLERANCE || *value > 1.0 + SURFACE_TOLERANCE)
        {
            return Err("The point is outside the calibrated surface.".into());
        }
        Ok(output.map(|value| value.clamp(0.0, 1.0)))
    }
}

impl Surface {
    /// Checks the surface identifiers, physical pose, dimensions, and calibrations.
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && text(&self.name, 128)
            && self.workspace > 0
            && valid_id(&self.projector)
            && self.camera.as_deref().is_none_or(valid_id)
            && room::valid_pose(&self.physical_pose_meters)
            && self
                .size_meters
                .iter()
                .all(|value| (0.01..=100.0).contains(value))
            && self.calibration_revision > 0
            && self.projector_quad.matrix().is_ok()
            && self.camera_quad.matrix().is_ok()
    }

    /// Resolves a normalized local point into physical meters.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for an invalid surface, local coordinate, or resulting pose.
    pub fn physical_point(&self, local: [f64; 2]) -> Result<[f64; 3], String> {
        if !self.valid() || !normalized(&local) {
            return Err("Invalid surface or local coordinate.".into());
        }
        let [x, y] = local;
        let [width, height] = self.size_meters;
        let point = room::compose(
            &self.physical_pose_meters,
            &Pose {
                position: [x * width, y * height, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
        );
        if !room::valid_pose(&point) {
            return Err("The physical point exceeds finite bounds.".into());
        }
        Ok(point.position)
    }
}
