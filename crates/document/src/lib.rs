//! Offline and range-backed PDF and EPUB document rendering for Lince.
//!
//! [`Reader`] opens local files, in-memory byte buffers, or
//! [`Resource`](lince_resources::Resource) sources and renders bounded RGBA
//! [`Tile`] slices and excerpts for display.

mod epub;
mod geometry;
mod pdf;
mod pdf_online;
mod reader;
mod resources;
mod tile;

use serde::{Deserialize, Serialize};

pub use reader::Reader;
pub use tile::Tile;

/// Height of each rendered vertical tile in pixels.
pub const TILE_HEIGHT: u32 = 512;
/// Extra pixels rendered below each non-terminal tile to prevent seam artifacts.
pub const TILE_OVERLAP: u32 = 2;
/// Maximum supported render width in pixels.
pub const MAX_WIDTH: u32 = 2048;
/// Maximum size in bytes for buffered document loading.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Result type returned by document loading and rendering operations.
pub type Result<T> = std::result::Result<T, String>;

const MAX_SECTIONS: usize = 100_000;

/// Reading layout mode stored alongside a [`Position`].
///
/// The set of modes is closed and persisted, so the enum is deliberately exhaustive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Continuous vertical scrolling through a section.
    #[default]
    Scroll,
    /// Discrete page-by-page navigation.
    Pages,
}

/// Persisted reading position within a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based chapter or page index.
    pub section: usize,
    /// Normalized vertical scroll progress in `0.0..=1.0`.
    pub fraction: f32,
    /// Active reading presentation mode.
    pub mode: Mode,
}

impl Position {
    /// Returns whether the section index and normalized scroll fraction are within valid bounds.
    pub fn valid(&self) -> bool {
        self.section < MAX_SECTIONS
            && self.fraction.is_finite()
            && (0.0..=1.0).contains(&self.fraction)
    }

    /// Converts the normalized scroll fraction to a pixel offset for the given content and viewport heights.
    pub fn offset(&self, height: f32, viewport: f32) -> f32 {
        self.fraction * (height - viewport).max(0.0)
    }

    /// Updates [`Self::fraction`] from a pixel scroll offset for the given content and viewport heights.
    pub fn set_offset(&mut self, offset: f32, height: f32, viewport: f32) {
        let extent = (height - viewport).max(0.0);
        self.fraction = if extent > 0.0 {
            (offset / extent).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

/// Static metadata discovered when opening a document.
#[derive(Clone, Debug)]
pub struct Info {
    /// Display title derived from the resource title or file name.
    pub title: String,
    /// Ordered chapter hrefs for EPUBs or display labels for PDF pages.
    pub sections: Vec<String>,
    /// The format of the opened document.
    pub format: Format,
}

impl Info {
    /// Returns whether the opened document is a PDF.
    pub fn is_pdf(&self) -> bool {
        self.format == Format::Pdf
    }
}

/// The format of an opened document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A PDF, whose sections are pages.
    Pdf,
    /// An EPUB, whose sections are chapters.
    Epub,
}

/// Computed pixel dimensions of a laid-out section at a given render width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Section width in pixels.
    pub width: u32,
    /// Section height in pixels.
    pub height: u32,
}

#[cfg(test)]
mod tests;
