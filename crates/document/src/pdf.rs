use hayro::{
    RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax,
    vello_cpu::Pixmap,
};
use peniko::color::palette::css::WHITE;

use crate::{
    Layout, MAX_SECTIONS, Result,
    geometry::{PDF_PAGE_TOO_LARGE, pdf_layout},
    tile::Tile,
};

pub(crate) struct Pdf {
    document: hayro_syntax::Pdf,
    /// The last rendered page, which serves every tile of that page.
    cached: Option<(usize, Layout, Pixmap)>,
}

impl Pdf {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self> {
        let document =
            hayro_syntax::Pdf::new(bytes).map_err(|error| format!("Cannot open PDF: {error:?}"))?;
        if document.pages().is_empty() || document.pages().len() >= MAX_SECTIONS {
            return Err("PDF has no pages or too many pages".into());
        }
        Ok(Self {
            document,
            cached: None,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.document.pages().len()
    }

    fn page(&self, section: usize) -> Result<&hayro_syntax::page::Page<'_>> {
        self.document
            .pages()
            .get(section)
            .ok_or_else(|| "Invalid document section".into())
    }

    pub(crate) fn layout(&self, section: usize, width: u32) -> Result<Layout> {
        let (page_width, page_height) = self.page(section)?.render_dimensions();
        pdf_layout(page_width, page_height, width)
    }

    pub(crate) fn tile(
        &mut self,
        section: usize,
        layout: Layout,
        top: u32,
        height: u32,
    ) -> Result<Tile> {
        let pixmap = match &self.cached {
            Some((page, size, pixmap)) if *page == section && *size == layout => pixmap,
            _ => {
                let page = self.page(section)?;
                let width = u16::try_from(layout.width).map_err(|_| PDF_PAGE_TOO_LARGE)?;
                let height = u16::try_from(layout.height).map_err(|_| PDF_PAGE_TOO_LARGE)?;
                let scale = f32::from(width) / page.render_dimensions().0;
                let settings = RenderSettings {
                    x_scale: scale,
                    y_scale: scale,
                    width: Some(width),
                    height: Some(height),
                    bg_color: WHITE,
                };
                let pixmap = hayro::render(
                    page,
                    &RenderCache::new(),
                    &InterpreterSettings::default(),
                    &settings,
                );
                &self.cached.insert((section, layout, pixmap)).2
            }
        };
        Tile::from_rows(pixmap.data_as_u8_slice(), layout.width, top, height)
            .ok_or_else(|| "Invalid PDF region".into())
    }
}
