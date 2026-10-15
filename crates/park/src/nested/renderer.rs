use super::*;
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
    version: u64,
    size: [i32; 2],
}

#[derive(Default)]
pub(super) struct Composition {
    canvases: HashMap<OutputId, GlesTexture>,
    views: HashMap<WlSurface, View>,
    layers: HashMap<WlSurface, LayerSnapshot>,
    popups: HashMap<(WlSurface, OutputId), PopupScene>,
}

#[derive(Clone, PartialEq)]
pub(super) enum PopupOwner {
    Application(lince_interface::host::SurfaceId),
    Layer(WlSurface),
}

#[derive(Clone)]
pub(super) struct PopupScene {
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

pub(super) struct Presented {
    pub layers: Vec<LayerSnapshot>,
    pub clips: Vec<(OutputId, Rectangle)>,
    pub popups: Vec<PopupScene>,
}

impl Composition {
    pub(super) fn canvas(
        &mut self,
        backend: &mut WinitGraphicsBackend<GlesRenderer>,
        frame: &crate::presentation::Frame,
    ) -> io::Result<()> {
        let mut canvases = HashMap::new();
        canvases.insert(
            frame.output.unwrap_or(OutputId(1)),
            backend
                .renderer()
                .import_dmabuf(frame.dmabuf(), None)
                .map_err(error)?,
        );
        for output in &frame.outputs {
            canvases.insert(
                output.id,
                backend
                    .renderer()
                    .import_dmabuf(output.dmabuf(), None)
                    .map_err(error)?,
            );
        }
        self.canvases = canvases;
        Ok(())
    }

    fn import(&mut self, renderer: &mut GlesRenderer, node: &SurfaceNode) -> io::Result<bool> {
        if self
            .views
            .get(&node.surface)
            .is_some_and(|view| view.version == node.content.version)
        {
            return Ok(true);
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
                    return Ok(false);
                }
            }
        }
        let size: Size<i32, Buffer> = (node.content.size[0], node.content.size[1]).into();
        let source = renderer
            .import_buffer(node.content.buffer(), None, &[Rect::from_size(size)])
            .ok_or_else(|| io::Error::other("unsupported layer buffer"))?
            .map_err(error)?;
        let mut texture: GlesTexture = match self
            .views
            .get(&node.surface)
            .filter(|view| view.size == node.content.size)
        {
            Some(view) => view.texture.clone(),
            None => renderer
                .create_buffer(Fourcc::Argb8888, size)
                .map_err(error)?,
        };
        {
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
            frame.finish().map_err(error)?.wait().map_err(error)?;
        }
        node.content.release();
        self.views.insert(
            node.surface.clone(),
            View {
                texture,
                version: node.content.version,
                size: node.content.size,
            },
        );
        Ok(true)
    }

    pub(super) fn present(
        &mut self,
        backend: &mut WinitGraphicsBackend<GlesRenderer>,
        server: &mut Server,
        canvas: Option<&Completed>,
        pointer: [f64; 2],
    ) -> io::Result<Presented> {
        self.views.retain(|surface, _| surface.is_alive());
        let layers = server.layers();
        self.layers
            .retain(|surface, _| layers.iter().any(|layer| &layer.surface == surface));
        for layer in &layers {
            let mut ready = true;
            for node in &layer.nodes {
                match self.import(backend.renderer(), node) {
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
        let cursor_nodes = if let CursorImageStatus::Surface(surface) = &cursor {
            let nodes = server.surface_nodes(surface);
            let mut ready = true;
            for node in &nodes {
                match self.import(backend.renderer(), node) {
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
        let size = backend.window_size();
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
                            uncovered(output.bounds, &canvas.frame.input.controls)
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
        let mut visible_popups = Vec::new();
        let mut popup_surfaces = HashSet::new();
        for (popup, scene) in popup_plans {
            let key = (popup.surface.clone(), scene.output);
            popup_surfaces.insert(key.clone());
            let mut ready = true;
            for node in &popup.nodes {
                match self.import(backend.renderer(), node) {
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
        {
            let (renderer, mut target) = backend.bind().map_err(error)?;
            let mut frame = renderer
                .render(&mut target, size, Transform::Flipped180)
                .map_err(error)?;
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
                    .render_texture_at(
                        texture,
                        (
                            rectangle.origin()[0].round() as i32,
                            rectangle.origin()[1].round() as i32,
                        )
                            .into(),
                        1,
                        1.0,
                        Transform::Normal,
                        &[clip],
                        &[],
                        1.0,
                    )
                    .map_err(error)?;
            }
            for popup in visible_popups
                .iter()
                .filter(|popup| matches!(popup.owner, PopupOwner::Application(_)))
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
            frame.finish().map_err(error)?.wait().map_err(error)?;
        }
        backend.submit(Some(&[damage])).map_err(error)?;
        match cursor {
            CursorImageStatus::Named(icon) => {
                backend.window().set_cursor_visible(true);
                backend.window().set_cursor(icon);
            }
            CursorImageStatus::Hidden | CursorImageStatus::Surface(_) => {
                backend.window().set_cursor_visible(false)
            }
        }
        for layer in layers
            .iter()
            .filter(|layer| layer.layer == Layer::Overlay || !output_fullscreen(layer.output))
        {
            server.presented_nodes(&layer.nodes);
        }
        server.presented_nodes(&cursor_nodes);
        for popup in &visible_popups {
            if !popup.clips.is_empty() {
                server.presented_nodes(&popup.nodes);
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
                    server.presented_nodes(&window.nodes);
                }
            }
        }
        Ok(Presented {
            layers,
            clips: plans,
            popups: visible_popups,
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
            let Some(view) = self.views.get(&node.surface) else {
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
            smithay::backend::renderer::Frame::render_texture_from_to(
                frame,
                &view.texture,
                source,
                destination,
                &[damage],
                &[],
                node.transform,
                1.0,
            )
            .map_err(error)?;
        }
        Ok(())
    }
}

fn uncovered(bounds: Rectangle, controls: &[Rectangle]) -> Vec<Rectangle> {
    let mut pieces = vec![bounds];
    for control in controls {
        let mut remaining = Vec::new();
        for piece in pieces {
            let low: [f64; 2] =
                std::array::from_fn(|axis| piece.origin()[axis].max(control.origin()[axis]));
            let high: [f64; 2] = std::array::from_fn(|axis| {
                (piece.origin()[axis] + piece.size()[axis])
                    .min(control.origin()[axis] + control.size()[axis])
            });
            if (0..2).any(|axis| low[axis] >= high[axis]) {
                remaining.push(piece);
                continue;
            }
            let end: [f64; 2] =
                std::array::from_fn(|axis| piece.origin()[axis] + piece.size()[axis]);
            for (origin, size) in [
                (
                    piece.origin(),
                    [piece.size()[0], low[1] - piece.origin()[1]],
                ),
                (
                    [piece.origin()[0], high[1]],
                    [piece.size()[0], end[1] - high[1]],
                ),
                (
                    [piece.origin()[0], low[1]],
                    [low[0] - piece.origin()[0], high[1] - low[1]],
                ),
                ([high[0], low[1]], [end[0] - high[0], high[1] - low[1]]),
            ] {
                if let Some(rectangle) = Rectangle::new(origin, size) {
                    remaining.push(rectangle);
                }
            }
            if remaining.len() > 256 {
                return Vec::new();
            }
        }
        pieces = remaining;
    }
    pieces
}
