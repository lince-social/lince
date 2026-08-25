use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

const LATO_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf");

pub struct JoinedPanel {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    buffer: Buffer,
    scale_factor: f64,
    width: u32,
    height: u32,
}

impl JoinedPanel {
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> Self {
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(LATO_REGULAR.to_vec());
        let buffer = Buffer::new(&mut font_system, panel_metrics(scale_factor));
        let mut panel = Self {
            font_system,
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            renderer,
            buffer,
            scale_factor,
            width,
            height,
        };
        panel.resize(queue, width, height, scale_factor);
        panel
    }

    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32, scale_factor: f64) {
        self.width = width;
        self.height = height;
        self.scale_factor = scale_factor;
        self.viewport.update(queue, Resolution { width, height });
        self.buffer
            .set_metrics(&mut self.font_system, panel_metrics(scale_factor));
        self.buffer.set_size(
            &mut self.font_system,
            Some(width as f32 * 0.45),
            Some(height as f32),
        );
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        text: Option<&str>,
        dark: bool,
    ) -> Result<(), String> {
        if let Some(text) = text {
            self.buffer.set_text(
                &mut self.font_system,
                text,
                &Attrs::new().family(Family::Name("Lato")),
                Shaping::Advanced,
                None,
            );
            self.buffer.shape_until_scroll(&mut self.font_system, false);
        }
        let margin = (22.0 * self.scale_factor) as f32;
        let right = (self.width as f32 * 0.47) as i32;
        let bottom = i32::try_from(self.height).unwrap_or(i32::MAX);
        let color = if dark {
            Color::rgb(238, 245, 239)
        } else {
            Color::rgb(25, 35, 29)
        };
        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.buffer,
                    left: margin,
                    top: margin,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: margin as i32,
                        top: margin as i32,
                        right,
                        bottom,
                    },
                    default_color: color,
                    custom_glyphs: &[],
                }],
                &mut self.swash_cache,
            )
            .map_err(|error| error.to_string())
    }

    pub fn render<'a>(&'a self, pass: &mut RenderPass<'a>) -> Result<(), String> {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .map_err(|error| error.to_string())
    }

    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

fn panel_metrics(scale_factor: f64) -> Metrics {
    let scale = scale_factor as f32;
    Metrics::new(14.0 * scale, 20.0 * scale)
}
