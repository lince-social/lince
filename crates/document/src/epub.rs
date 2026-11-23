use std::{
    collections::VecDeque,
    io::{Cursor, Read, Seek},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{DocumentConfig, FontContext, Point, build_single_font_ctx};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{NetHandler, NetProvider, Request, Url},
    shell::{ColorScheme, Viewport},
};
use peniko::{
    Blob, Fill,
    color::palette::css::WHITE,
    kurbo::{Affine, Rect},
};
use rbook::epub::spine::EpubSpineEntry;

use crate::{
    Layout, MAX_SECTIONS, Result,
    geometry::bounded_page_height,
    resources,
    tile::{Tile, rgba_byte_len},
};

const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const _: () = assert!(
    MAX_ARCHIVE_ENTRIES == 20_000,
    "update the archive-entry limit in the error message"
);
const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHAPTER_BYTES: usize = 4 * 1024 * 1024;
const MAX_ASSET_REQUESTS: usize = 2048;
const MAX_EPUB_HEIGHT: u32 = 1_000_000;
const DEFAULT_VIEWPORT_HEIGHT: u32 = 800;
const EPUB_BASE_URL: &str = "https://book.invalid/";
const UA_STYLESHEET: &str = "html { background: white; color: #202020; } :root body { margin: 28px; font-size: 20px; line-height: 1.55; overflow-wrap: anywhere; } img, svg { max-width: 100%; height: auto; } pre { white-space: pre-wrap; }";
const FALLBACK_FONT: &[u8] =
    include_bytes!("../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf");
const BOOK_FONTS: [&[u8]; 4] = [
    include_bytes!("../../../institute/assets/fonts/Lato/Lato-Regular.ttf"),
    include_bytes!("../../../institute/assets/fonts/Lato/Lato-Bold.ttf"),
    include_bytes!("../../../institute/assets/fonts/Lato/Lato-Italic.ttf"),
    include_bytes!("../../../institute/assets/fonts/Lato/Lato-BoldItalic.ttf"),
];

type Pending = (Request, Box<dyn NetHandler>);

/// Queues asset requests so `layout` can answer them from the archive.
#[derive(Default)]
struct Assets(Mutex<VecDeque<Pending>>);

impl Assets {
    /// Locks the queue. A panic while it was held cannot leave a partial
    /// request behind, so a poisoned queue is still usable.
    fn queue(&self) -> MutexGuard<'_, VecDeque<Pending>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl NetProvider for Assets {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        self.queue().push_back((request, handler));
    }
}

pub(crate) struct Epub {
    book: rbook::Epub,
    /// Bundled fonts, built once and cloned for each chapter document.
    fonts: FontContext,
    document: Option<(usize, Layout, HtmlDocument)>,
    renderer: VelloCpuImageRenderer,
}

impl Epub {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<(Self, Vec<String>)> {
        Self::from_reader(Cursor::new(bytes))
    }

    /// Opens an EPUB archive and returns it with its linear chapter hrefs.
    pub(crate) fn from_reader<R: Read + Seek + Send + Sync + 'static>(
        reader: R,
    ) -> Result<(Self, Vec<String>)> {
        let mut archive = zip::ZipArchive::new(reader).map_err(|error| error.to_string())?;
        if archive.len() > MAX_ARCHIVE_ENTRIES || archive.by_name("META-INF/encryption.xml").is_ok()
        {
            return Err(
                "Encrypted EPUBs and archives over 20,000 entries are not supported".into(),
            );
        }
        let mut expanded = 0u64;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| error.to_string())?;
            expanded = expanded.saturating_add(entry.size());
            if entry.size() > MAX_ENTRY_BYTES || expanded > MAX_EXPANDED_BYTES {
                return Err("EPUB resources exceed the size limit".into());
            }
        }
        let book = rbook::Epub::read(archive.into_inner()).map_err(|error| error.to_string())?;
        let sections: Vec<_> = book
            .spine()
            .iter()
            .filter(EpubSpineEntry::is_linear)
            .filter_map(|entry| {
                entry
                    .manifest_entry()
                    .map(|item| item.href().as_str().to_owned())
            })
            .collect();
        if sections.is_empty() || sections.len() >= MAX_SECTIONS {
            return Err("EPUB has no readable chapters".into());
        }
        let mut fonts = build_single_font_ctx(FALLBACK_FONT);
        for data in BOOK_FONTS {
            fonts
                .collection
                .register_fonts(Blob::new(Arc::new(data)), None);
        }
        let epub = Self {
            book,
            fonts,
            document: None,
            renderer: VelloCpuImageRenderer::new(1, 1),
        };
        Ok((epub, sections))
    }

    pub(crate) fn resource(&self, href: &str) -> Result<Vec<u8>> {
        let bytes = if href.starts_with('/') {
            self.book.read_resource_bytes(href)
        } else {
            self.book
                .read_resource_bytes(format!("/{href}"))
                .or_else(|_| self.book.read_resource_bytes(href))
        };
        bytes.map_err(|error| error.to_string())
    }

    /// Returns the normalized vertical position of a fragment or quoted passage
    /// in the chapter that was laid out last.
    pub(crate) fn anchor(&self, fragment: &str, quote: &str) -> Option<f32> {
        let (_, layout, document) = self.document.as_ref()?;
        let doc = document.as_ref();
        let id = if !fragment.is_empty() {
            doc.get_element_by_id(fragment)
        } else if !quote.is_empty() {
            doc.query_selector_all("*")
                .ok()?
                .into_iter()
                .filter_map(|id| {
                    let node = doc.get_node(id)?;
                    let text = node.text_content();
                    text.contains(quote).then_some((id, text.len()))
                })
                .min_by_key(|(_, length)| *length)
                .map(|(id, _)| id)
        } else {
            None
        }?;
        let node = doc.get_node(id)?;
        let mut top = node.final_layout().location.y;
        let mut parent = node.parent;
        while let Some(id) = parent {
            let node = doc.get_node(id)?;
            top += node.final_layout().location.y;
            parent = node.parent;
        }
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "`layout` validates the height within 1..=MAX_EPUB_HEIGHT, which is exact in f32"
        )]
        let height = layout.height as f32;
        Some((top / height).clamp(0.0, 1.0))
    }

    /// Lays out chapter `section`, stored at `href`, at `width` pixels.
    pub(crate) fn layout(&mut self, section: usize, href: &str, width: u32) -> Result<Layout> {
        if let Some((loaded, layout, _)) = &self.document
            && *loaded == section
            && layout.width == width
        {
            return Ok(*layout);
        }
        let source = self
            .book
            .read_resource_str(href)
            .map_err(|error| error.to_string())?;
        if source.len() > MAX_CHAPTER_BYTES {
            return Err(format!(
                "EPUB chapter exceeds {} MiB",
                MAX_CHAPTER_BYTES >> 20
            ));
        }
        let mut base = Url::parse(EPUB_BASE_URL).expect("EPUB_BASE_URL is an absolute https URL");
        base.set_path(href);
        let source = resources::markup(&self.book, &base, source.as_bytes(), 0)?;
        let assets = Arc::new(Assets::default());
        let mut document = HtmlDocument::from_html(
            &String::from_utf8_lossy(&source),
            DocumentConfig {
                font_ctx: Some(self.fonts.clone()),
                base_url: Some(base.to_string()),
                viewport: Some(Viewport::new(
                    width,
                    DEFAULT_VIEWPORT_HEIGHT,
                    1.0,
                    ColorScheme::Light,
                )),
                net_provider: Some(assets.clone()),
                ua_stylesheets: Some(vec![UA_STYLESHEET.into()]),
                ..Default::default()
            },
        );
        let mut requests = 0;
        loop {
            document.resolve(0.0);
            let pending: Vec<_> = assets.queue().drain(..).collect();
            if pending.is_empty() {
                break;
            }
            for (request, handler) in pending {
                requests += 1;
                if requests > MAX_ASSET_REQUESTS {
                    return Err("EPUB chapter requests too many resources".into());
                }
                // A missing or rejected asset renders as an empty resource,
                // like a broken image in a browser.
                let bytes = resources::load(&self.book, &request.url, 0).unwrap_or_default();
                handler.bytes(request.url.to_string(), bytes.into());
            }
        }
        let height = document
            .as_ref()
            .root_element()
            .final_layout()
            .size
            .height
            .ceil();
        let height = bounded_page_height(
            height,
            MAX_EPUB_HEIGHT,
            "EPUB chapter dimensions exceed the rendering limit",
        )?;
        let layout = Layout { width, height };
        self.document = Some((section, layout, document));
        Ok(layout)
    }

    pub(crate) fn tile(&mut self, layout: Layout, top: u32, height: u32) -> Result<Tile> {
        let document = &mut self
            .document
            .as_mut()
            .ok_or("EPUB chapter is not laid out")?
            .2;
        document.as_mut().set_viewport_scroll(Point {
            x: 0.0,
            y: f64::from(top),
        });
        let byte_len = rgba_byte_len(layout.width, height).ok_or("Invalid image dimensions")?;
        self.renderer.resize(layout.width, height);
        self.renderer.reset();
        let mut rgba = vec![0; byte_len];
        self.renderer.render(
            |scene| {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    WHITE,
                    None,
                    &Rect::new(0.0, 0.0, f64::from(layout.width), f64::from(height)),
                );
                blitz_paint::paint_scene(scene, document.as_mut(), 1.0, layout.width, height, 0, 0);
            },
            &mut rgba,
        );
        Ok(Tile {
            width: layout.width,
            height,
            rgba,
        })
    }
}
