mod epub;
mod pdf;
mod resources;

use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

pub const TILE_HEIGHT: u32 = 512;
pub const TILE_OVERLAP: u32 = 2;
pub const MAX_WIDTH: u32 = 2048;
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Scroll,
    Pages,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub section: usize,
    pub fraction: f32,
    pub mode: Mode,
}

impl Position {
    pub fn valid(&self) -> bool {
        self.section < 100_000 && self.fraction.is_finite() && (0.0..=1.0).contains(&self.fraction)
    }

    pub fn offset(&self, height: f32, viewport: f32) -> f32 {
        self.fraction * (height - viewport).max(0.0)
    }

    pub fn set_offset(&mut self, offset: f32, height: f32, viewport: f32) {
        let extent = (height - viewport).max(0.0);
        self.fraction = if extent > 0.0 {
            (offset / extent).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

#[derive(Clone, Debug)]
pub struct Info {
    pub title: String,
    pub sections: Vec<String>,
    pub is_pdf: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct Tile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

enum Content {
    Pdf(pdf::Pdf),
    Epub(epub::Epub),
}

pub struct Reader {
    content: Content,
    pub info: Info,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("Choose a PDF or EPUB file".into());
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err("Documents are limited to 256 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Documents are limited to 256 MiB".into());
        }
        Self::from_bytes(
            bytes,
            &path.file_name().unwrap_or_default().to_string_lossy(),
        )
    }

    pub fn from_bytes(bytes: Vec<u8>, title: &str) -> Result<Self> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Documents are limited to 256 MiB".into());
        }
        if bytes.starts_with(b"%PDF-") {
            let pdf = pdf::Pdf::new(bytes)?;
            let sections = (1..=pdf.len()).map(|page| format!("Page {page}")).collect();
            Ok(Self {
                content: Content::Pdf(pdf),
                info: Info {
                    title: title.into(),
                    sections,
                    is_pdf: true,
                },
            })
        } else if bytes.starts_with(b"PK") {
            let epub = epub::Epub::new(bytes)?;
            let info = Info {
                title: title.into(),
                sections: epub.sections.clone(),
                is_pdf: false,
            };
            Ok(Self {
                content: Content::Epub(epub),
                info,
            })
        } else {
            Err("This file is not a PDF or EPUB".into())
        }
    }

    pub fn layout(&mut self, section: usize, width: u32) -> Result<Layout> {
        if section >= self.info.sections.len() || !(128..=MAX_WIDTH).contains(&width) {
            return Err("Invalid document section or render width".into());
        }
        match &mut self.content {
            Content::Pdf(pdf) => pdf.layout(section, width),
            Content::Epub(epub) => epub.layout(section, width),
        }
    }

    pub fn tile(&mut self, section: usize, width: u32, index: u32) -> Result<Tile> {
        let layout = self.layout(section, width)?;
        let top = index
            .checked_mul(TILE_HEIGHT)
            .filter(|top| *top < layout.height)
            .ok_or("Tile is outside the document")?;
        let height = (TILE_HEIGHT + TILE_OVERLAP).min(layout.height - top);
        match &mut self.content {
            Content::Pdf(pdf) => pdf.tile(section, layout, top, height),
            Content::Epub(epub) => epub.tile(layout, top, height),
        }
    }
}

#[cfg(test)]
mod tests;
