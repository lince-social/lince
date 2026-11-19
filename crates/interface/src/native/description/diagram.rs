use super::raster::{self, Pixels};
use super::*;
use std::sync::{Arc, OnceLock};

#[derive(Component)]
struct Diagram {
    source: Arc<str>,
    label: Entity,
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str) {
    let frame = crate::native::embedded_media::frame(world, parent, "Diagram");
    let owner = frame.owner;
    crate::native::edit_mode::label(world, frame.controls, source, 12.0);
    let label = crate::native::edit_mode::label(world, owner, "Rendering diagram…", 14.0);
    world.entity_mut(owner).insert(Diagram {
        source: source.into(),
        label,
    });
}

pub(super) fn rasterize(source: &str) -> Pixels {
    let _raster = nucleus::performance::Span::new("description-diagram-rasterize");
    if source.len() > 16_384 || source.lines().count() > 256 {
        return Err("Diagram is too large to render (256 lines / 16 KB).".into());
    }
    let mut rendering = mermaid_rs_renderer::RenderOptions::default();
    rendering.theme.font_family = "Lato".into();
    let svg = mermaid_rs_renderer::render_with_options(source, rendering)
        .map_err(|error| error.to_string())?;
    let mut options = resvg::usvg::Options {
        font_family: "Lato".into(),
        fontdb: fonts().clone(),
        ..default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = (1600.0 / size.width().max(size.height())).min(1.5);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("Diagram size is invalid")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    for rgba in pixels.chunks_exact_mut(4) {
        if rgba[3] > 0 {
            for index in 0..3 {
                rgba[index] = ((rgba[index] as u32 * 255) / rgba[3] as u32).min(255) as u8;
            }
        }
    }
    Ok((width, height, pixels))
}

fn fonts() -> &'static Arc<resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let _fonts = nucleus::performance::Span::new("description-diagram-font-preparation");
        let mut database = resvg::usvg::fontdb::Database::new();
        database.load_font_data(
            include_bytes!("../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        );
        database.set_sans_serif_family("Lato");
        database.set_serif_family("Lato");
        database.set_monospace_family("Lato");
        nucleus::performance::add("description-diagram-font-databases-prepared", 1);
        Arc::new(database)
    })
}

impl raster::Request for Diagram {
    const CAPACITY: usize = 1;
    const STOPPED: &'static str = "Diagram renderer stopped.";
    const PANICKED: &'static str = "Diagram syntax could not be rendered.";

    fn source(&self) -> &Arc<str> {
        &self.source
    }

    fn label(&self) -> Entity {
        self.label
    }

    fn render(source: &str) -> Pixels {
        rasterize(source)
    }

    fn layout(_: u32) -> Node {
        Node {
            width: percent(100),
            height: Val::Auto,
            ..default()
        }
    }

    fn failure(source: &str, error: &str) -> String {
        format!("Diagram could not be rendered: {error}\n\n{source}")
    }
}

pub(super) fn update(world: &mut World) {
    raster::update::<Diagram>(world);
}
