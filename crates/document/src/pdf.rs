use super::{Layout, Result, Tile};
use hayro::{RenderCache, RenderSettings, hayro_syntax};

pub(super) struct Pdf {
    document: hayro_syntax::Pdf,
    cached: Option<(usize, Layout, Vec<u8>)>,
}

impl Pdf {
    pub(super) fn new(bytes: Vec<u8>) -> Result<Self> {
        let document =
            hayro_syntax::Pdf::new(bytes).map_err(|error| format!("Cannot open PDF: {error:?}"))?;
        if document.pages().is_empty() || document.pages().len() >= 100_000 {
            return Err("PDF has no pages or too many pages".into());
        }
        Ok(Self {
            document,
            cached: None,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.document.pages().len()
    }

    pub(super) fn layout(&self, section: usize, width: u32) -> Result<Layout> {
        let (w, h) = self.document.pages()[section].render_dimensions();
        let height = (h * width as f32 / w).ceil();
        if !height.is_finite() || !(1.0..=8192.0).contains(&height) {
            return Err("PDF page dimensions exceed the rendering limit".into());
        }
        Ok(Layout {
            width,
            height: height as u32,
        })
    }

    pub(super) fn tile(
        &mut self,
        section: usize,
        layout: Layout,
        top: u32,
        height: u32,
    ) -> Result<Tile> {
        if self
            .cached
            .as_ref()
            .is_none_or(|(page, size, _)| *page != section || *size != layout)
        {
            let page = &self.document.pages()[section];
            let scale = layout.width as f32 / page.render_dimensions().0;
            let settings = RenderSettings {
                x_scale: scale,
                y_scale: scale,
                width: Some(layout.width as u16),
                height: Some(layout.height as u16),
                bg_color: peniko::color::palette::css::WHITE,
            };
            let pixmap = hayro::render(page, &RenderCache::new(), &Default::default(), &settings);
            self.cached = Some((section, layout, pixmap.data_as_u8_slice().to_vec()));
        }
        let stride = layout.width as usize * 4;
        let rgba = self.cached.as_ref().unwrap().2
            [top as usize * stride..(top + height) as usize * stride]
            .to_vec();
        Ok(Tile {
            width: layout.width,
            height,
            rgba,
        })
    }
}
