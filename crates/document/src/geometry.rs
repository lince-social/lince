//! Conversions between normalized fractions, page units, and pixel extents.

use crate::{Layout, Result};

pub(crate) const MAX_PDF_HEIGHT: u32 = 8192;
pub(crate) const PDF_PAGE_TOO_LARGE: &str = "PDF page dimensions exceed the rendering limit";

/// Scales a validated `0.0..=1.0` fraction to pixels, rounding down.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Callers pass a validated finite fraction in 0.0..=1.0, so the product is finite and \
              nonnegative; `as` truncates toward zero, and callers clamp the result to the extent"
)]
pub(crate) fn scale_floor(fraction: f32, extent: u32) -> u32 {
    (fraction * extent as f32) as u32
}

/// Scales a validated `0.0..=1.0` fraction to pixels, rounding up.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Callers pass a validated finite fraction in 0.0..=1.0, so the rounded product is \
              finite and nonnegative, and callers clamp the result to the extent"
)]
pub(crate) fn scale_ceil(fraction: f32, extent: u32) -> u32 {
    (fraction * extent as f32).ceil() as u32
}

/// Converts a rounded page height to pixels, rejecting values outside `1..=max_height`.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Every `max_height` passed here is exact in f32, and `height` is checked to be finite \
              and within 1.0..=max_height before the conversion"
)]
pub(crate) fn bounded_page_height(
    height: f32,
    max_height: u32,
    error: &'static str,
) -> Result<u32> {
    if height.is_finite() && (1.0..=max_height as f32).contains(&height) {
        Ok(height as u32)
    } else {
        Err(error.into())
    }
}

/// Lays out a PDF page of `page_width` by `page_height` points at a pixel `width`.
pub(crate) fn pdf_layout(page_width: f32, page_height: f32, width: u32) -> Result<Layout> {
    let scaled_width = u16::try_from(width).map_err(|_| PDF_PAGE_TOO_LARGE)?;
    let height = (page_height * f32::from(scaled_width) / page_width).ceil();
    let height = bounded_page_height(height, MAX_PDF_HEIGHT, PDF_PAGE_TOO_LARGE)?;
    Ok(Layout { width, height })
}
