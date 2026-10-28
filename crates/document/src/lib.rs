mod epub;
mod pdf;
mod pdf_online;
mod resources;

use lince_resources::{Library, Locator, Resource};
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

pub fn crop(tile: Tile, region: [f32; 4]) -> Result<Tile> {
    Locator {
        region: Some(region),
        ..Default::default()
    }
    .validate()?;
    let length = (tile.width as usize)
        .checked_mul(tile.height as usize)
        .and_then(|length| length.checked_mul(4))
        .ok_or("Invalid image dimensions")?;
    if tile.width == 0 || tile.height == 0 || tile.rgba.len() != length {
        return Err("Invalid image dimensions".into());
    }
    let left = ((region[0] * tile.width as f32) as u32).min(tile.width - 1);
    let top = ((region[1] * tile.height as f32) as u32).min(tile.height - 1);
    let width = ((region[2] * tile.width as f32).ceil() as u32)
        .max(1)
        .min(tile.width - left);
    let height = ((region[3] * tile.height as f32).ceil() as u32)
        .max(1)
        .min(tile.height - top);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in top..top + height {
        let offset = ((row * tile.width + left) * 4) as usize;
        rgba.extend_from_slice(&tile.rgba[offset..offset + width as usize * 4]);
    }
    Ok(Tile {
        width,
        height,
        rgba,
    })
}

enum Content {
    Pdf(pdf::Pdf),
    PdfOnline(pdf_online::Pdf),
    Epub(epub::Epub),
}

pub struct Reader {
    content: Content,
    pub info: Info,
}

impl Reader {
    pub fn open_resource(library: Library, resource: &Resource) -> Result<Self> {
        use std::io::{Seek, SeekFrom};
        let mut source = lince_resources::RangeReader::open(library.clone(), resource)?;
        let mut magic = [0; 4];
        source
            .read_exact(&mut magic)
            .map_err(|error| error.to_string())?;
        source
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        if magic.starts_with(b"PK") {
            let epub = epub::Epub::from_reader(source)?;
            let info = Info {
                title: resource.title.clone(),
                sections: epub.sections.clone(),
                is_pdf: false,
            };
            return Ok(Self {
                content: Content::Epub(epub),
                info,
            });
        }
        if magic == *b"%PDF"
            && let Ok(pdf) = pdf_online::Pdf::new(source)
        {
            let sections = (1..=pdf.len()).map(|page| format!("Page {page}")).collect();
            return Ok(Self {
                content: Content::PdfOnline(pdf),
                info: Info {
                    title: resource.title.clone(),
                    sections,
                    is_pdf: true,
                },
            });
        }
        let mut source = lince_resources::RangeReader::open(library, resource)?;
        if source.len() > MAX_FILE_BYTES {
            return Err("This renderer requires a document under 256 MiB".into());
        }
        let mut bytes = Vec::new();
        source
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if resource
            .hash
            .as_ref()
            .is_some_and(|hash| !hash.eq_ignore_ascii_case(&lince_resources::fingerprint(&bytes)))
        {
            return Err("The document does not match its content hash".into());
        }
        Self::from_bytes(bytes, &resource.title)
    }

    pub fn locate(&mut self, locator: &Locator, width: u32) -> Result<Position> {
        locator.validate()?;
        let section = if self.info.is_pdf {
            locator.page.unwrap_or(1) - 1
        } else if locator.href.is_empty() {
            0
        } else {
            self.info
                .sections
                .iter()
                .position(|href| {
                    href.trim_start_matches('/') == locator.href.trim_start_matches('/')
                        || href.ends_with(locator.href.trim_start_matches('/'))
                })
                .ok_or("The referenced chapter is not present in this edition")?
        };
        if self.info.is_pdf
            && locator
                .end_page
                .is_some_and(|end| end > self.info.sections.len())
        {
            return Err("The referenced page range is not present in this edition".into());
        }
        self.layout(section, width)?;
        let fraction = match &self.content {
            Content::Epub(epub) => {
                if locator.fragment.is_empty() && locator.quote.is_empty() {
                    0.0
                } else {
                    epub.anchor(&locator.fragment, &locator.quote)
                        .ok_or("The referenced passage is not present in this edition")?
                }
            }
            Content::Pdf(_) | Content::PdfOnline(_) => {
                locator.region.map_or(0.0, |region| region[1])
            }
        };
        Ok(Position {
            section,
            fraction,
            mode: Mode::Scroll,
        })
    }

    pub fn resource_bytes(&self, href: &str) -> Result<Vec<u8>> {
        match &self.content {
            Content::Epub(epub) => epub.resource(href),
            Content::Pdf(_) | Content::PdfOnline(_) => {
                Err("Choose a page region for a PDF image".into())
            }
        }
    }

    pub fn excerpt(&mut self, locator: &Locator, width: u32) -> Result<Tile> {
        if !locator.image.is_empty() {
            let bytes = self.resource_bytes(&locator.image)?;
            let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|error| error.to_string())?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            limits.max_alloc = Some(128 * 1024 * 1024);
            decoder.limits(limits);
            let image = decoder
                .decode()
                .map_err(|error| error.to_string())?
                .thumbnail(width, 1024)
                .into_rgba8();
            let tile = Tile {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            };
            return if let Some(region) = locator.region {
                crop(tile, region)
            } else {
                Ok(tile)
            };
        }
        let position = self.locate(locator, width)?;
        let layout = self.layout(position.section, width)?;
        let top = ((position.fraction * layout.height as f32) as u32)
            .min(layout.height.saturating_sub(1));
        let height = locator
            .region
            .map_or(512, |region| {
                (region[3] * layout.height as f32).ceil().max(1.0) as u32
            })
            .min(2048)
            .min(layout.height - top);
        let mut tile = match &mut self.content {
            Content::Epub(epub) => epub.tile(layout, top, height)?,
            Content::Pdf(pdf) => pdf.tile(position.section, layout, top, height)?,
            Content::PdfOnline(pdf) => pdf.tile(position.section, layout, top, height)?,
        };
        if let Some(region) = locator.region {
            let left = ((region[0] * tile.width as f32) as u32).min(tile.width - 1);
            let width = ((region[2] * tile.width as f32) as u32)
                .max(1)
                .min(tile.width - left);
            let height = ((region[3] * layout.height as f32) as u32)
                .max(1)
                .min(tile.height);
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for row in 0..height {
                let offset = ((row * tile.width + left) * 4) as usize;
                rgba.extend_from_slice(&tile.rgba[offset..offset + width as usize * 4]);
            }
            tile = Tile {
                width,
                height,
                rgba,
            };
        }
        Ok(tile)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("Choose a PDF or EPUB file".into());
        }
        let mut signature = [0u8; 4];
        let mut probe = file.try_clone().map_err(|error| error.to_string())?;
        probe
            .read_exact(&mut signature)
            .map_err(|error| error.to_string())?;
        if signature == *b"%PDF"
            && let Ok(pdf) = pdf_online::Pdf::new(lince_resources::RangeReader::from_file(path)?)
        {
            let sections = (1..=pdf.len()).map(|page| format!("Page {page}")).collect();
            return Ok(Self {
                content: Content::PdfOnline(pdf),
                info: Info {
                    title: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    sections,
                    is_pdf: true,
                },
            });
        }
        if signature.starts_with(b"PK") {
            let epub =
                epub::Epub::from_reader(File::open(path).map_err(|error| error.to_string())?)?;
            let info = Info {
                title: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                sections: epub.sections.clone(),
                is_pdf: false,
            };
            return Ok(Self {
                content: Content::Epub(epub),
                info,
            });
        }
        use std::io::{Seek, SeekFrom};
        probe
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err("Documents are limited to 256 MiB".into());
        }
        let mut bytes = Vec::new();
        probe
            .take(MAX_FILE_BYTES + 1)
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
            Content::PdfOnline(pdf) => pdf.layout(section, width),
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
            Content::PdfOnline(pdf) => pdf.tile(section, layout, top, height),
            Content::Epub(epub) => epub.tile(layout, top, height),
        }
    }
}

#[cfg(test)]
mod tests;
