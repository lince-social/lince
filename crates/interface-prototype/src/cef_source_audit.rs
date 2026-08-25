pub fn accelerated_paint_metadata_size() -> usize {
    std::mem::size_of::<cef::AcceleratedPaintInfo>()
}

pub fn render_handler_type_name() -> &'static str {
    std::any::type_name::<cef::RenderHandler>()
}
