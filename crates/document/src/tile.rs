//! RGBA8 pixel buffers and the checked offset arithmetic used to cut them.

use lince_resources::Locator;

use crate::{
    Result,
    geometry::{scale_ceil, scale_floor},
};

const RGBA_BYTES_PER_PIXEL: usize = 4;
const INVALID_DIMENSIONS: &str = "Invalid image dimensions";

/// Rendered RGBA8 pixel buffer for a tile or cropped excerpt.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Tightly packed row-major RGBA8 bytes (`width * height * 4`).
    pub rgba: Vec<u8>,
}

/// A pixel rectangle inside a [`Tile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PixelRect {
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Tile {
    /// Crops the tile to a normalized `[left, top, width, height]` region.
    ///
    /// The region's origin rounds down and its size rounds up, so every
    /// partially covered pixel is kept. The pixel buffer is reused.
    ///
    /// # Errors
    ///
    /// Returns an error if `region` fails [`Locator::validate`], if the tile
    /// has zero dimensions, or if `rgba.len()` is not `width * height * 4`.
    pub fn crop(self, region: [f32; 4]) -> Result<Self> {
        Locator {
            region: Some(region),
            ..Default::default()
        }
        .validate()?;
        if self.width == 0 || self.height == 0 {
            return Err(INVALID_DIMENSIONS.into());
        }
        let left = scale_floor(region[0], self.width).min(self.width - 1);
        let top = scale_floor(region[1], self.height).min(self.height - 1);
        let rect = PixelRect {
            left,
            top,
            width: scale_ceil(region[2], self.width)
                .max(1)
                .min(self.width - left),
            height: scale_ceil(region[3], self.height)
                .max(1)
                .min(self.height - top),
        };
        self.into_rect(rect)
            .ok_or_else(|| INVALID_DIMENSIONS.into())
    }

    /// Keeps only `rect`, compacting its rows to the front of the existing buffer.
    ///
    /// Returns `None` if the buffer length does not match the dimensions or
    /// `rect` reaches outside them.
    pub(crate) fn into_rect(mut self, rect: PixelRect) -> Option<Self> {
        let stride = rgba_bytes(self.width)?;
        if stride.checked_mul(to_usize(self.height)?)? != self.rgba.len()
            || rect.left.checked_add(rect.width)? > self.width
            || rect.top.checked_add(rect.height)? > self.height
        {
            return None;
        }
        // Every offset below stays within `rgba.len()`, which was just checked
        // to equal `stride * height`, so none of this arithmetic can overflow.
        let row_bytes = rgba_bytes(rect.width)?;
        let first = to_usize(rect.top)? * stride + rgba_bytes(rect.left)?;
        let len = row_bytes * to_usize(rect.height)?;
        if row_bytes == stride {
            self.rgba.copy_within(first..first + len, 0);
        } else {
            // Rows move toward the front and never overtake an unread source row,
            // because `row_bytes <= stride`.
            for row in 0..to_usize(rect.height)? {
                let source = first + row * stride;
                self.rgba
                    .copy_within(source..source + row_bytes, row * row_bytes);
            }
        }
        self.rgba.truncate(len);
        self.rgba.shrink_to_fit();
        Some(Self {
            width: rect.width,
            height: rect.height,
            rgba: self.rgba,
        })
    }

    /// Copies rows `top..top + height` of a `width`-pixel-wide RGBA buffer.
    pub(crate) fn from_rows(pixels: &[u8], width: u32, top: u32, height: u32) -> Option<Self> {
        let stride = rgba_bytes(width)?;
        let start = to_usize(top)?.checked_mul(stride)?;
        let end = to_usize(top.checked_add(height)?)?.checked_mul(stride)?;
        Some(Self {
            width,
            height,
            rgba: pixels.get(start..end)?.to_vec(),
        })
    }
}

/// Returns the length of a `width` by `height` RGBA8 buffer, or `None` on overflow.
pub(crate) fn rgba_byte_len(width: u32, height: u32) -> Option<usize> {
    rgba_bytes(width)?.checked_mul(to_usize(height)?)
}

fn rgba_bytes(pixels: u32) -> Option<usize> {
    to_usize(pixels)?.checked_mul(RGBA_BYTES_PER_PIXEL)
}

fn to_usize(value: u32) -> Option<usize> {
    usize::try_from(value).ok()
}
