use super::*;
#[path = "renderer/capture.rs"]
pub(crate) mod capture;
#[path = "renderer/color.rs"]
mod color;
use crate::server::SurfaceNode;
use lince_interface::host::raster::RasterMapping;
use smithay::{
    backend::{
        allocator::{Buffer as _, Fourcc},
        renderer::{
            Bind, Frame as _, ImportAll, ImportDma, Offscreen, Renderer, Texture, gles::GlesTexture,
        },
        winit::WinitGraphicsBackend,
    },
    input::pointer::{CursorImageStatus, CursorImageSurfaceData},
    reexports::wayland_server::Resource,
    utils::{Buffer, Physical, Rectangle as Rect, Size, Transform},
    wayland::{compositor::with_states, dmabuf::get_dmabuf},
};

struct View {
    texture: GlesTexture,
}

trait RasterRect {
    fn rectangle(self, rectangle: Rectangle) -> Rect<i32, Physical>;
}

impl RasterRect for RasterMapping {
    fn rectangle(self, rectangle: Rectangle) -> Rect<i32, Physical> {
        let (origin, size) = self.pixel_region(rectangle).unwrap_or(([0; 2], [0; 2]));
        Rect::new(
            (origin[0], origin[1]).into(),
            (size[0] as i32, size[1] as i32).into(),
        )
    }
}

#[cfg(any(feature = "session", test))]
pub(crate) fn output_element(
    renderer: &GlesRenderer,
    texture: GlesTexture,
    scale: f64,
) -> smithay::backend::renderer::element::utils::RescaleRenderElement<
    smithay::backend::renderer::element::texture::TextureRenderElement<GlesTexture>,
> {
    use smithay::backend::renderer::element::{
        Element, Kind,
        texture::{TextureBuffer, TextureRenderElement},
        utils::RescaleRenderElement,
    };
    let size = texture.size();
    let buffer = TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
    let element = TextureRenderElement::from_texture_buffer(
        (0.0, 0.0),
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    );
    let geometry = element.geometry(scale.into());
    RescaleRenderElement::from_element(
        element,
        (0, 0).into(),
        (
            f64::from(size.w) / f64::from(geometry.size.w),
            f64::from(size.h) / f64::from(geometry.size.h),
        ),
    )
}

struct Copy {
    view: View,
    _source: GlesTexture,
    _retained: Option<Arc<crate::graphics::RetainedCapture>>,
    buffer: Arc<crate::server::ClientBuffer>,
    fence: smithay::backend::renderer::sync::SyncPoint,
    _watch: crate::graphics::wait::Watch,
}

impl Drop for Copy {
    fn drop(&mut self) {
        if !self.fence.is_reached() {
            let _ = self.fence.wait();
        }
        self.buffer.release();
    }
}

#[derive(Default)]
pub(crate) struct Composition {
    color: Option<smithay::backend::renderer::gles::GlesTexProgram>,
    output_color: Option<smithay::backend::renderer::gles::GlesTexProgram>,
    color_failure: Option<String>,
    linear_composition: bool,
    linear_targets: HashMap<Option<OutputId>, GlesTexture>,
    capture: capture::Captures,
    reactor: Option<crate::graphics::wait::Reactor>,
    copies: HashMap<(WlSurface, u64), Copy>,
    acquiring: HashMap<(WlSurface, u64), crate::graphics::wait::Watch>,
    imports: HashMap<WlSurface, smithay::backend::renderer::utils::CommitCounter>,
    submitted: Option<smithay::backend::renderer::sync::SyncPoint>,
    submitted_watch: Option<crate::graphics::wait::Watch>,
    canvases: HashMap<OutputId, GlesTexture>,
    views: HashMap<(WlSurface, u64), View>,
    touched: HashSet<(WlSurface, u64)>,
    layers: HashMap<WlSurface, LayerSnapshot>,
    popups: HashMap<(WlSurface, OutputId), PopupScene>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum PopupOwner {
    Native,
    Application(lince_interface::host::SurfaceId),
    Layer(WlSurface),
}

#[derive(Clone)]
pub(crate) struct PopupScene {
    pub owner: PopupOwner,
    pub surface: WlSurface,
    pub output: OutputId,
    pub mapping: lince_interface::host::ContentMapping,
    pub nodes: Vec<SurfaceNode>,
    pub clips: Vec<Rectangle>,
}

impl PopupScene {
    pub fn point(&self, point: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|axis| {
            self.mapping.surface.origin()[axis]
                + (point[axis] - self.mapping.canvas.origin()[axis])
                    / self.mapping.canvas.size()[axis]
                    * self.mapping.surface.size()[axis]
        })
    }

    pub fn hit(&self, point: [f64; 2]) -> Option<WlSurface> {
        if !self.surface.is_alive() {
            return None;
        }
        if !self.clips.iter().any(|clip| clip.contains(point)) {
            return None;
        }
        let point = self.mapping.surface_point(point)?;
        self.nodes
            .iter()
            .filter(|node| node.surface.is_alive())
            .find_map(|node| node.hit(point).map(|_| node.surface.clone()))
    }
}

#[derive(Clone)]
pub(crate) struct TabletCursor {
    pub tool: u64,
    pub output: OutputId,
    pub cursor: capture::Cursor,
    pub owner: crate::server::CaptureContentOwner,
}

#[derive(Clone)]
pub(crate) struct Presented {
    pub colors: std::collections::BTreeMap<OutputId, u64>,
    pub cursor: capture::Cursor,
    pub tablets: Vec<TabletCursor>,
    pub drag: Vec<SurfaceNode>,
    pub pointer_owner: crate::server::CaptureContentOwner,
    pub drag_owner: crate::server::CaptureContentOwner,
    pub locked: bool,
    pub callbacks: Vec<(Option<OutputId>, Vec<SurfaceNode>)>,
    pub sync: smithay::backend::renderer::sync::SyncPoint,
    pub layers: Vec<LayerSnapshot>,
    pub clips: Vec<(OutputId, Rectangle)>,
    pub popups: Vec<PopupScene>,
}

impl Composition {
    pub(crate) fn enable_client_formats(&mut self, renderer: &GlesRenderer, server: &mut Server, frame: &crate::presentation::Frame) -> io::Result<()> {
        use smithay::backend::allocator::{Format, Modifier};
        if self.linear_color_available() && frame.encoding == lince_interface::host::color::NativeEncoding::LinearRelative {
            server.enable_hdr_color_management()?;
        } else { server.enable_sdr_color_management(); }
        let supported = renderer.egl_context().dmabuf_texture_formats();
        server.enable_dmabuf_formats(frame.client_formats.iter().copied().map(|code| Format { code, modifier: Modifier::Linear })
            .filter(|candidate| supported.iter().any(|format| format == candidate)));
        Ok(())
    }

    pub(super) fn output_raster(canvas: Option<&crate::presentation::Frame>, output: OutputId, target: RasterMapping) -> io::Result<RasterMapping> {
        let Some(snapshot) = canvas.map(|canvas| &canvas.input.mappings) else { return Ok(target); };
        if snapshot.outputs().is_empty() { return Ok(target); }
        snapshot.get(output).ok_or_else(|| io::Error::other("The presented output mapping is unavailable."))?
            .composition_raster(target).ok_or_else(|| io::Error::other("The compositor cannot compose this final output transform."))
    }

    pub(crate) fn new(wake: lince_interface::host::WakeSignal) -> io::Result<Self> {
        Ok(Self {
            reactor: Some(crate::graphics::wait::Reactor::new(move || wake.ring())?),
            ..Default::default()
        })
    }

    #[cfg(feature = "session")]
    pub(crate) fn ready(&self) -> bool {
        self.submitted.as_ref().is_none_or(|sync| sync.is_reached())
    }

    pub(crate) fn imports_changed(&self) -> bool {
        self.copies.values().any(|copy| copy.fence.is_reached())
            || self
                .acquiring
                .values()
                .any(crate::graphics::wait::Watch::ready)
    }

    pub(crate) fn watch(
        &self,
        sync: smithay::backend::renderer::sync::SyncPoint,
    ) -> io::Result<crate::graphics::wait::Watch> {
        self.reactor
            .as_ref()
            .ok_or_else(|| io::Error::other("The compositor fence reactor is unavailable"))?
            .fence(sync)
    }

    pub(crate) fn canvas(
        &mut self,
        renderer: &mut GlesRenderer,
        frame: &crate::presentation::Frame,
    ) -> io::Result<()> {
        let mut canvases = HashMap::new();
        if let Some(buffer) = frame.dmabuf() {
            canvases.insert(
            frame.output.unwrap_or(OutputId(1)),
            renderer
                .import_dmabuf(buffer, None)
                .map_err(error)?,
            );
        }
        for output in &frame.outputs {
            canvases.insert(
                output.id,
                renderer
                    .import_dmabuf(output.dmabuf(), None)
                    .map_err(error)?,
            );
        }
        self.canvases = canvases;
        Ok(())
    }

    fn import(&mut self, renderer: &mut GlesRenderer, node: &SurfaceNode) -> io::Result<bool> {
        self.import_source(renderer, node, false)
    }

    fn import_source(&mut self, renderer: &mut GlesRenderer, node: &SurfaceNode, retained: bool) -> io::Result<bool> {
        let retained = if retained {
            let Some(image) = crate::graphics::retained_capture(&node.content).map_err(io::Error::other)? else { return Ok(false); };
            if !image.image.ready().map_err(error)? { return Ok(false); }
            Some(image)
        } else { None };
        let key = (node.surface.clone(), node.content.version);
        self.touched.insert(key.clone());
        if self.views.contains_key(&key) {
            return Ok(true);
        }
        if let Some(copy) = self.copies.get(&key) {
            if !copy.fence.is_reached() {
                return Ok(false);
            }
            let texture = copy.view.texture.clone();
            self.views.insert(key.clone(), View { texture });
            self.copies.remove(&key);
            return Ok(true);
        }
        let pixels = u64::try_from(node.content.size[0])
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(node.content.size[1]).unwrap_or(u64::MAX));
        if pixels > 64 * 1024 * 1024 {
            return Err(io::Error::other(
                "The surface exceeds the compositor import budget",
            ));
        }
        let buffer_format = retained.as_ref().map(|image| image.image.dmabuf.format().code)
            .or_else(|| get_dmabuf(node.content.buffer()).ok().map(|buffer| buffer.format().code));
        let bytes_per_pixel = if buffer_format == Some(Fourcc::Abgr16161616f) { 8 } else { 4 };
        let pending_bytes: u64 = self
            .copies
            .values()
            .map(|copy| {
                let size = copy.view.texture.size();
                let bytes = if copy.view.texture.format() == Some(Fourcc::Abgr16161616f) { 8 } else { 4 };
                (size.w.max(0) as u64).saturating_mul(size.h.max(0) as u64).saturating_mul(bytes)
            })
            .sum();
        if self.copies.len() >= 128
            || self.acquiring.len() >= 128 && !self.acquiring.contains_key(&key)
            || pending_bytes.saturating_add(pixels.saturating_mul(bytes_per_pixel)) > 256 * 1024 * 1024
        {
            return Ok(false);
        }
        if retained.is_none() && let Ok(dmabuf) = get_dmabuf(node.content.buffer()) {
            for fd in dmabuf.handles() {
                let mut poll = [rustix::event::PollFd::new(
                    &fd,
                    rustix::event::PollFlags::IN,
                )];
                rustix::event::poll(
                    &mut poll,
                    Some(&rustix::time::Timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    }),
                )
                .map_err(error)?;
                if !poll[0].revents().contains(rustix::event::PollFlags::IN) {
                    if !self.acquiring.contains_key(&key) {
                        let watch = self
                            .reactor
                            .as_ref()
                            .ok_or_else(|| {
                                io::Error::other("The compositor fence reactor is unavailable")
                            })?
                            .descriptor(fd.try_clone_to_owned()?)?;
                        self.acquiring.insert(key, watch);
                    }
                    return Ok(false);
                }
            }
        }
        self.acquiring.remove(&key);
        let size: Size<i32, Buffer> = (node.content.size[0], node.content.size[1]).into();
        let damage = node.buffer_damage_since(self.imports.get(&node.surface).copied());
        let source = if let Some(image) = &retained {
            renderer.import_dmabuf(&image.image.dmabuf, Some(&damage)).map_err(error)?
        } else {
            renderer.import_buffer(node.content.buffer(), None, &damage)
                .ok_or_else(|| io::Error::other("unsupported layer buffer"))?.map_err(error)?
        };
        self.imports
            .insert(node.surface.clone(), node.damage.current_commit());
        let copy_format = match buffer_format.or_else(|| source.format()) {
            Some(Fourcc::Abgr16161616f) => Fourcc::Abgr16161616f,
            Some(Fourcc::Abgr2101010) => Fourcc::Abgr2101010,
            Some(Fourcc::Abgr8888) => Fourcc::Abgr8888,
            _ => Fourcc::Argb8888,
        };
        let mut texture: GlesTexture = renderer
            .create_buffer(copy_format, size)
            .map_err(error)?;
        let fence = {
            let mut target = renderer.bind(&mut texture).map_err(error)?;
            let dimensions: Size<i32, Physical> = (size.w, size.h).into();
            let damage = Rect::from_size(dimensions);
            let mut frame = renderer
                .render(&mut target, dimensions, Transform::Normal)
                .map_err(error)?;
            frame
                .clear(smithay::backend::renderer::Color32F::TRANSPARENT, &[damage])
                .map_err(error)?;
            frame
                .render_texture_at(
                    &source,
                    (0, 0).into(),
                    1,
                    1.0,
                    Transform::Normal,
                    &[damage],
                    &[],
                    1.0,
                )
                .map_err(error)?;
            frame.finish().map_err(error)?
        };
        if fence.is_reached() {
            node.content.release();
            self.views.insert(key, View { texture });
            Ok(true)
        } else {
            let watch = self.watch(fence.clone())?;
            self.copies.insert(
                key,
                Copy {
                    view: View { texture },
                    _source: source,
                    _retained: retained,
                    buffer: node.content.clone(),
                    fence,
                    _watch: watch,
                },
            );
            Ok(false)
        }
    }

    pub(crate) fn present(
        &mut self,
        backend: &mut WinitGraphicsBackend<GlesRenderer>,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
    ) -> io::Result<Presented> {
        let started = std::time::Instant::now();
        let size = backend.window_size();
        let presented = {
            let (renderer, mut target) = backend.bind().map_err(error)?;
            self.render(
                renderer,
                &mut target,
                size,
                Transform::Flipped180,
                server,
                canvas,
                pointer,
            )?
        };
        crate::runtime::trace_frame("composition", started);
        let display = backend
            .renderer()
            .egl_context()
            .display()
            .get_display_handle();
        if unsafe { smithay::backend::egl::ffi::egl::SwapInterval(**display, 0) }
            == smithay::backend::egl::ffi::egl::FALSE
        {
            return Err(io::Error::other(
                "Could not disable blocking nested buffer swaps",
            ));
        }
        let started = std::time::Instant::now();
        backend
            .submit(Some(&[Rect::from_size(size)]))
            .map_err(error)?;
        crate::runtime::trace_frame("nested-submit", started);
        backend.window().set_cursor_visible(false);
        Ok(presented)
    }

    pub(crate) fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
    ) -> io::Result<Presented> {
        self.render_view(
            renderer, target, size, transform, server, canvas, pointer, None, Default::default(),
        )
    }

    #[cfg(any(feature = "session", test))]
    pub(crate) fn render_output(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        output: &lince_interface::host::OutputViewport,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
        color: lince_interface::host::color::OutputColor,
    ) -> io::Result<Presented> {
        self.render_view(
            renderer,
            target,
            (
                output.source_size()[0] as i32,
                output.source_size()[1] as i32,
            )
                .into(),
            Transform::Normal,
            server,
            canvas,
            pointer,
            Some(output),
            color,
        )
    }

    fn render_view(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
        view: Option<&lince_interface::host::OutputViewport>,
        color: lince_interface::host::color::OutputColor,
    ) -> io::Result<Presented> {
        self.prepare_color(renderer)?;
        if color.mode == lince_interface::host::color::ColorMode::Hdr10
            && !matches!(target.format(), Some(Fourcc::Abgr2101010 | Fourcc::Xbgr2101010 | Fourcc::Argb2101010 | Fourcc::Xrgb2101010)) {
            return Err(io::Error::other("HDR10 output requires a ten-bit final render target"));
        }
        if color.mode == lince_interface::host::color::ColorMode::Hdr10
            && canvas.is_some_and(|canvas| canvas.frame.encoding != lince_interface::host::color::NativeEncoding::LinearRelative) {
            return Err(io::Error::other("HDR output requires a precision-preserving native scene"));
        }
        if !self.linear_color_available() {
            if color.mode != lince_interface::host::color::ColorMode::Srgb {
                return Err(io::Error::other(self.color_failure.clone().unwrap_or_else(|| "The linear output pipeline is unavailable".into())));
            }
            return self.render_scene(renderer, target, size, transform, server, canvas, pointer, view);
        }
        self.linear_targets.retain(|id, _| id.is_none() || server.output_viewports().iter().any(|(_, output)| Some(output.id) == *id));
        let key = view.map(|view| view.id);
        let mut linear = self.linear_targets.remove(&key).filter(|texture| texture.size().w == size.w && texture.size().h == size.h);
        if linear.is_none() {
            let bytes = self.linear_targets.values().map(|texture| texture.size().w.max(0) as u64 * texture.size().h.max(0) as u64 * 8).sum::<u64>();
            let candidate = u64::try_from(size.w).ok().zip(u64::try_from(size.h).ok())
                .and_then(|(width, height)| width.checked_mul(height)).and_then(|pixels| pixels.checked_mul(8));
            if candidate.and_then(|candidate| bytes.checked_add(candidate)).is_none_or(|bytes| bytes > 256 * 1024 * 1024) {
                return Err(io::Error::other("Linear output targets exceed the 256 MiB compositor budget"));
            }
            linear = Some(renderer.create_buffer(Fourcc::Abgr16161616f, (size.w, size.h).into()).map_err(error)?);
        }
        let mut linear = linear.unwrap();
        self.linear_composition = true;
        let result = (|| {
            let mut intermediate = renderer.bind(&mut linear).map_err(error)?;
            self.render_scene(renderer, &mut intermediate, size, Transform::Normal, server, canvas, pointer, view)
        })();
        self.linear_composition = false;
        let result = result.and_then(|mut scene| {
            let sync = self.encode_output(renderer, target, &linear, size, transform, color)?;
            scene.sync = sync.clone();
            self.submitted_watch = if sync.is_reached() { None } else { Some(self.watch(sync.clone())?) };
            self.submitted = Some(sync);
            Ok(scene)
        });
        self.linear_targets.insert(key, linear);
        result
    }

    fn render_scene(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
        view: Option<&lince_interface::host::OutputViewport>,
    ) -> io::Result<Presented> {
        self.prepare_color(renderer)?;
        server.enable_sdr_color_management();
        self.touched.clear();
        if server.locked() {
            return self.render_lock(renderer, target, size, transform, server, canvas.map(|canvas| &canvas.frame), view);
        }
        self.views.retain(|(surface, _), _| surface.is_alive());
        let layers = server.layers();
        let native_popups = server.native_popups();
        self.layers
            .retain(|surface, _| layers.iter().any(|layer| &layer.surface == surface));
        for layer in &layers {
            let mut ready = true;
            for node in &layer.nodes {
                match self.import(renderer, node) {
                    Ok(imported) => ready &= imported,
                    Err(error) => {
                        eprintln!("A Park layer buffer could not be imported: {error}");
                        server.reject_surface(&layer.surface);
                        ready = false;
                        break;
                    }
                }
            }
            if ready {
                self.layers.insert(layer.surface.clone(), layer.clone());
            }
        }
        let mut layers: Vec<_> = layers
            .iter()
            .filter_map(|layer| self.layers.get(&layer.surface).cloned())
            .collect();
        layers.sort_by_key(|layer| match layer.layer {
            Layer::Background => 0,
            Layer::Bottom => 1,
            Layer::Top => 2,
            Layer::Overlay => 3,
        });
        let cursor = server.cursor().clone();
        let mut tablet_cursors = Vec::new();
        let mut tablet_callbacks = Vec::new();
        for tablet in server.tablet_cursors() {
            let nodes = if let CursorImageStatus::Surface(surface) = &tablet.image {
                let nodes = server.surface_nodes(surface);
                let mut ready = true;
                for node in &nodes {
                    match self.import(renderer, node) {
                        Ok(imported) => ready &= imported,
                        Err(failure) => { eprintln!("A Park tablet cursor could not be imported: {failure}"); server.reject_surface(surface); ready = false; break; }
                    }
                }
                if ready { nodes } else { Vec::new() }
            } else { Vec::new() };
            tablet_callbacks.push((Some(tablet.output), nodes.clone()));
            tablet_cursors.push(TabletCursor { tool: tablet.tool, output: tablet.output, owner: tablet.owner,
                cursor: capture::Cursor::new(tablet.image, nodes, tablet.point) });
        }
        let drag_nodes = server.drag_nodes();
        let mut drag_ready = true;
        for node in &drag_nodes {
            drag_ready &= self.import(renderer, node).unwrap_or(false);
        }
        let cursor_nodes = if let CursorImageStatus::Surface(surface) = &cursor {
            let nodes = server.surface_nodes(surface);
            let mut ready = true;
            for node in &nodes {
                match self.import(renderer, node) {
                    Ok(imported) => ready &= imported,
                    Err(error) => {
                        eprintln!("A Park cursor buffer could not be imported: {error}");
                        server.reject_surface(surface);
                        ready = false;
                        break;
                    }
                }
            }
            if ready { nodes } else { Vec::new() }
        } else {
            Vec::new()
        };
        let damage = Rect::from_size(size);
        let output_fullscreen = |id| {
            canvas.is_some_and(|canvas| {
                if canvas.frame.output == Some(id) {
                    canvas.frame.fullscreen
                } else {
                    canvas
                        .frame
                        .outputs
                        .iter()
                        .find(|output| output.id == id)
                        .is_some_and(|output| output.fullscreen)
                }
            })
        };
        let plans: Vec<_> = canvas
            .and_then(|canvas| canvas.frame.geometry.as_ref())
            .into_iter()
            .flat_map(|geometry| geometry.outputs.iter())
            .filter_map(|output| {
                let rectangle = if output_fullscreen(output.id) {
                    Some(output.bounds)
                } else {
                    output.usable
                }?;
                Some((output.id, rectangle))
            })
            .collect();
        let mut popup_plans = Vec::new();
        let mut applications = HashSet::new();
        if let Some(canvas) = canvas
            && let Some(geometry) = &canvas.frame.geometry
        {
            for item in &geometry.items {
                let ItemId::Application(id) = item.id else {
                    continue;
                };
                let Some(window) = canvas.windows.iter().find(|window| window.id == id) else {
                    continue;
                };
                let Some(output) = geometry
                    .outputs
                    .iter()
                    .find(|output| output.id == item.output)
                else {
                    continue;
                };
                for popup in window.popups.iter().rev() {
                    let Some(mapping) = popup.mapping(window, item) else {
                        continue;
                    };
                    let clips =
                        if canvas.frame.input.modal && canvas.frame.output == Some(output.id) {
                            Vec::new()
                        } else {
                            vec![output.bounds]
                        };
                    popup_plans.push((
                        popup,
                        PopupScene {
                            owner: PopupOwner::Application(id),
                            surface: popup.surface.clone(),
                            output: output.id,
                            mapping,
                            nodes: popup.nodes.clone(),
                            clips,
                        },
                    ));
                }
            }
            for layer in &layers {
                if output_fullscreen(layer.output) && layer.layer != Layer::Overlay {
                    continue;
                }
                let Some(output) = geometry
                    .outputs
                    .iter()
                    .find(|output| output.id == layer.output)
                else {
                    continue;
                };
                for popup in layer.popups.iter().rev() {
                    let Some(mapping) = popup.layer_mapping(layer.origin.map(f64::from)) else {
                        continue;
                    };
                    popup_plans.push((
                        popup,
                        PopupScene {
                            owner: PopupOwner::Layer(layer.surface.clone()),
                            surface: popup.surface.clone(),
                            output: layer.output,
                            mapping,
                            nodes: popup.nodes.clone(),
                            clips: vec![output.bounds],
                        },
                    ));
                }
            }
        }
        if let Some(geometry) = canvas.and_then(|canvas| canvas.frame.geometry.as_ref()) {
            for popup in &native_popups {
                let Some(mapping) = popup.layer_mapping([0.0; 2]) else {
                    continue;
                };
                let parent = canvas.and_then(|canvas| canvas.frame.input.editor.as_ref());
                let Some(output) = parent.and_then(|editor| {
                    geometry.outputs.iter().find(|output| {
                        output
                            .bounds
                            .contains([f64::from(editor.caret[0]), f64::from(editor.caret[1])])
                    })
                }) else {
                    continue;
                };
                popup_plans.push((
                    popup,
                    PopupScene {
                        owner: PopupOwner::Native,
                        surface: popup.surface.clone(),
                        output: output.id,
                        mapping,
                        nodes: popup.nodes.clone(),
                        clips: vec![output.usable.unwrap_or(output.bounds)],
                    },
                ));
            }
        }
        let mut visible_popups = Vec::new();
        let mut popup_surfaces = HashSet::new();
        for (popup, scene) in popup_plans {
            let key = (popup.surface.clone(), scene.output);
            popup_surfaces.insert(key.clone());
            let mut ready = true;
            for node in &popup.nodes {
                match self.import(renderer, node) {
                    Ok(imported) => ready &= imported,
                    Err(_) => {
                        server.reject_surface(&popup.surface);
                        ready = false;
                    }
                }
            }
            if ready {
                self.popups.insert(key.clone(), scene);
            } else if let Some(previous) = self.popups.get_mut(&key) {
                previous.clips = scene.clips;
            }
            if let Some(scene) = self.popups.get(&key) {
                visible_popups.push(scene.clone());
            }
        }
        self.popups
            .retain(|surface, _| popup_surfaces.contains(surface));
        let mut callbacks = Vec::new();
        for layer in layers
            .iter()
            .filter(|layer| layer.layer == Layer::Overlay || !output_fullscreen(layer.output))
        {
            callbacks.push((Some(layer.output), layer.nodes.clone()));
        }
        let pointer_output = server
            .output_viewports()
            .into_iter()
            .find(|(_, output)| output.bounds.contains(pointer))
            .map(|(_, output)| output.id);
        callbacks.push((pointer_output, cursor_nodes.clone()));
        callbacks.extend(tablet_callbacks);
        if drag_ready {
            callbacks.push((pointer_output, drag_nodes.clone()));
        }
        for popup in &visible_popups {
            if !popup.clips.is_empty() {
                callbacks.push((Some(popup.output), popup.nodes.clone()));
            }
        }
        if let Some(canvas) = canvas
            && let Some(geometry) = &canvas.frame.geometry
        {
            for item in &geometry.items {
                let ItemId::Application(id) = item.id else {
                    continue;
                };
                let rectangle = item.mapping.canvas;
                let overlap = plans.iter().any(|(output, clip)| {
                    *output == item.output
                        && (0..2).all(|axis| {
                            rectangle.origin()[axis] < clip.origin()[axis] + clip.size()[axis]
                                && rectangle.origin()[axis] + rectangle.size()[axis]
                                    > clip.origin()[axis]
                        })
                });
                if overlap
                    && let Some(window) = canvas.windows.iter().find(|window| window.id == id)
                {
                    applications.insert(id);
                    callbacks.push((Some(item.output), server.callback_nodes(window)));
                }
            }
        }
        if let Some(canvas) = canvas
            && let Some(geometry) = &canvas.frame.geometry
        {
            let output = canvas
                .frame
                .output
                .or_else(|| geometry.outputs.first().map(|output| output.id));
            if let Some(output) = output {
                for id in &canvas.frame.previewed {
                    if applications.insert(*id)
                        && let Some(window) = canvas.windows.iter().find(|window| window.id == *id)
                    {
                        callbacks.push((Some(output), server.callback_nodes(window)));
                    }
                }
            }
        }
        let mut presented = Presented {
            colors: server.output_color_revisions(),
            cursor: capture::Cursor::new(cursor, cursor_nodes.clone(), pointer),
            tablets: tablet_cursors,
            drag: if drag_ready { drag_nodes } else { Vec::new() },
            pointer_owner: server.capture_cursor_owner(),
            drag_owner: server.capture_drag_owner(),
            locked: false,
            callbacks,
            sync: smithay::backend::renderer::sync::SyncPoint::signaled(),
            layers,
            clips: plans,
            popups: visible_popups,
        };
        let mapping = view.map_or_else(RasterMapping::default, |view| view.raster_mapping());
        let sync = {
            let mut frame = renderer.render(target, size, transform).map_err(error)?;
            frame
                .clear(
                    self.sdr_solid([0.08, 0.08, 0.08, 1.0]),
                    &[damage],
                )
                .map_err(error)?;
            for (_, output) in server
                .output_viewports()
                .into_iter()
                .filter(|(_, output)| view.is_none_or(|view| view.id == output.id))
            {
                let clip = if view.is_some() {
                    Some(damage)
                } else {
                    mapping.rectangle(output.bounds).intersection(damage)
                };
                if let Some(clip) = clip {
                    let output_mapping = Self::output_raster(canvas.map(|canvas| &canvas.frame), output.id, mapping)?;
                    self.draw_output(
                        &mut frame,
                        canvas.map(|canvas| &canvas.frame),
                        &presented,
                        output.id,
                        output_mapping,
                        self.canvases.get(&output.id),
                        clip,
                    )?;
                }
            }
            if let Some(output) = server.output_viewports().into_iter().map(|(_, output)| output)
                .find(|output| Some(output.id) == pointer_output && view.is_none_or(|view| view.id == output.id))
            {
                let pointer_mapping = Self::output_raster(canvas.map(|canvas| &canvas.frame), output.id, mapping)?;
                if let Some(clip) = mapping.rectangle(output.bounds).intersection(damage) {
                    self.capture_cursor_draw(&mut frame, &presented.cursor, pointer_mapping, clip)?;
                    if drag_ready {
                        self.draw_scaled(&mut frame, &presented.drag, pointer_mapping.point(pointer), pointer_mapping.scale, clip)?;
                    }
                }
            }
            if !presented.tablets.is_empty() {
                for (_, output) in server.output_viewports().into_iter().filter(|(_, output)| view.is_none_or(|view| view.id == output.id)) {
                    let output_mapping = Self::output_raster(canvas.map(|canvas| &canvas.frame), output.id, mapping)?;
                    if let Some(clip) = mapping.rectangle(output.bounds).intersection(damage) {
                        for tablet in presented.tablets.iter().filter(|tablet| tablet.output == output.id) {
                            self.capture_cursor_draw(&mut frame, &tablet.cursor, output_mapping, clip)?;
                        }
                    }
                }
            }
            frame.finish().map_err(error)?
        };
        self.submitted_watch = if sync.is_reached() {
            None
        } else {
            Some(self.watch(sync.clone())?)
        };
        self.submitted = Some(sync.clone());
        presented.sync = sync;
        self.prune_views();
        Ok(presented)
    }

    fn render_lock(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
        server: &mut Server,
        canvas: Option<&crate::presentation::Frame>,
        view: Option<&lince_interface::host::OutputViewport>,
    ) -> io::Result<Presented> {
        let mapping = view.map_or_else(RasterMapping::default, |view| view.raster_mapping());
        let surfaces = server.lock_surfaces();
        let mut ready = Vec::new();
        for (id, origin, nodes) in surfaces
            .into_iter()
            .filter(|(id, _, _)| view.is_none_or(|view| view.id == *id))
        {
            let mut complete = true;
            for node in &nodes {
                complete &= self.import(renderer, node).unwrap_or(false);
            }
            if complete {
                ready.push((id, origin, nodes));
            }
        }
        let damage = Rect::from_size(size);
        let mut frame = renderer.render(target, size, transform).map_err(error)?;
        frame
            .clear(smithay::backend::renderer::Color32F::BLACK, &[damage])
            .map_err(error)?;
        for (id, origin, nodes) in &ready {
            let output_mapping = Self::output_raster(canvas, *id, mapping)?;
            let clip = server.output_viewports().into_iter().find(|(_, output)| output.id == *id)
                .and_then(|(_, output)| mapping.rectangle(output.bounds).intersection(damage));
            let Some(clip) = clip else { continue; };
            self.draw_scaled(
                &mut frame,
                nodes,
                output_mapping.point(origin.map(f64::from)),
                output_mapping.scale,
                clip,
            )?;
        }
        let sync = frame.finish().map_err(error)?;
        self.submitted_watch = if sync.is_reached() {
            None
        } else {
            Some(self.watch(sync.clone())?)
        };
        self.submitted = Some(sync.clone());
        self.prune_views();
        Ok(Presented {
            colors: server.output_color_revisions(),
            cursor: capture::Cursor::hidden(),
            tablets: Vec::new(),
            drag: Vec::new(),
            pointer_owner: crate::server::CaptureContentOwner::Unknown,
            drag_owner: crate::server::CaptureContentOwner::Unknown,
            locked: true,
            callbacks: ready
                .into_iter()
                .map(|(id, _, nodes)| (Some(id), nodes))
                .collect(),
            sync,
            layers: Vec::new(),
            clips: Vec::new(),
            popups: Vec::new(),
        })
    }

    fn prune_views(&mut self) {
        let mut retained = self.touched.clone();
        for node in self
            .layers
            .values()
            .flat_map(|layer| &layer.nodes)
            .chain(self.popups.values().flat_map(|popup| &popup.nodes))
        {
            retained.insert((node.surface.clone(), node.content.version));
        }
        self.views
            .retain(|key, _| retained.contains(key) && key.0.is_alive());
        self.acquiring
            .retain(|key, _| retained.contains(key) && key.0.is_alive());
        self.imports.retain(|surface, _| {
            surface.is_alive() && retained.iter().any(|(existing, _)| existing == surface)
        });
        self.copies.retain(|key, copy| {
            !copy.fence.is_reached() || retained.contains(key) && key.0.is_alive()
        });
    }

    fn draw_scaled(
        &self,
        frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>,
        nodes: &[SurfaceNode],
        origin: [f64; 2],
        scale: [f64; 2],
        damage: Rect<i32, Physical>,
    ) -> io::Result<()> {
        for node in nodes.iter().rev() {
            let Some(view) = self
                .views
                .get(&(node.surface.clone(), node.content.version))
            else {
                continue;
            };
            let location = std::array::from_fn::<_, 2, _>(|axis| {
                origin[axis] + f64::from(node.origin[axis]) * scale[axis]
            });
            let extent =
                std::array::from_fn::<_, 2, _>(|axis| node.layout.size[axis] * scale[axis]);
            if location
                .iter()
                .chain(extent.iter())
                .any(|value| !value.is_finite() || value.abs() > 1_048_576.0)
                || extent.iter().any(|value| *value <= 0.0)
            {
                continue;
            }
            let points = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]].map(|point| {
                [
                    node.layout.uv_origin[0]
                        + node.layout.uv_x[0] * point[0]
                        + node.layout.uv_y[0] * point[1],
                    node.layout.uv_origin[1]
                        + node.layout.uv_x[1] * point[0]
                        + node.layout.uv_y[1] * point[1],
                ]
            });
            let low = points
                .into_iter()
                .fold([f32::INFINITY; 2], |a, b| [a[0].min(b[0]), a[1].min(b[1])]);
            let high = points.into_iter().fold([f32::NEG_INFINITY; 2], |a, b| {
                [a[0].max(b[0]), a[1].max(b[1])]
            });
            let texture_size = view.texture.size();
            let source = Rect::new(
                (
                    f64::from(low[0]) * f64::from(texture_size.w),
                    f64::from(low[1]) * f64::from(texture_size.h),
                )
                    .into(),
                (
                    f64::from(high[0] - low[0]) * f64::from(texture_size.w),
                    f64::from(high[1] - low[1]) * f64::from(texture_size.h),
                )
                    .into(),
            );
            let destination = Rect::new(
                (location[0].round() as i32, location[1].round() as i32).into(),
                (
                    ((location[0] + extent[0]).round() - location[0].round()) as i32,
                    ((location[1] + extent[1]).round() - location[1].round()) as i32,
                )
                    .into(),
            );
            let Some(damage) = local_damage(destination, damage) else {
                continue;
            };
            let opaque: Vec<Rect<i32, Physical>> = node
                .opaque_rectangles()
                .into_iter()
                .filter_map(|rectangle| {
                    let low = [
                        (location[0] + rectangle.loc.x * scale[0]).ceil() as i32
                            - destination.loc.x,
                        (location[1] + rectangle.loc.y * scale[1]).ceil() as i32
                            - destination.loc.y,
                    ];
                    let high = [
                        (location[0] + (rectangle.loc.x + rectangle.size.w) * scale[0]).floor()
                            as i32
                            - destination.loc.x,
                        (location[1] + (rectangle.loc.y + rectangle.size.h) * scale[1]).floor()
                            as i32
                            - destination.loc.y,
                    ];
                    (high[0] > low[0] && high[1] > low[1]).then(|| {
                        Rect::new(
                            (low[0], low[1]).into(),
                            (high[0] - low[0], high[1] - low[1]).into(),
                        )
                    })
                })
                .collect();
            self.color_surface(frame, node)?;
            let result = smithay::backend::renderer::Frame::render_texture_from_to(
                frame,
                &view.texture,
                source,
                destination,
                &[damage],
                &opaque,
                node.transform,
                1.0,
            );
            frame.clear_tex_program_override();
            result.map_err(error)?;
        }
        Ok(())
    }
}

fn local_damage(
    destination: Rect<i32, Physical>,
    clip: Rect<i32, Physical>,
) -> Option<Rect<i32, Physical>> {
    let intersection = destination.intersection(clip)?;
    Some(Rect::new(
        intersection.loc - destination.loc,
        intersection.size,
    ))
}

#[cfg(test)]
#[path = "renderer/tests.rs"]
pub(crate) mod tests;
