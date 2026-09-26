use super::{Layout, Result, Tile};
use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{NetHandler, NetProvider, Request, Url},
    shell::{ColorScheme, Viewport},
};
use peniko::{Fill, color::palette::css::WHITE, kurbo::Rect};
use std::{
    collections::VecDeque,
    io::Cursor,
    sync::{Arc, Mutex},
};

type Pending = (Request, Box<dyn NetHandler>);

#[derive(Default)]
struct Assets(Mutex<VecDeque<Pending>>);

impl NetProvider for Assets {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        self.0.lock().unwrap().push_back((request, handler));
    }
}

pub(super) struct Epub {
    book: rbook::Epub,
    pub(super) sections: Vec<String>,
    document: Option<(usize, Layout, HtmlDocument)>,
    renderer: VelloCpuImageRenderer,
}

impl Epub {
    pub(super) fn new(bytes: Vec<u8>) -> Result<Self> {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|error| error.to_string())?;
        if archive.len() > 20_000 || archive.by_name("META-INF/encryption.xml").is_ok() {
            return Err(
                "Encrypted EPUBs and archives over 20,000 entries are not supported".into(),
            );
        }
        let mut expanded = 0u64;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| error.to_string())?;
            expanded = expanded.saturating_add(entry.size());
            if entry.size() > 32 * 1024 * 1024 || expanded > 512 * 1024 * 1024 {
                return Err("EPUB resources exceed the size limit".into());
            }
        }
        drop(archive);
        let book = rbook::Epub::read(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        let sections: Vec<_> = book
            .spine()
            .iter()
            .filter(|entry| entry.is_linear())
            .filter_map(|entry| {
                entry
                    .manifest_entry()
                    .map(|item| item.href().as_str().to_owned())
            })
            .collect();
        if sections.is_empty() || sections.len() >= 100_000 {
            return Err("EPUB has no readable chapters".into());
        }
        Ok(Self {
            book,
            sections,
            document: None,
            renderer: VelloCpuImageRenderer::new(1, 1),
        })
    }

    pub(super) fn layout(&mut self, section: usize, width: u32) -> Result<Layout> {
        if let Some((loaded, layout, _)) = &self.document
            && *loaded == section
            && layout.width == width
        {
            return Ok(*layout);
        }
        let source = self
            .book
            .read_resource_str(self.sections[section].as_str())
            .map_err(|error| error.to_string())?;
        if source.len() > 4 * 1024 * 1024 {
            return Err("EPUB chapter exceeds 4 MiB".into());
        }
        let mut base = Url::parse("https://book.invalid/").unwrap();
        base.set_path(&self.sections[section]);
        let source = super::resources::markup(&self.book, &base, source.as_bytes(), 0)?;
        let assets = Arc::new(Assets::default());
        let mut fonts = blitz_dom::build_single_font_ctx(include_bytes!(
            "../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf"
        ));
        for data in [
            include_bytes!("../../../institute/assets/fonts/Lato/Lato-Regular.ttf").as_slice(),
            include_bytes!("../../../institute/assets/fonts/Lato/Lato-Bold.ttf").as_slice(),
            include_bytes!("../../../institute/assets/fonts/Lato/Lato-Italic.ttf").as_slice(),
            include_bytes!("../../../institute/assets/fonts/Lato/Lato-BoldItalic.ttf").as_slice(),
        ] {
            fonts.collection.register_fonts(data.to_vec().into(), None);
        }
        let mut document = HtmlDocument::from_html(&String::from_utf8_lossy(&source), DocumentConfig {
            font_ctx: Some(fonts),
            base_url: Some(base.to_string()),
            viewport: Some(Viewport::new(width, 800, 1.0, ColorScheme::Light)),
            net_provider: Some(assets.clone()),
            ua_stylesheets: Some(vec!["html { background: white; color: #202020; } :root body { margin: 28px; font-size: 20px; line-height: 1.55; overflow-wrap: anywhere; } img, svg { max-width: 100%; height: auto; } pre { white-space: pre-wrap; }".into()]),
            ..Default::default()
        });
        let mut requests = 0;
        loop {
            document.resolve(0.0);
            let pending: Vec<_> = assets.0.lock().unwrap().drain(..).collect();
            if pending.is_empty() {
                break;
            }
            for (request, handler) in pending {
                requests += 1;
                if requests > 2048 {
                    return Err("EPUB chapter requests too many resources".into());
                }
                let bytes = super::resources::load(&self.book, &request.url, 0).unwrap_or_default();
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
        if !height.is_finite() || !(1.0..=1_000_000.0).contains(&height) {
            return Err("EPUB chapter dimensions exceed the rendering limit".into());
        }
        let layout = Layout {
            width,
            height: height as u32,
        };
        self.document = Some((section, layout, document));
        Ok(layout)
    }

    pub(super) fn tile(&mut self, layout: Layout, top: u32, height: u32) -> Result<Tile> {
        let document = &mut self
            .document
            .as_mut()
            .ok_or("EPUB chapter is not laid out")?
            .2;
        document.as_mut().set_viewport_scroll(blitz_dom::Point {
            x: 0.0,
            y: top as f64,
        });
        self.renderer.resize(layout.width, height);
        self.renderer.reset();
        let mut rgba = vec![0; layout.width as usize * height as usize * 4];
        self.renderer.render(
            |scene| {
                scene.fill(
                    Fill::NonZero,
                    Default::default(),
                    WHITE,
                    None,
                    &Rect::new(0.0, 0.0, layout.width as f64, height as f64),
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
