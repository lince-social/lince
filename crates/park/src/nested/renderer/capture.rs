use super::*;
use crate::server::{CaptureJob, CaptureTarget, WindowSnapshot};
use lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason as Failure;
use smithay::{backend::renderer::{ExportMem, Frame as _, TextureMapping, gles::GlesMapping}, wayland::shm::with_buffer_contents_mut};
use std::collections::VecDeque;

pub(crate) fn monotonic() -> Duration {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(time.tv_sec.max(0) as u64, time.tv_nsec.clamp(0, 999_999_999) as u32)
}

#[derive(Clone)]
pub(crate) struct Cursor {
    pointer: [f64; 2],
    hotspot: [f64; 2],
    nodes: Vec<SurfaceNode>,
    bounds: Rectangle,
    named: bool,
    visible: bool,
}

impl Cursor {
    pub(super) fn hidden() -> Self {
        Self { pointer: [0.0; 2], hotspot: [0.0; 2], nodes: Vec::new(), bounds: Rectangle::new([0.0; 2], [1.0; 2]).unwrap(), named: false, visible: false }
    }

    pub(super) fn new(status: CursorImageStatus, nodes: Vec<SurfaceNode>, pointer: [f64; 2]) -> Self {
        if pointer.iter().any(|value| !value.is_finite()) { return Self::hidden(); }
        match status {
            CursorImageStatus::Hidden => Self::hidden(),
            CursorImageStatus::Named(_) => Self { pointer, hotspot: [0.0; 2], nodes: Vec::new(),
                bounds: Rectangle::new([0.0; 2], [24.0; 2]).unwrap(), named: true, visible: true },
            CursorImageStatus::Surface(surface) => {
                let hotspot = with_states(&surface, |states| states.data_map.get::<CursorImageSurfaceData>()
                    .map(|data| data.lock().unwrap_or_else(|error| error.into_inner()).hotspot).unwrap_or_default());
                let low: [f64; 2] = std::array::from_fn(|axis| nodes.iter().map(|node| f64::from(node.origin[axis])).fold(f64::INFINITY, f64::min));
                let high: [f64; 2] = std::array::from_fn(|axis| nodes.iter().map(|node| f64::from(node.origin[axis]) + node.layout.size[axis]).fold(f64::NEG_INFINITY, f64::max));
                let Some(bounds) = Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis])) else { return Self::hidden() };
                Self { pointer, hotspot: [f64::from(hotspot.x), f64::from(hotspot.y)], nodes, bounds, named: false, visible: true }
            }
        }
    }

    fn rectangle(&self) -> Rectangle {
        Rectangle::new(std::array::from_fn(|axis| self.pointer[axis] - self.hotspot[axis] + self.bounds.origin()[axis]), self.bounds.size()).unwrap()
    }
}

#[derive(Clone)]
pub(crate) struct View {
    id: OutputId,
    frame: Arc<Completed>,
    scene: Arc<Presented>,
    time: Duration,
}

impl Desktop {
    fn capture_view(&self, target: CaptureTarget) -> Option<View> {
        let id = match target {
            CaptureTarget::Output(id) => id,
            CaptureTarget::Window(surface) => self.output_frames.iter().find_map(|(id, (frame, _))|
                frame.frame.geometry.as_ref()?.items.iter().any(|item| item.output == *id && item.id == ItemId::Application(surface)).then_some(*id))?,
        };
        let (frame, scene) = self.output_frames.get(&id)?;
        Some(View { id, frame: frame.clone(), scene: scene.clone(), time: self.capture_times.get(&id).copied().unwrap_or_else(monotonic) })
    }
}

#[derive(Clone, Copy, Debug)]
struct Mapping { origin: [f64; 2], scale: [f64; 2] }

impl Mapping {
    fn point(self, point: [f64; 2]) -> [f64; 2] { std::array::from_fn(|axis| (point[axis] - self.origin[axis]) * self.scale[axis]) }
    fn rectangle(self, rectangle: Rectangle) -> Rect<i32, Physical> {
        let origin = self.point(rectangle.origin());
        Rect::new((origin[0].round() as i32, origin[1].round() as i32).into(),
            ((rectangle.size()[0] * self.scale[0]).round().max(1.0) as i32, (rectangle.size()[1] * self.scale[1]).round().max(1.0) as i32).into())
    }
}

enum Source {
    Output { view: View, bounds: Rectangle, mapping: Mapping },
    Window { window: WindowSnapshot, bounds: Rectangle, view: Option<View> },
}

impl Source {
    fn new(desktop: &Desktop, target: CaptureTarget) -> Option<Self> {
        match target {
            CaptureTarget::Output(_) => {
                let view = desktop.capture_view(target)?;
                if view.scene.locked { return None; }
                let output = view.frame.frame.geometry.as_ref()?.outputs.iter().find(|output| output.id == view.id)?;
                let bounds = output.bounds;
                let mapping = Mapping { origin: bounds.origin(), scale: std::array::from_fn(|axis| f64::from(output.physical_size[axis]) / bounds.size()[axis]) };
                Some(Self::Output { view, bounds, mapping })
            }
            CaptureTarget::Window(id) => {
                let window = desktop.server.windows().into_iter().find(|window| window.id == id && window.mapped)?;
                let bounds = window.bounds()?;
                let bounds = Rectangle::new([bounds.loc.x, bounds.loc.y], [bounds.size.w, bounds.size.h])?;
                Some(Self::Window { window, bounds, view: desktop.capture_view(target) })
            }
        }
    }

    fn cursor(&self) -> Cursor {
        match self {
            Self::Output { view, .. } => view.scene.cursor.clone(),
            Self::Window { view, .. } => view.as_ref().map_or_else(Cursor::hidden, |view| view.scene.cursor.clone()),
        }
    }

    fn cursor_mapping(&self) -> Option<(Mapping, Rectangle)> {
        match self {
            Self::Output { mapping, bounds, .. } => Some((*mapping, *bounds)),
            Self::Window { window, bounds, view: Some(view) } => {
                let item = view.frame.frame.geometry.as_ref()?.items.iter().find(|item| item.output == view.id && item.id == ItemId::Application(window.id))?;
                let scale: [f64; 2] = std::array::from_fn(|axis| bounds.size()[axis].ceil() / item.mapping.canvas.size()[axis]);
                let origin = std::array::from_fn(|axis| item.mapping.canvas.origin()[axis]
                    - item.mapping.surface.origin()[axis] * item.mapping.canvas.size()[axis] / item.mapping.surface.size()[axis]);
                Some((Mapping { origin, scale }, item.mapping.canvas))
            }
            Self::Window { view: None, .. } => None,
        }
    }

    fn cursor_info(&self) -> ([i32; 2], Option<[i32; 2]>, [i32; 2]) {
        let cursor = self.cursor();
        let Some((mapping, _)) = self.cursor_mapping().filter(|(_, bounds)| cursor.visible && intersects(*bounds, cursor.rectangle())) else { return ([1, 1], None, [0, 0]) };
        let point = mapping.point(cursor.pointer);
        let size = std::array::from_fn(|axis| (cursor.bounds.size()[axis] * mapping.scale[axis]).ceil().max(1.0) as i32);
        let hotspot = std::array::from_fn(|axis| ((cursor.hotspot[axis] - cursor.bounds.origin()[axis]) * mapping.scale[axis]).round() as i32);
        let position = Some(point.map(|axis| axis.round() as i32));
        (size, position, hotspot)
    }

    fn nodes(&self, job: &CaptureJob) -> Vec<SurfaceNode> {
        let cursor = self.cursor();
        if job.cursor { return cursor.nodes; }
        let mut nodes: Vec<_> = match self {
            Self::Output { view, .. } => view.scene.layers.iter().filter(|layer| layer.output == view.id).flat_map(|layer| layer.nodes.iter().cloned())
                .chain(view.scene.popups.iter().filter(|popup| popup.output == view.id).flat_map(|popup| popup.nodes.iter().cloned()))
                .chain(view.scene.drag.iter().cloned()).collect(),
            Self::Window { window, .. } => window.nodes.iter().cloned().chain(window.popups.iter().flat_map(|popup| popup.nodes.iter().cloned())).collect(),
        };
        if job.paint_cursor { nodes.extend(cursor.nodes); }
        nodes
    }

    fn time(&self) -> Duration { match self { Self::Output { view, .. } => view.time, Self::Window { .. } => monotonic() } }
}

fn intersects(a: Rectangle, b: Rectangle) -> bool {
    (0..2).all(|axis| a.origin()[axis] < b.origin()[axis] + b.size()[axis] && b.origin()[axis] < a.origin()[axis] + a.size()[axis])
}

struct Waiting { job: CaptureJob, source: Option<Source> }

struct Readback {
    job: Option<CaptureJob>,
    mapping: GlesMapping,
    sync: smithay::backend::renderer::sync::SyncPoint,
    _watch: Option<crate::graphics::wait::Watch>,
    _texture: GlesTexture,
    _inputs: Vec<GlesTexture>,
    _source: Source,
    time: Duration,
    hotspot: [i32; 2],
}

impl Drop for Readback {
    fn drop(&mut self) {
        if !self.sync.is_reached() { let _ = self.sync.wait(); }
    }
}

#[derive(Default)]
pub(super) struct Captures {
    waiting: VecDeque<Waiting>,
    readbacks: Vec<Readback>,
    imported: HashSet<(WlSurface, u64)>,
    paused: bool,
}

impl Composition {
    pub(crate) fn capture_timeout(&self) -> Option<Duration> {
        if self.capture.paused { return None; }
        if self.capture.readbacks.iter().any(|readback| readback.sync.is_reached()) { return Some(Duration::ZERO); }
        self.capture.waiting.iter().map(|waiting| waiting.job.deadline)
            .chain(self.capture.readbacks.iter().filter_map(|readback| readback.job.as_ref().map(|job| job.deadline)))
            .map(|deadline| deadline.saturating_duration_since(Instant::now())).min()
    }

    pub(crate) fn capture(&mut self, renderer: &mut GlesRenderer, desktop: &mut Desktop, active: bool) -> io::Result<()> {
        desktop.server.capture_suspend(!active);
        if !active { self.capture.paused = true; return Ok(()); }
        if std::mem::take(&mut self.capture.paused) {
            let deadline = Instant::now() + Duration::from_secs(2);
            for waiting in &mut self.capture.waiting { waiting.job.deadline = deadline; }
            for readback in &mut self.capture.readbacks { if let Some(job) = &mut readback.job { job.deadline = deadline; } }
        }
        for target in desktop.server.capture_cursor_targets() {
            if let Some(source) = Source::new(desktop, target) {
                let (size, position, _) = source.cursor_info();
                desktop.server.capture_cursor(target, size, position);
            }
        }
        let readbacks = std::mem::take(&mut self.capture.readbacks);
        let mut copied = false;
        for mut readback in readbacks {
            if let Some(job) = readback.job.as_ref() && let Some(reason) = desktop.server.capture_job_failure(job) {
                readback.job.take().unwrap().frame.fail(reason);
            }
            if !readback.sync.is_reached() { self.capture.readbacks.push(readback); continue; }
            let Some(job) = readback.job.take() else { continue };
            if copied { readback.job = Some(job); self.capture.readbacks.push(readback); continue; }
            copied = true;
            let result = renderer.map_texture(&readback.mapping).map_err(error).and_then(|bytes| write_buffer(&job, bytes, readback.mapping.flipped()));
            if result.is_ok() {
                if job.cursor { desktop.server.capture_cursor_ready(job.target, readback.hotspot); }
                job.frame.success(Transform::Normal, None, readback.time);
            } else {
                if let Err(error) = result { eprintln!("Park capture readback: {error}"); }
                job.frame.fail(Failure::Unknown);
            }
        }
        for job in desktop.server.capture_jobs() {
            if self.capture.waiting.len() + self.capture.readbacks.len() >= 8 { job.frame.fail(Failure::Unknown); }
            else { self.capture.waiting.push_back(Waiting { job, source: None }); }
        }
        let count = self.capture.waiting.len();
        for _ in 0..count {
            let mut waiting = self.capture.waiting.pop_front().unwrap();
            if let Some(reason) = desktop.server.capture_job_failure(&waiting.job) {
                if matches!(reason, Failure::Unknown) { eprintln!("Park capture timed out; scene available: {}", waiting.source.is_some()); }
                waiting.job.frame.fail(reason);
                continue;
            }
            if self.capture.readbacks.len() >= 4 { self.capture.waiting.push_back(waiting); continue; }
            if waiting.source.is_none() { waiting.source = Source::new(desktop, waiting.job.target); }
            let Some(source) = waiting.source.as_ref() else { self.capture.waiting.push_back(waiting); continue };
            if waiting.job.cursor && source.cursor_info().0 != waiting.job.size {
                waiting.job.frame.fail(Failure::BufferConstraints);
                continue;
            }
            if let Source::Output { view, .. } = source {
                let size = view.frame.frame.geometry.as_ref().and_then(|geometry| geometry.outputs.iter().find(|output| output.id == view.id))
                    .map(|output| output.physical_size.map(|size| size as i32));
                if !waiting.job.cursor && size != Some(waiting.job.size) {
                    waiting.source = None;
                    self.capture.waiting.push_back(waiting);
                    continue;
                }
            }
            let nodes = source.nodes(&waiting.job);
            self.capture.imported.extend(nodes.iter().map(|node| (node.surface.clone(), node.content.version)));
            let mut ready = true;
            for node in &nodes {
                match self.import(renderer, node) { Ok(imported) => ready &= imported, Err(_) => { ready = false; break; } }
            }
            if !ready { self.capture.waiting.push_back(waiting); continue; }
            let mut inputs: Vec<_> = nodes.iter().filter_map(|node| self.views.get(&(node.surface.clone(), node.content.version)).map(|view| view.texture.clone())).collect();
            let result = self.capture_readback(renderer, &waiting.job, source, &mut inputs);
            match result {
                Ok((mapping, sync, texture)) => {
                    let source = waiting.source.take().unwrap();
                    let time = source.time();
                    let hotspot = source.cursor_info().2;
                    let mut readback = Readback { job: Some(waiting.job), mapping, sync, _watch: None, _texture: texture,
                        _inputs: inputs, _source: source, time, hotspot };
                    readback._watch = Some(self.watch(readback.sync.clone())?);
                    self.capture.readbacks.push(readback);
                }
                Err(error) => {
                    eprintln!("Park capture preparation: {error}");
                    waiting.job.frame.fail(Failure::Unknown);
                }
            }
        }
        let mut retained: HashSet<_> = self.capture.waiting.iter().filter_map(|waiting| waiting.source.as_ref().map(|source| source.nodes(&waiting.job)))
            .flatten().map(|node| (node.surface, node.content.version)).collect();
        retained.extend(self.layers.values().flat_map(|layer| &layer.nodes).chain(self.popups.values().flat_map(|popup| &popup.nodes))
            .map(|node| (node.surface.clone(), node.content.version)));
        let obsolete: Vec<_> = self.capture.imported.iter().filter(|key| !retained.contains(*key)).cloned().collect();
        for key in obsolete {
            self.views.remove(&key);
            self.acquiring.remove(&key);
            if self.copies.get(&key).is_none_or(|copy| copy.fence.is_reached()) {
                self.copies.remove(&key);
                self.capture.imported.remove(&key);
            }
            if !self.views.keys().any(|(surface, _)| surface == &key.0) && !self.copies.keys().any(|(surface, _)| surface == &key.0) {
                self.imports.remove(&key.0);
            }
        }
        Ok(())
    }

    fn capture_readback(&self, renderer: &mut GlesRenderer, job: &CaptureJob, source: &Source, inputs: &mut Vec<GlesTexture>)
        -> io::Result<(GlesMapping, smithay::backend::renderer::sync::SyncPoint, GlesTexture)> {
        if !renderer.capabilities().contains(&smithay::backend::renderer::gles::Capability::ExportFence) {
            return Err(io::Error::other("Asynchronous capture requires GPU completion fences"));
        }
        let size: Size<i32, Physical> = (job.size[0], job.size[1]).into();
        let damage = Rect::from_size(size);
        let mut texture = renderer.create_buffer(Fourcc::Argb8888, (job.size[0], job.size[1]).into()).map_err(error)?;
        let native = if !job.cursor && let Source::Output { view, .. } = source {
            let dmabuf = if view.frame.frame.output.unwrap_or(OutputId(1)) == view.id { view.frame.frame.dmabuf() }
                else { view.frame.frame.outputs.iter().find(|output| output.id == view.id).ok_or_else(|| io::Error::other("The presented output image is unavailable"))?.dmabuf() };
            let texture = renderer.import_dmabuf(dmabuf, None).map_err(error)?;
            inputs.push(texture.clone()); Some(texture)
        } else { None };
        let mapping;
        let sync;
        {
            let mut target = renderer.bind(&mut texture).map_err(error)?;
            {
                let mut frame = renderer.render(&mut target, size, Transform::Flipped180).map_err(error)?;
                frame.clear(smithay::backend::renderer::Color32F::TRANSPARENT, &[damage]).map_err(error)?;
                if job.cursor {
                    let cursor = source.cursor();
                    if let Some((base, _)) = source.cursor_mapping().filter(|(_, bounds)| cursor.visible && intersects(*bounds, cursor.rectangle())) {
                        let origin = cursor.rectangle().origin();
                        self.capture_cursor_draw(&mut frame, &cursor, Mapping { origin, scale: base.scale }, damage)?;
                    }
                } else {
                    match source {
                        Source::Output { view, mapping, .. } => self.capture_output_draw(&mut frame, view, *mapping, native.as_ref().unwrap(), damage)?,
                        Source::Window { window, bounds, .. } => {
                            let scale = std::array::from_fn(|axis| f64::from(job.size[axis]) / bounds.size()[axis]);
                            let origin = std::array::from_fn(|axis| -bounds.origin()[axis] * scale[axis]);
                            self.draw_scaled(&mut frame, &window.nodes, origin, scale, damage)?;
                            let parent = window.geometry.map_or(bounds.origin(), |geometry| [f64::from(geometry.loc.x), f64::from(geometry.loc.y)]);
                            for popup in window.popups.iter().rev() {
                                let origin = std::array::from_fn(|axis| (parent[axis] + f64::from(popup.origin[axis]) - bounds.origin()[axis]) * scale[axis]);
                                self.draw_scaled(&mut frame, &popup.nodes, origin, scale, damage)?;
                            }
                        }
                    }
                    if job.paint_cursor && let Some((mapping, bounds)) = source.cursor_mapping() {
                        let cursor = source.cursor();
                        if cursor.visible && intersects(bounds, cursor.rectangle()) { self.capture_cursor_draw(&mut frame, &cursor, mapping, damage)?; }
                    }
                    if let Source::Output { view, mapping, .. } = source {
                        self.draw_scaled(&mut frame, &view.scene.drag, mapping.point(view.scene.cursor.pointer), mapping.scale, damage)?;
                    }
                }
                let _ = frame.finish().map_err(error)?;
            }
            mapping = renderer.copy_framebuffer(&target, Rect::from_size((job.size[0], job.size[1]).into()), Fourcc::Argb8888).map_err(error)?;
            sync = renderer.render(&mut target, size, Transform::Flipped180).map_err(error)?.finish().map_err(error)?;
        }
        Ok((mapping, sync, texture))
    }

    fn capture_output_draw(&self, frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>, view: &View, mapping: Mapping,
        native: &GlesTexture, damage: Rect<i32, Physical>) -> io::Result<()> {
        let fullscreen = if view.frame.frame.output.unwrap_or(OutputId(1)) == view.id { view.frame.frame.fullscreen }
            else { view.frame.frame.outputs.iter().find(|output| output.id == view.id).is_some_and(|output| output.fullscreen) };
        frame.clear(smithay::backend::renderer::Color32F::new(0.08, 0.08, 0.08, 1.0), &[damage]).map_err(error)?;
        for layer in view.scene.layers.iter().filter(|layer| layer.output == view.id && !fullscreen && matches!(layer.layer, Layer::Background | Layer::Bottom)) {
            self.draw_scaled(frame, &layer.nodes, mapping.point(layer.origin.map(f64::from)), mapping.scale, damage)?;
            for popup in view.scene.popups.iter().filter(|popup| popup.output == view.id && popup.owner == PopupOwner::Layer(layer.surface.clone())) { self.capture_popup_draw(frame, popup, mapping, damage)?; }
        }
        if let Some((_, clip)) = view.scene.clips.iter().find(|(id, _)| *id == view.id) {
            let clip = mapping.rectangle(*clip);
            frame.render_texture_from_to(native, Rect::from_size(native.size().to_f64()), clip, &[Rect::from_size(clip.size)], &[], Transform::Normal, 1.0, None, &[]).map_err(error)?;
        }
        for popup in view.scene.popups.iter().filter(|popup| popup.output == view.id && matches!(popup.owner, PopupOwner::Application(_) | PopupOwner::Native)) { self.capture_popup_draw(frame, popup, mapping, damage)?; }
        for layer in view.scene.layers.iter().filter(|layer| layer.output == view.id && (layer.layer == Layer::Overlay || !fullscreen && layer.layer == Layer::Top)) {
            self.draw_scaled(frame, &layer.nodes, mapping.point(layer.origin.map(f64::from)), mapping.scale, damage)?;
            for popup in view.scene.popups.iter().filter(|popup| popup.output == view.id && popup.owner == PopupOwner::Layer(layer.surface.clone())) { self.capture_popup_draw(frame, popup, mapping, damage)?; }
        }
        Ok(())
    }

    fn capture_popup_draw(&self, frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>, popup: &PopupScene, mapping: Mapping, damage: Rect<i32, Physical>) -> io::Result<()> {
        let scale: [f64; 2] = std::array::from_fn(|axis| popup.mapping.canvas.size()[axis] / popup.mapping.surface.size()[axis]);
        let origin = mapping.point(std::array::from_fn(|axis| popup.mapping.canvas.origin()[axis] - popup.mapping.surface.origin()[axis] * scale[axis]));
        for clip in &popup.clips {
            if let Some(clip) = mapping.rectangle(*clip).intersection(damage) {
                self.draw_scaled(frame, &popup.nodes, origin, std::array::from_fn(|axis| scale[axis] * mapping.scale[axis]), clip)?;
            }
        }
        Ok(())
    }

    fn capture_cursor_draw(&self, frame: &mut smithay::backend::renderer::gles::GlesFrame<'_, '_>, cursor: &Cursor, mapping: Mapping,
        damage: Rect<i32, Physical>) -> io::Result<()> {
        if !cursor.visible { return Ok(()); }
        if cursor.named {
            for size in [[4.0, 20.0], [14.0, 4.0]] {
                let rectangle = Rectangle::new(cursor.pointer, size).unwrap();
                let destination = mapping.rectangle(rectangle);
                if let Some(damage) = local_damage(destination, damage) {
                    frame.draw_solid(destination, &[damage], smithay::backend::renderer::Color32F::new(1.0, 1.0, 1.0, 1.0)).map_err(error)?;
                }
            }
            Ok(())
        } else {
            let origin = mapping.point(std::array::from_fn(|axis| cursor.pointer[axis] - cursor.hotspot[axis]));
            self.draw_scaled(frame, &cursor.nodes, origin, mapping.scale, damage)
        }
    }
}

fn write_buffer(job: &CaptureJob, pixels: &[u8], flipped: bool) -> io::Result<()> {
    let copied = with_buffer_contents_mut(&job.frame.buffer(), |pointer, length, data| {
        if [data.width, data.height] != job.size || data.format != smithay::reexports::wayland_server::protocol::wl_shm::Format::Argb8888
            || job.size.iter().any(|size| *size <= 0) || data.offset < 0 || data.stride < 0 || length > isize::MAX as usize { return false; }
        let Some((row, height)) = pixel_layout(pixels.len(), job.size, length, data.offset as usize, data.stride as usize) else { return false };
        if pointer.is_null() { return false; }
        for y in 0..height {
            let from = if flipped { height - y - 1 } else { y } * row;
            let to = data.offset as usize + y * data.stride as usize;
            unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr().add(from), pointer.add(to), row); }
        }
        true
    }).map_err(error)?;
    if copied { Ok(()) } else { Err(io::Error::other("The capture buffer no longer satisfies its constraints")) }
}

fn pixel_layout(source: usize, size: [i32; 2], destination: usize, offset: usize, stride: usize) -> Option<(usize, usize)> {
    if size.iter().any(|size| *size <= 0) { return None; }
    let row = (size[0] as usize).checked_mul(4)?;
    let height = size[1] as usize;
    let bytes = row.checked_mul(height)?;
    let end = stride.checked_mul(height - 1)?.checked_add(row)?.checked_add(offset)?;
    (stride >= row && source >= bytes && end <= destination).then_some((row, height))
}

#[cfg(all(test, feature = "server-tests"))]
pub(crate) fn probe_window(job: CaptureJob, window: WindowSnapshot) -> io::Result<Vec<u8>> {
    use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
    let device = EGLDevice::enumerate().map_err(error)?.find(|device| !device.is_software())
        .ok_or_else(|| io::Error::other("The capture export probe requires a hardware EGL device"))?;
    let display = unsafe { EGLDisplay::new(device) }.map_err(error)?;
    let context = EGLContext::new(&display).map_err(error)?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(error)?;
    let mut composition = Composition::new(lince_interface::host::WakeSignal::new(|| {}))?;
    let bounds = window.bounds().ok_or_else(|| io::Error::other("The capture probe window has no content"))?;
    let bounds = Rectangle::new([bounds.loc.x, bounds.loc.y], [bounds.size.w, bounds.size.h])
        .ok_or_else(|| io::Error::other("The capture probe window has invalid bounds"))?;
    let source = Source::Window { window, bounds, view: None };
    for node in source.nodes(&job) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !composition.import(&mut renderer, &node)? {
            if Instant::now() >= deadline { return Err(io::Error::other("The capture probe client buffer did not become ready")); }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let mut inputs = Vec::new();
    let (mapping, fence, _texture) = composition.capture_readback(&mut renderer, &job, &source, &mut inputs)?;
    fence.wait().map_err(error)?;
    let bytes = renderer.map_texture(&mapping).map_err(error)?;
    write_buffer(&job, bytes, mapping.flipped())?;
    let pixels = smithay::wayland::shm::with_buffer_contents(&job.frame.buffer(), |pointer, length, data| {
        let count = job.size[0] as usize * job.size[1] as usize * 4;
        if data.offset < 0 || data.offset as usize + count > length { return None; }
        let mut pixels = vec![0; count];
        unsafe { std::ptr::copy_nonoverlapping(pointer.add(data.offset as usize), pixels.as_mut_ptr(), count); }
        Some(pixels)
    }).map_err(error)?.ok_or_else(|| io::Error::other("The capture probe destination is unavailable"))?;
    job.frame.success(Transform::Normal, None, monotonic());
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_buffers_validate_strides_offsets_lengths_and_overflow() {
        assert_eq!(pixel_layout(32, [4, 2], 64, 8, 32), Some((16, 2)));
        assert_eq!(pixel_layout(32, [4, 2], 55, 8, 32), None);
        assert_eq!(pixel_layout(31, [4, 2], 64, 8, 32), None);
        assert_eq!(pixel_layout(32, [4, 2], 64, 0, 15), None);
        assert_eq!(pixel_layout(32, [4, 2], usize::MAX, usize::MAX, 32), None);
        assert_eq!(pixel_layout(32, [4, 2], usize::MAX, 0, usize::MAX), None);
        assert_eq!(pixel_layout(0, [0, 2], 0, 0, 0), None);
        assert_eq!(pixel_layout(0, [2, -1], 0, 0, 0), None);
    }

    #[test]
    fn capture_coordinates_preserve_output_origin_and_fractional_scale() {
        let mapping = Mapping { origin: [960.0, 180.0], scale: [1.5, 1.5] };
        assert_eq!(mapping.point([972.0, 200.0]), [18.0, 30.0]);
        let rectangle = mapping.rectangle(Rectangle::new([972.0, 200.0], [16.0, 24.0]).unwrap());
        assert_eq!(rectangle.loc, (18, 30).into());
        assert_eq!(rectangle.size, (24, 36).into());
    }

    #[test]
    fn cursor_can_intersect_an_output_while_its_hotspot_is_outside() {
        let bounds = Rectangle::new([100.0, 0.0], [100.0, 100.0]).unwrap();
        let cursor = Cursor { pointer: [95.0, 50.0], hotspot: [0.0; 2], nodes: Vec::new(),
            bounds: Rectangle::new([0.0; 2], [24.0; 2]).unwrap(), named: true, visible: true };
        assert!(!bounds.contains(cursor.pointer));
        assert!(intersects(bounds, cursor.rectangle()));
        assert!(!intersects(bounds, Rectangle::new([76.0, 50.0], [24.0; 2]).unwrap()));
    }
}
