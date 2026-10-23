use super::*;
#[path = "renderer/capture.rs"]
pub(crate) mod capture;
use crate::server::SurfaceNode;
use smithay::{
    backend::{
        allocator::Fourcc,
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

struct Copy {
    view: View,
    _source: GlesTexture,
    buffer: Arc<crate::server::ClientBuffer>,
    fence: smithay::backend::renderer::sync::SyncPoint,
    _watch: crate::graphics::wait::Watch,
}

impl Drop for Copy {
    fn drop(&mut self) {
        if !self.fence.is_reached() { let _ = self.fence.wait(); }
        self.buffer.release();
    }
}

#[derive(Default)]
pub(crate) struct Composition {
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

pub(crate) struct Presented {
    pub cursor: capture::Cursor,
    pub drag: Vec<SurfaceNode>,
    pub locked: bool,
    pub callbacks: Vec<(Option<OutputId>, Vec<SurfaceNode>)>,
    pub sync: smithay::backend::renderer::sync::SyncPoint,
    pub layers: Vec<LayerSnapshot>,
    pub clips: Vec<(OutputId, Rectangle)>,
    pub popups: Vec<PopupScene>,
}

impl Composition {
    pub(crate) fn new(wake: lince_interface::host::WakeSignal) -> io::Result<Self> {
        Ok(Self { reactor: Some(crate::graphics::wait::Reactor::new(move || wake.ring())?), ..Default::default() })
    }

    #[cfg(feature = "session")]
    pub(crate) fn ready(&self) -> bool {
        self.submitted.as_ref().is_none_or(|sync| sync.is_reached())
    }

    pub(crate) fn imports_changed(&self) -> bool {
        self.copies.values().any(|copy| copy.fence.is_reached()) || self.acquiring.values().any(crate::graphics::wait::Watch::ready)
    }

    pub(crate) fn watch(&self, sync: smithay::backend::renderer::sync::SyncPoint) -> io::Result<crate::graphics::wait::Watch> {
        self.reactor.as_ref().ok_or_else(|| io::Error::other("The compositor fence reactor is unavailable"))?.fence(sync)
    }

    pub(crate) fn canvas(
        &mut self,
        renderer: &mut GlesRenderer,
        frame: &crate::presentation::Frame,
    ) -> io::Result<()> {
        let mut canvases = HashMap::new();
        canvases.insert(
            frame.output.unwrap_or(OutputId(1)),
            renderer
                .import_dmabuf(frame.dmabuf(), None)
                .map_err(error)?,
        );
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
        let key = (node.surface.clone(), node.content.version);
        self.touched.insert(key.clone());
        if self.views.contains_key(&key) {
            return Ok(true);
        }
        if let Some(copy) = self.copies.get(&key) {
            if !copy.fence.is_reached() { return Ok(false); }
            let texture = copy.view.texture.clone();
            self.views.insert(key.clone(), View { texture });
            self.copies.remove(&key);
            return Ok(true);
        }
        let pixels = u64::try_from(node.content.size[0]).unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(node.content.size[1]).unwrap_or(u64::MAX));
        if pixels > 64 * 1024 * 1024 { return Err(io::Error::other("The surface exceeds the compositor import budget")); }
        let pending_pixels: u64 = self.copies.values().map(|copy| {
            let size = copy.view.texture.size();
            size.w.max(0) as u64 * size.h.max(0) as u64
        }).sum();
        if self.copies.len() >= 128 || self.acquiring.len() >= 128 && !self.acquiring.contains_key(&key)
            || pending_pixels.saturating_add(pixels) > 64 * 1024 * 1024 {
            return Ok(false);
        }
        if let Ok(dmabuf) = get_dmabuf(node.content.buffer()) {
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
                        let watch = self.reactor.as_ref().ok_or_else(|| io::Error::other("The compositor fence reactor is unavailable"))?.descriptor(fd.try_clone_to_owned()?)?;
                        self.acquiring.insert(key, watch);
                    }
                    return Ok(false);
                }
            }
        }
        self.acquiring.remove(&key);
        let size: Size<i32, Buffer> = (node.content.size[0], node.content.size[1]).into();
        let damage = node.buffer_damage_since(self.imports.get(&node.surface).copied());
        let source = renderer
            .import_buffer(node.content.buffer(), None, &damage)
            .ok_or_else(|| io::Error::other("unsupported layer buffer"))?
            .map_err(error)?;
        self.imports.insert(node.surface.clone(), node.damage.current_commit());
        let mut texture: GlesTexture = renderer
            .create_buffer(Fourcc::Argb8888, size)
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
            self.copies.insert(key, Copy { view: View { texture }, _source: source, buffer: node.content.clone(), fence, _watch: watch });
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
        let display = backend.renderer().egl_context().display().get_display_handle();
        if unsafe { smithay::backend::egl::ffi::egl::SwapInterval(**display, 0) }
            == smithay::backend::egl::ffi::egl::FALSE
        {
            return Err(io::Error::other("Could not disable blocking nested buffer swaps"));
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
        self.touched.clear();
        if server.locked() {
            return self.render_lock(renderer, target, size, transform, server);
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
                let Some(mapping) = popup.layer_mapping([0.0; 2]) else { continue };
                let parent = canvas.and_then(|canvas| canvas.frame.input.editor.as_ref());
                let Some(output) = parent.and_then(|editor| geometry.outputs.iter().find(|output|
                    output.bounds.contains([f64::from(editor.caret[0]), f64::from(editor.caret[1])]))) else { continue };
                popup_plans.push((popup, PopupScene { owner: PopupOwner::Native,
                    surface: popup.surface.clone(), output: output.id, mapping,
                    nodes: popup.nodes.clone(), clips: vec![output.usable.unwrap_or(output.bounds)] }));
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
        let sync = {
            let mut frame = renderer.render(target, size, transform).map_err(error)?;
            frame
                .clear(
                    smithay::backend::renderer::Color32F::new(0.08, 0.08, 0.08, 1.0),
                    &[damage],
                )
                .map_err(error)?;
            for layer in layers.iter().filter(|layer| {
                !output_fullscreen(layer.output)
                    && matches!(layer.layer, Layer::Background | Layer::Bottom)
            }) {
                self.draw(
                    &mut frame,
                    &layer.nodes,
                    layer.origin.map(f64::from),
                    damage,
                )?;
                for popup in visible_popups
                    .iter()
                    .filter(|popup| popup.owner == PopupOwner::Layer(layer.surface.clone()))
                {
                    self.draw_popup(&mut frame, popup)?;
                }
            }
            for (id, rectangle) in &plans {
                let Some(texture) = self.canvases.get(id) else {
                    continue;
                };
                let clip: Rect<i32, Physical> = Rect::new(
                    (
                        rectangle.origin()[0].round() as i32,
                        rectangle.origin()[1].round() as i32,
                    )
                        .into(),
                    (
                        rectangle.size()[0].round() as i32,
                        rectangle.size()[1].round() as i32,
                    )
                        .into(),
                );
                frame
                    .render_texture_from_to(
                        texture,
                        Rect::from_size(texture.size().to_f64()),
                        clip,
                        &[Rect::from_size(clip.size)],
                        &[],
                        Transform::Normal,
                        1.0,
                        None,
                        &[],
                    )
                    .map_err(error)?;
            }
            for popup in visible_popups
                .iter()
                .filter(|popup| matches!(popup.owner, PopupOwner::Application(_) | PopupOwner::Native))
            {
                self.draw_popup(&mut frame, popup)?;
            }
            for layer in layers.iter().filter(|layer| {
                layer.layer == Layer::Overlay
                    || !output_fullscreen(layer.output) && layer.layer == Layer::Top
            }) {
                self.draw(
                    &mut frame,
                    &layer.nodes,
                    layer.origin.map(f64::from),
                    damage,
                )?;
                for popup in visible_popups
                    .iter()
                    .filter(|popup| popup.owner == PopupOwner::Layer(layer.surface.clone()))
                {
                    self.draw_popup(&mut frame, popup)?;
                }
            }
            if let CursorImageStatus::Surface(surface) = &cursor {
                let hotspot = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<CursorImageSurfaceData>()
                        .map(|data| {
                            data.lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .hotspot
                        })
                        .unwrap_or_default()
                });
                self.draw(
                    &mut frame,
                    &cursor_nodes,
                    [
                        pointer[0] - f64::from(hotspot.x),
                        pointer[1] - f64::from(hotspot.y),
                    ],
                    damage,
                )?;
            }
            if matches!(cursor, CursorImageStatus::Named(_)) {
                let position = (pointer[0] as i32, pointer[1] as i32);
                frame
                    .draw_solid(
                        Rect::new(position.into(), (4, 20).into()),
                        &[Rect::from_size((4, 20).into())],
                        smithay::backend::renderer::Color32F::new(1.0, 1.0, 1.0, 1.0),
                    )
                    .map_err(error)?;
                frame
                    .draw_solid(
                        Rect::new(position.into(), (14, 4).into()),
                        &[Rect::from_size((14, 4).into())],
                        smithay::backend::renderer::Color32F::new(1.0, 1.0, 1.0, 1.0),
                    )
                    .map_err(error)?;
            }
            if drag_ready {
                self.draw(&mut frame, &drag_nodes, pointer, damage)?;
            }
            frame.finish().map_err(error)?
        };
        self.submitted_watch = if sync.is_reached() { None } else { Some(self.watch(sync.clone())?) };
        self.submitted = Some(sync.clone());
        let mut callbacks = Vec::new();
        for layer in layers
            .iter()
            .filter(|layer| layer.layer == Layer::Overlay || !output_fullscreen(layer.output))
        {
            callbacks.push((Some(layer.output), layer.nodes.clone()));
        }
        let pointer_output = server.output_viewports().into_iter().find(|(_, output)| output.bounds.contains(pointer)).map(|(_, output)| output.id);
        callbacks.push((pointer_output, cursor_nodes.clone()));
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
                    callbacks.push((Some(item.output), server.callback_nodes(window)));
                }
            }
        }
        self.prune_views();
        Ok(Presented {
            cursor: capture::Cursor::new(cursor, cursor_nodes.clone(), pointer),
            drag: if drag_ready { drag_nodes } else { Vec::new() },
            locked: false,
            callbacks,
            sync,
            layers,
            clips: plans,
            popups: visible_popups,
        })
    }

    fn render_lock(
        &mut self,
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
        server: &mut Server,
    ) -> io::Result<Presented> {
        let surfaces = server.lock_surfaces();
        let mut ready = Vec::new();
        for (id, origin, nodes) in surfaces {
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
        for (_, origin, nodes) in &ready {
            self.draw(&mut frame, nodes, origin.map(f64::from), damage)?;
        }
        let sync = frame.finish().map_err(error)?;
        self.submitted_watch = if sync.is_reached() { None } else { Some(self.watch(sync.clone())?) };
        self.submitted = Some(sync.clone());
        self.prune_views();
        Ok(Presented {
            cursor: capture::Cursor::hidden(),
            drag: Vec::new(),
            locked: true,
            callbacks: ready.into_iter().map(|(id, _, nodes)| (Some(id), nodes)).collect(),
            sync,
            layers: Vec::new(),
            clips: Vec::new(),
            popups: Vec::new(),
        })
    }

    fn draw(
        &self,
        frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>,
        nodes: &[SurfaceNode],
        origin: [f64; 2],
        damage: Rect<i32, Physical>,
    ) -> io::Result<()> {
        self.draw_scaled(frame, nodes, origin, [1.0; 2], damage)
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
        self.acquiring.retain(|key, _| retained.contains(key) && key.0.is_alive());
        self.imports.retain(|surface, _| surface.is_alive() && retained.iter().any(|(existing, _)| existing == surface));
        self.copies.retain(|key, copy| !copy.fence.is_reached() || retained.contains(key) && key.0.is_alive());
    }

    fn draw_popup(
        &self,
        frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>,
        popup: &PopupScene,
    ) -> io::Result<()> {
        let scale = std::array::from_fn::<_, 2, _>(|axis| {
            popup.mapping.canvas.size()[axis] / popup.mapping.surface.size()[axis]
        });
        let origin = std::array::from_fn(|axis| {
            popup.mapping.canvas.origin()[axis] - popup.mapping.surface.origin()[axis] * scale[axis]
        });
        for clip in &popup.clips {
            let clip = Rect::new(
                (
                    clip.origin()[0].round() as i32,
                    clip.origin()[1].round() as i32,
                )
                    .into(),
                (clip.size()[0].round() as i32, clip.size()[1].round() as i32).into(),
            );
            self.draw_scaled(frame, &popup.nodes, origin, scale, clip)?;
        }
        Ok(())
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
                (
                    (origin[0] + f64::from(node.origin[0]) * scale[0]).round() as i32,
                    (origin[1] + f64::from(node.origin[1]) * scale[1]).round() as i32,
                )
                    .into(),
                (
                    (node.layout.size[0] * scale[0]).round() as i32,
                    (node.layout.size[1] * scale[1]).round() as i32,
                )
                    .into(),
            );
            let Some(damage) = local_damage(destination, damage) else {
                continue;
            };
            let opaque: Vec<Rect<i32, Physical>> = node.opaque_rectangles().into_iter().filter_map(|rectangle| {
                let low = [(rectangle.loc.x * scale[0]).ceil() as i32, (rectangle.loc.y * scale[1]).ceil() as i32];
                let high = [((rectangle.loc.x + rectangle.size.w) * scale[0]).floor() as i32, ((rectangle.loc.y + rectangle.size.h) * scale[1]).floor() as i32];
                (high[0] > low[0] && high[1] > low[1]).then(|| Rect::new((low[0], low[1]).into(), (high[0] - low[0], high[1] - low[1]).into()))
            }).collect();
            smithay::backend::renderer::Frame::render_texture_from_to(
                frame,
                &view.texture,
                source,
                destination,
                &[damage],
                &opaque,
                node.transform,
                1.0,
            )
            .map_err(error)?;
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
