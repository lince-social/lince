//! The [`Reader`] façade over the PDF and EPUB backends.

use std::{
    fmt,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
};

use image::{ImageReader, Limits};
use lince_resources::{Library, Locator, RangeReader, Resource, fingerprint};

use crate::{
    Format, Info, Layout, MAX_FILE_BYTES, MAX_WIDTH, Mode, Position, Result, TILE_HEIGHT,
    TILE_OVERLAP,
    epub::Epub,
    geometry::{scale_ceil, scale_floor},
    pdf, pdf_online,
    tile::{PixelRect, Tile},
};

const MIN_WIDTH: u32 = 128;
const MAX_EXCERPT_HEIGHT: u32 = 2048;
const MAX_EXCERPT_IMAGE_DIMENSION: u32 = 8192;
const MAX_EXCERPT_THUMBNAIL_HEIGHT: u32 = 1024;
const MAX_EXCERPT_IMAGE_ALLOC_BYTES: u64 = 128 * 1024 * 1024;
const PDF_MAGIC: &[u8; 4] = b"%PDF";
const PDF_HEADER: &[u8; 5] = b"%PDF-";
const ZIP_MAGIC: &[u8; 2] = b"PK";
const NOT_A_DOCUMENT: &str = "This file is not a PDF or EPUB";

pub(crate) enum Content {
    Pdf(pdf::Pdf),
    PdfOnline(pdf_online::Pdf),
    Epub(Box<Epub>),
}

/// Stateful PDF and EPUB document reader with section layout and tile rendering.
pub struct Reader {
    pub(crate) content: Content,
    /// Metadata for the opened document.
    pub info: Info,
}

impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let backend = match &self.content {
            Content::Pdf(_) => "Pdf",
            Content::PdfOnline(_) => "PdfOnline",
            Content::Epub(_) => "Epub",
        };
        f.debug_struct("Reader")
            .field("backend", &backend)
            .field("info", &self.info)
            .finish()
    }
}

impl Reader {
    fn from_epub((epub, sections): (Epub, Vec<String>), title: String) -> Self {
        Self {
            content: Content::Epub(Box::new(epub)),
            info: Info {
                title,
                sections,
                format: Format::Epub,
            },
        }
    }

    fn from_pdf(content: Content, pages: usize, title: String) -> Self {
        let sections = (1..=pages).map(|page| format!("Page {page}")).collect();
        Self {
            content,
            info: Info {
                title,
                sections,
                format: Format::Pdf,
            },
        }
    }

    /// Opens a PDF or EPUB from a [`Resource`] using range-backed reading when supported.
    ///
    /// # Errors
    ///
    /// Returns an error if the resource cannot be opened or read, does not
    /// start with a PDF or ZIP signature, exceeds [`MAX_FILE_BYTES`] during
    /// buffered fallback, fails content-hash verification, or is not a valid
    /// supported PDF or EPUB.
    pub fn open_resource(library: Library, resource: &Resource) -> Result<Self> {
        let mut source = RangeReader::open(library.clone(), resource)?;
        let mut magic = [0; 4];
        source
            .read_exact(&mut magic)
            .map_err(|error| error.to_string())?;
        source
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        if magic.starts_with(ZIP_MAGIC) {
            return Ok(Self::from_epub(
                Epub::from_reader(source)?,
                resource.title.clone(),
            ));
        }
        if magic != *PDF_MAGIC {
            return Err(NOT_A_DOCUMENT.into());
        }
        if let Ok(pdf) = pdf_online::Pdf::new(source) {
            let pages = pdf.len();
            return Ok(Self::from_pdf(
                Content::PdfOnline(pdf),
                pages,
                resource.title.clone(),
            ));
        }
        // PDFium consumed the reader, so the buffered fallback needs a fresh one.
        let mut source = RangeReader::open(library, resource)?;
        if source.len() > MAX_FILE_BYTES {
            return Err(format!(
                "This renderer requires a document under {} MiB",
                MAX_FILE_BYTES >> 20
            ));
        }
        let mut bytes = Vec::new();
        source
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if resource
            .hash
            .as_ref()
            .is_some_and(|hash| !hash.eq_ignore_ascii_case(&fingerprint(&bytes)))
        {
            return Err("The document does not match its content hash".into());
        }
        Self::from_bytes(bytes, &resource.title)
    }

    /// Opens a local PDF or EPUB file from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` is not a regular file, cannot be read,
    /// exceeds [`MAX_FILE_BYTES`] during buffered loading, or is not a valid
    /// PDF or EPUB.
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("Choose a PDF or EPUB file".into());
        }
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature)
            .map_err(|error| error.to_string())?;
        let title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if signature == *PDF_MAGIC
            && let Ok(pdf) = pdf_online::Pdf::new(RangeReader::from_file(path)?)
        {
            let pages = pdf.len();
            return Ok(Self::from_pdf(Content::PdfOnline(pdf), pages, title));
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        if signature.starts_with(ZIP_MAGIC) {
            return Ok(Self::from_epub(Epub::from_reader(file)?, title));
        }
        // Reject a large file before reading it. The bounded read also covers a
        // file that grows after this check, and `from_bytes` rejects the extra byte.
        if metadata.len() > MAX_FILE_BYTES {
            return Err(file_too_large());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Self::from_bytes(bytes, &title)
    }

    /// Opens an in-memory PDF or EPUB byte buffer with the given display `title`.
    ///
    /// # Errors
    ///
    /// Returns an error if `bytes` exceeds [`MAX_FILE_BYTES`], does not begin
    /// with a PDF or ZIP signature, or fails format validation.
    pub fn from_bytes(bytes: Vec<u8>, title: &str) -> Result<Self> {
        if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_FILE_BYTES) {
            return Err(file_too_large());
        }
        if bytes.starts_with(PDF_HEADER) {
            let pdf = pdf::Pdf::new(bytes)?;
            let pages = pdf.len();
            Ok(Self::from_pdf(Content::Pdf(pdf), pages, title.into()))
        } else if bytes.starts_with(ZIP_MAGIC) {
            Ok(Self::from_epub(Epub::new(bytes)?, title.into()))
        } else {
            Err(NOT_A_DOCUMENT.into())
        }
    }

    /// Resolves a [`Locator`] to a [`Position`] by laying out the target section at `width`.
    ///
    /// # Errors
    ///
    /// Returns an error if `locator` is invalid, references a missing chapter,
    /// page range, or passage, or if section layout fails.
    pub fn locate(&mut self, locator: &Locator, width: u32) -> Result<Position> {
        self.locate_with_layout(locator, width)
            .map(|(position, _)| position)
    }

    fn locate_with_layout(&mut self, locator: &Locator, width: u32) -> Result<(Position, Layout)> {
        locator.validate()?;
        let section = if self.info.is_pdf() {
            locator.page.unwrap_or(1).saturating_sub(1)
        } else if locator.href.is_empty() {
            0
        } else {
            let target = locator.href.trim_start_matches('/');
            self.info
                .sections
                .iter()
                .position(|href| href.trim_start_matches('/') == target || href.ends_with(target))
                .ok_or("The referenced chapter is not present in this edition")?
        };
        if self.info.is_pdf()
            && locator
                .end_page
                .is_some_and(|end| end > self.info.sections.len())
        {
            return Err("The referenced page range is not present in this edition".into());
        }
        let layout = self.layout(section, width)?;
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
        let position = Position {
            section,
            fraction,
            mode: Mode::Scroll,
        };
        Ok((position, layout))
    }

    /// Reads raw bytes for an embedded EPUB resource identified by `href`.
    ///
    /// # Errors
    ///
    /// Returns an error if the opened document is a PDF or if the resource
    /// cannot be read from the EPUB archive.
    pub fn resource_bytes(&self, href: &str) -> Result<Vec<u8>> {
        match &self.content {
            Content::Epub(epub) => epub.resource(href),
            Content::Pdf(_) | Content::PdfOnline(_) => {
                Err("Choose a page region for a PDF image".into())
            }
        }
    }

    /// Renders a visual excerpt for `locator` at `width`, optionally cropped to its region.
    ///
    /// An image excerpt is cropped with [`Tile::crop`]. A section excerpt
    /// starts at the located row, and its region's left edge, width, and
    /// height round down to whole pixels.
    ///
    /// # Errors
    ///
    /// Returns an error if the referenced image, chapter, page, or region is
    /// invalid, cannot be decoded, or fails to render.
    pub fn excerpt(&mut self, locator: &Locator, width: u32) -> Result<Tile> {
        if !locator.image.is_empty() {
            let tile = self.image(&locator.image, width)?;
            return match locator.region {
                Some(region) => tile.crop(region),
                None => Ok(tile),
            };
        }
        let (position, layout) = self.locate_with_layout(locator, width)?;
        let top =
            scale_floor(position.fraction, layout.height).min(layout.height.saturating_sub(1));
        let height = locator
            .region
            .map_or(TILE_HEIGHT, |region| {
                scale_ceil(region[3], layout.height).max(1)
            })
            .min(MAX_EXCERPT_HEIGHT)
            .min(layout.height.saturating_sub(top));
        let tile = self.render(position.section, layout, top, height)?;
        let Some(region) = locator.region else {
            return Ok(tile);
        };
        let left = scale_floor(region[0], tile.width).min(tile.width.saturating_sub(1));
        let rect = PixelRect {
            left,
            top: 0,
            width: scale_floor(region[2], tile.width)
                .max(1)
                .min(tile.width.saturating_sub(left)),
            height: scale_floor(region[3], layout.height)
                .max(1)
                .min(tile.height),
        };
        tile.into_rect(rect)
            .ok_or_else(|| "Invalid image dimensions".into())
    }

    fn image(&self, href: &str, width: u32) -> Result<Tile> {
        let bytes = self.resource_bytes(href)?;
        let mut decoder = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_EXCERPT_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_EXCERPT_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_EXCERPT_IMAGE_ALLOC_BYTES);
        decoder.limits(limits);
        let image = decoder
            .decode()
            .map_err(|error| error.to_string())?
            .thumbnail(width, MAX_EXCERPT_THUMBNAIL_HEIGHT)
            .into_rgba8();
        Ok(Tile {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        })
    }

    /// Computes or retrieves the pixel [`Layout`] for `section` at `width`.
    ///
    /// # Errors
    ///
    /// Returns an error if `section` is out of range, `width` is outside
    /// `128..=MAX_WIDTH`, or the backend cannot lay out the section.
    pub fn layout(&mut self, section: usize, width: u32) -> Result<Layout> {
        let Some(href) = self
            .info
            .sections
            .get(section)
            .filter(|_| (MIN_WIDTH..=MAX_WIDTH).contains(&width))
        else {
            return Err("Invalid document section or render width".into());
        };
        match &mut self.content {
            Content::Pdf(pdf) => pdf.layout(section, width),
            Content::PdfOnline(pdf) => pdf.layout(section, width),
            Content::Epub(epub) => epub.layout(section, href, width),
        }
    }

    /// Renders vertical tile `index` for `section` at `width`.
    ///
    /// # Errors
    ///
    /// Returns an error if laying out `section` fails, `index` falls past the
    /// end of the section, or rendering the tile fails.
    pub fn tile(&mut self, section: usize, width: u32, index: u32) -> Result<Tile> {
        let layout = self.layout(section, width)?;
        let top = index
            .checked_mul(TILE_HEIGHT)
            .filter(|top| *top < layout.height)
            .ok_or("Tile is outside the document")?;
        let height = (TILE_HEIGHT + TILE_OVERLAP).min(layout.height - top);
        self.render(section, layout, top, height)
    }

    /// Renders rows `top..top + height` of a section that was just laid out.
    fn render(&mut self, section: usize, layout: Layout, top: u32, height: u32) -> Result<Tile> {
        match &mut self.content {
            Content::Pdf(pdf) => pdf.tile(section, layout, top, height),
            Content::PdfOnline(pdf) => pdf.tile(section, layout, top, height),
            Content::Epub(epub) => epub.tile(layout, top, height),
        }
    }
}

fn file_too_large() -> String {
    format!("Documents are limited to {} MiB", MAX_FILE_BYTES >> 20)
}
