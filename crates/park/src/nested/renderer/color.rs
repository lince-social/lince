use super::*;
use smithay::backend::renderer::gles::{GlesFrame, Uniform, UniformName, UniformType};

impl Composition {
    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn color_test_resources(&self) -> [usize; 8] {
        [self.linear_targets.len(), self.copies.len(), self.views.len(), self.acquiring.len(), self.layers.len(), self.popups.len(), self.canvases.len(),
            self.linear_targets.values().map(|texture| texture.size().w as usize * texture.size().h as usize * 8).sum()]
    }

    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn color_test_surface_ready(&self, surface: &WlSurface, version: u64) -> bool {
        self.views.contains_key(&(surface.clone(), version))
    }

    pub(crate) fn prepare_color(&mut self, renderer: &mut GlesRenderer) -> io::Result<()> {
        if self.color.is_none() {
            let source = include_str!("color.frag").replace("PARK_SHADER_DEFINES", "//_DEFINES_");
            let uniforms = ["color_red", "color_green", "color_blue", "color_curve"].map(|name| UniformName::new(name, UniformType::_4f));
            self.color = Some(renderer.compile_custom_texture_shader(source, &uniforms).map_err(error)?);
        }
        if self.output_color.is_none() && self.color_failure.is_none() {
            match self.prepare_output_color(renderer) {
                Ok(program) => self.output_color = Some(program),
                Err(failure) => { self.color_failure = Some(failure.to_string()); eprintln!("Park linear output color is unavailable: {failure}"); }
            }
        }
        Ok(())
    }

    pub(crate) fn linear_color_available(&self) -> bool { self.output_color.is_some() }

    fn prepare_output_color(&self, renderer: &mut GlesRenderer) -> io::Result<smithay::backend::renderer::gles::GlesTexProgram> {
        use smithay::backend::allocator::{Format, Modifier};
        if !renderer.egl_context().dmabuf_texture_formats().contains(&Format { code: Fourcc::Abgr16161616f, modifier: Modifier::Linear }) {
            return Err(io::Error::other("The EGL renderer cannot import linear RGBA16F scene buffers"));
        }
        let mut probe: GlesTexture = renderer.create_buffer(Fourcc::Abgr16161616f, (1, 1).into()).map_err(error)?;
        let mut target = renderer.bind(&mut probe).map_err(error)?;
        let mut frame = renderer.render(&mut target, (1, 1).into(), Transform::Normal).map_err(error)?;
        frame.clear(smithay::backend::renderer::Color32F::TRANSPARENT, &[Rect::from_size((1, 1).into())]).map_err(error)?;
        drop(frame.finish().map_err(error)?);
        drop(target);
        let source = include_str!("output.frag").replace("PARK_SHADER_DEFINES", "//_DEFINES_");
        let uniforms = ["output_red", "output_green", "output_blue", "output_curve"].map(|name| UniformName::new(name, UniformType::_4f));
        renderer.compile_custom_texture_shader(source, &uniforms).map_err(error)
    }

    pub(super) fn color_surface(&self, frame: &mut GlesFrame<'_, '_>, node: &SurfaceNode) -> io::Result<()> {
        if !node.color.supported() { return Err(io::Error::other("This surface color description has no implemented conversion")); }
        if node.color == Default::default() && !self.linear_composition { return Ok(()); }
        self.color_texture(frame, node.color, node.fully_opaque(), false)
    }

    pub(super) fn color_canvas(&self, frame: &mut GlesFrame<'_, '_>, encoding: lince_interface::host::color::NativeEncoding) -> io::Result<()> {
        use lince_interface::host::color::{Description, NativeEncoding};
        if encoding == NativeEncoding::Srgb && !self.linear_composition { return Ok(()); }
        self.color_texture(frame, if encoding == NativeEncoding::LinearRelative { Description::linear_working() } else { Description::default() }, false, encoding == NativeEncoding::LinearRelative)
    }

    pub(super) fn color_texture(&self, frame: &mut GlesFrame<'_, '_>, description: lince_interface::host::color::Description, opaque: bool, native_linear: bool) -> io::Result<()> {
        use lince_interface::host::color::Transfer;
        let program = self.color.as_ref().ok_or_else(|| io::Error::other("The surface color program is unavailable"))?;
        let [red, green, blue, mut curve] = description.uniforms();
        let absolute = matches!(description.transfer, Transfer::Pq | Transfer::Linear) && !native_linear;
        if self.linear_composition && !absolute { curve[1] *= 203.0 / 80.0; }
        else if !self.linear_composition && absolute { curve[1] *= 80.0 / 203.0; }
        curve[2] = if opaque { 1.0 } else { 0.0 };
        curve[3] = if self.linear_composition { 1.0 } else { 0.0 };
        let values = [red, green, blue, curve];
        let uniforms = ["color_red", "color_green", "color_blue", "color_curve"].into_iter().zip(values)
            .map(|(name, value)| Uniform::new(name, (value[0], value[1], value[2], value[3]))).collect();
        frame.override_default_tex_program(program.clone(), uniforms);
        Ok(())
    }

    pub(super) fn sdr_solid(&self, color: [f32; 4]) -> smithay::backend::renderer::Color32F {
        if !self.linear_composition { return smithay::backend::renderer::Color32F::new(color[0], color[1], color[2], color[3]); }
        let rgb = std::array::from_fn::<_, 3, _>(|index| {
            lince_interface::host::color::Transfer::Srgb.decode(f64::from(color[index])).unwrap_or(0.0) as f32 * (203.0 / 80.0)
        });
        smithay::backend::renderer::Color32F::new(rgb[0], rgb[1], rgb[2], color[3])
    }

    pub(super) fn encode_output(&self, renderer: &mut GlesRenderer, target: &mut smithay::backend::renderer::gles::GlesTarget<'_>, source: &GlesTexture,
        size: Size<i32, Physical>, transform: Transform, color: lince_interface::host::color::OutputColor) -> io::Result<smithay::backend::renderer::sync::SyncPoint> {
        let started = std::time::Instant::now();
        let program = self.output_color.as_ref().ok_or_else(|| io::Error::other("The final color program is unavailable"))?;
        let values = color.mode.output_uniforms(color.peak_nits).ok_or_else(|| io::Error::other("The output color range is invalid"))?;
        let uniforms = ["output_red", "output_green", "output_blue", "output_curve"].into_iter().zip(values)
            .map(|(name, value)| Uniform::new(name, (value[0], value[1], value[2], value[3]))).collect();
        let damage = Rect::from_size(transform.transform_size(size));
        let mut frame = renderer.render(target, size, transform).map_err(error)?;
        frame.clear(smithay::backend::renderer::Color32F::TRANSPARENT, &[damage]).map_err(error)?;
        frame.override_default_tex_program(program.clone(), uniforms);
        let result = frame.render_texture_from_to(source, Rect::from_size(source.size().to_f64()), damage, &[damage], &[], Transform::Normal, 1.0, None, &[]).map_err(error);
        frame.clear_tex_program_override();
        result?;
        let sync = frame.finish().map_err(error)?;
        crate::runtime::trace_frame("color-encode-submit", started);
        Ok(sync)
    }
}
