use bevy_math::{Affine2, Rect, Vec2};

pub(crate) fn visible(transform: Affine2, center: Vec2, size: Vec2, clip: Option<Rect>) -> bool {
    let Some(clip) = clip else {
        return true;
    };
    if transform.matrix2.x_axis.y != 0.0 || transform.matrix2.y_axis.x != 0.0 {
        return true;
    }
    let half = size * 0.5;
    let a = transform.transform_point2(center - half);
    let b = transform.transform_point2(center + half);
    if !a.is_finite() || !b.is_finite() || !clip.min.is_finite() || !clip.max.is_finite() {
        return true;
    }
    let padding = a
        .abs()
        .max(b.abs())
        .max(clip.min.abs())
        .max(clip.max.abs())
        .max(Vec2::ONE)
        * (32.0 * f32::EPSILON);
    !(a.max(b).cmplt(clip.min - padding).any() || a.min(b).cmpgt(clip.max + padding).any())
}
