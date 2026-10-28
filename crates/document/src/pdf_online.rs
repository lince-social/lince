use crate::{Layout, Result, Tile};
use lince_resources::RangeReader;
use pdfium_render::prelude::*;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, OnceLock};

static PDFIUM: OnceLock<std::result::Result<Mutex<Pdfium>, String>> = OnceLock::new();

fn pdfium() -> Result<&'static Mutex<Pdfium>> {
    PDFIUM
        .get_or_init(|| {
            let bindings = if let Some(path) = std::env::var_os("LINCE_PDFIUM_LIBRARY") {
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
        self.position += count as u64;
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

pub(super) struct Pdf {
    source: Source,
    pages: usize,
}

impl Pdf {
    pub fn new(reader: RangeReader) -> Result<Self> {
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
        if pages == 0 || pages >= 100_000 {
            return Err("PDF has no pages or too many pages".into());
        }
        Ok(Self { source, pages })
    }

    pub fn len(&self) -> usize {
        self.pages
    }

    pub fn layout(&self, section: usize, width: u32) -> Result<Layout> {
        let engine = pdfium()?.lock().map_err(|error| error.to_string())?;
        self.source.clear_failure()?;
        let document = engine
            .load_pdf_from_reader(self.source.clone(), None)
            .map_err(|error| error.to_string())?;
        let page = document
            .pages()
            .get(i32::try_from(section).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        let height = (page.height().value * width as f32 / page.width().value).ceil();
        self.source.check()?;
        if !height.is_finite() || !(1.0..=8192.0).contains(&height) {
            return Err("PDF page dimensions exceed the rendering limit".into());
        }
        Ok(Layout {
            width,
            height: height as u32,
        })
    }

    pub fn tile(&self, section: usize, layout: Layout, top: u32, height: u32) -> Result<Tile> {
        let engine = pdfium()?.lock().map_err(|error| error.to_string())?;
        self.source.clear_failure()?;
        let document = engine
            .load_pdf_from_reader(self.source.clone(), None)
            .map_err(|error| error.to_string())?;
        let page = document
            .pages()
            .get(i32::try_from(section).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(layout.width as i32)
                    .set_target_height(layout.height as i32)
                    .render_form_data(false),
            )
            .map_err(|error| error.to_string())?;
        let bytes = bitmap.as_rgba_bytes();
        self.source.check()?;
        let stride = layout.width as usize * 4;
        let rgba = bytes
            .get(top as usize * stride..(top + height) as usize * stride)
            .ok_or("Invalid PDF region")?
            .to_vec();
        Ok(Tile {
            width: layout.width,
            height,
            rgba,
        })
    }
}
