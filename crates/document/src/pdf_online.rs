use std::{
    env::var_os,
    io::{self, Read, Seek, SeekFrom},
    sync::{Arc, Mutex, OnceLock},
};

use lince_resources::RangeReader;
use pdfium_render::prelude::{PdfPage, PdfRenderConfig, Pdfium};

use crate::{
    Layout, MAX_SECTIONS, Result,
    geometry::{PDF_PAGE_TOO_LARGE, pdf_layout},
    tile::{PixelRect, Tile},
};

static PDFIUM: OnceLock<std::result::Result<Mutex<Pdfium>, String>> = OnceLock::new();

fn pdfium() -> Result<&'static Mutex<Pdfium>> {
    PDFIUM
        .get_or_init(|| {
            let bindings = if let Some(path) = var_os("LINCE_PDFIUM_LIBRARY") {
                Pdfium::bind_to_library(path)
            } else {
                Pdfium::bind_to_system_library()
            };
            bindings
                .map(|bindings| Mutex::new(Pdfium::new(bindings)))
                .map_err(|error| format!("PDF range reading needs the PDFium library: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// A cloneable cursor over a shared [`RangeReader`].
///
/// The engine reports a failed read only as a parse error, so the first read
/// error is kept in `failure` for the caller to report instead.
#[derive(Clone)]
struct Source {
    reader: Arc<Mutex<RangeReader>>,
    failure: Arc<Mutex<Option<String>>>,
    position: u64,
}

impl Source {
    fn clear_failure(&self) -> Result<()> {
        *self.failure.lock().map_err(|error| error.to_string())? = None;
        Ok(())
    }

    fn check(&self) -> Result<()> {
        match self
            .failure
            .lock()
            .map_err(|error| error.to_string())?
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut reader = self
            .reader
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?;
        reader.seek(SeekFrom::Start(self.position))?;
        let count = reader.read(bytes).inspect_err(|error| {
            if let Ok(mut failure) = self.failure.lock() {
                *failure = Some(error.to_string());
            }
        })?;
        self.position = reader.stream_position()?;
        Ok(count)
    }
}

impl Seek for Source {
    fn seek(&mut self, offset: SeekFrom) -> io::Result<u64> {
        let mut reader = self
            .reader
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?;
        reader.seek(SeekFrom::Start(self.position))?;
        self.position = reader.seek(offset)?;
        Ok(self.position)
    }
}

pub(crate) struct Pdf {
    source: Source,
    pages: usize,
    /// The last computed layout, which spares a document load on every tile.
    layout: Option<(usize, Layout)>,
}

impl Pdf {
    pub(crate) fn new(reader: RangeReader) -> Result<Self> {
        let source = Source {
            reader: Arc::new(Mutex::new(reader)),
            failure: Arc::new(Mutex::new(None)),
            position: 0,
        };
        let engine = pdfium()?.lock().map_err(|error| error.to_string())?;
        let document = engine
            .load_pdf_from_reader(source.clone(), None)
            .map_err(|error| error.to_string())?;
        let pages = usize::try_from(document.pages().len()).map_err(|error| error.to_string())?;
        source.check()?;
        if pages == 0 || pages >= MAX_SECTIONS {
            return Err("PDF has no pages or too many pages".into());
        }
        Ok(Self {
            source,
            pages,
            layout: None,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.pages
    }

    /// Loads the document and passes page `section` to `visit`.
    ///
    /// A loaded `PdfDocument` borrows the `Pdfium` instance behind the global
    /// lock, so it cannot outlive this call and each visit reloads it through
    /// the range reader, which keeps fetched ranges.
    fn with_page<T>(
        &self,
        section: usize,
        visit: impl FnOnce(PdfPage<'_>) -> Result<T>,
    ) -> Result<T> {
        let engine = pdfium()?.lock().map_err(|error| error.to_string())?;
        self.source.clear_failure()?;
        let document = engine
            .load_pdf_from_reader(self.source.clone(), None)
            .map_err(|error| error.to_string())?;
        let page = document
            .pages()
            .get(i32::try_from(section).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        let output = visit(page)?;
        self.source.check()?;
        Ok(output)
    }

    pub(crate) fn layout(&mut self, section: usize, width: u32) -> Result<Layout> {
        if let Some((loaded, layout)) = self.layout
            && loaded == section
            && layout.width == width
        {
            return Ok(layout);
        }
        let (page_width, page_height) = self.with_page(section, |page| {
            Ok((page.width().value, page.height().value))
        })?;
        let layout = pdf_layout(page_width, page_height, width)?;
        self.layout = Some((section, layout));
        Ok(layout)
    }

    pub(crate) fn tile(
        &self,
        section: usize,
        layout: Layout,
        top: u32,
        height: u32,
    ) -> Result<Tile> {
        let target_width = i32::try_from(layout.width).map_err(|_| PDF_PAGE_TOO_LARGE)?;
        let target_height = i32::try_from(layout.height).map_err(|_| PDF_PAGE_TOO_LARGE)?;
        let rgba = self.with_page(section, |page| {
            let bitmap = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(target_width)
                        .set_target_height(target_height)
                        .render_form_data(false),
                )
                .map_err(|error| error.to_string())?;
            Ok(bitmap.as_rgba_bytes())
        })?;
        let page = Tile {
            width: layout.width,
            height: layout.height,
            rgba,
        };
        let band = PixelRect {
            left: 0,
            top,
            width: layout.width,
            height,
        };
        page.into_rect(band)
            .ok_or_else(|| "Invalid PDF region".into())
    }
}
