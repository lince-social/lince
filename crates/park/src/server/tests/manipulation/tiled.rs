use super::*;
use crate::{
    desktop::{Desktop, renderer::Composition},
    runtime::{CanvasWorker, Configuration, Input},
};
use lince_interface::host::{
    DesktopAction, ItemGeometry, ItemId, OutputViewport, Rectangle, WakeSignal, WorkspaceLayout,
};
use smithay::backend::{
    allocator::Fourcc,
    egl::{EGLContext, EGLDevice, EGLDisplay},
    renderer::{
        Bind, ExportMem, Offscreen, TextureMapping,
        gles::{GlesRenderer, GlesTexture},
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const COLORS: [[u8; 4]; 3] = [[240, 20, 230, 255], [20, 230, 20, 255], [20, 160, 240, 255]];

#[path = "tiled/limits.rs"]
mod limits;
#[path = "tiled/editor.rs"]
mod editor;
#[path = "tiled/native.rs"]
mod native;
#[path = "tiled/outputs.rs"]
mod outputs;
#[path = "tiled/projected.rs"]
mod projected;
#[path = "tiled/rebound.rs"]
mod rebound;
#[path = "tiled/recovery.rs"]
mod recovery;
#[path = "tiled/toolkit.rs"]
mod toolkit;

struct Fixture {
    desktop: Desktop,
    harness: Harness,
    clients: Vec<(Window, SurfaceId)>,
    size: [u32; 2],
    output: OutputId,
    directory: PathBuf,
    composition: Composition,
    renderer: GlesRenderer,
    companions: BTreeMap<OutputId, Vec<u8>>,
    companion: Option<outputs::Companion>,
    cell: lince_interface::native::app::CellHandle,
}

impl Fixture {
    fn new(runtime: &tokio::runtime::Runtime, directory: &Path, size: [u32; 2]) -> Self {
        Self::prepared(runtime, directory, size, None)
    }

    fn prepared(
        runtime: &tokio::runtime::Runtime,
        directory: &Path,
        size: [u32; 2],
        prepare: Option<fn(&mut crate::presentation::Presentation)>,
    ) -> Self {
        let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
        let cell = lince_interface::native::app::CellHandle(cell::CellRuntime {
            store: engine.store.clone(),
            engine,
            commands: Default::default(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            speech: None,
            fiote: None,
        });
        if let Some(prepare) = prepare {
            runtime
                .block_on(lince_interface::native::app::initialize_language(&cell.0))
                .unwrap();
            let _entered = runtime.enter();
            let mut canvas = crate::presentation::Presentation::new_with_cell(
                size,
                directory.join("interface-assets"),
                WakeSignal::new(|| {}),
                Some((cell.clone(), directory.into())),
            )
            .unwrap();
            let (sender, _receiver) = std::sync::mpsc::sync_channel(128);
            canvas.host_requests(sender);
            prepare(&mut canvas);
            canvas.finish().unwrap();
        }
        let mut harness = Harness::new();
        harness
            .server
            .configure_outputs(&[Monitor {
                id: OutputId(1),
                name: "Tiled interaction".into(),
                origin: [0; 2],
                size: size.map(|value| value as i32),
            }])
            .unwrap();
        let mut clients = Vec::new();
        for index in 0..3 {
            let known: Vec<_> = harness
                .server
                .windows()
                .iter()
                .map(|window| window.id)
                .collect();
            let window = harness.window();
            window
                .role
                .set_min_size(if index == 1 { 220 } else { 80 }, 80);
            let buffer = colored_buffer(&harness, [320, 240], COLORS[index]);
            window.surface.attach(Some(&buffer), 0, 0);
            window.surface.commit();
            harness.sync();
            let id = harness
                .server
                .windows()
                .iter()
                .find(|window| !known.contains(&window.id))
                .unwrap()
                .id;
            clients.push((window, id));
        }
        let server = std::mem::replace(&mut harness.server, Server::new([640, 480]).unwrap());
        let worker = CanvasWorker::spawn(Configuration {
            showoff: false,
            size,
            directory: directory.into(),
            runtime: runtime.handle().clone(),
            cell: Some(cell.clone()),
        })
        .unwrap();
        let mut desktop = Desktop::new(server, worker, size, 1, true);
        desktop.dispatch().unwrap();
        advance(&mut desktop, 0);
        let device = EGLDevice::enumerate()
            .unwrap()
            .find(|device| !device.is_software())
            .unwrap();
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let context = EGLContext::new(&display).unwrap();
        let renderer = unsafe { GlesRenderer::new(context) }.unwrap();
        let mut fixture = Self {
            desktop,
            harness,
            clients,
            size,
            output: OutputId(1),
            directory: directory.into(),
            composition: Composition::new(WakeSignal::new(|| {})).unwrap(),
            renderer,
            companions: BTreeMap::new(),
            companion: None,
            cell,
        };
        fixture.action(DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        for index in 0..fixture.clients.len() {
            fixture
                .desktop
                .worker
                .input(Input::Window(
                    fixture.clients[index].1,
                    WindowRequest {
                        maximized: Some(false),
                        ..Default::default()
                    },
                ))
                .unwrap();
        }
        fixture.activate(0);
        fixture.configure();
        fixture
    }

    fn flush(&mut self) {
        let serial = barrier(&mut self.desktop);
        advance(&mut self.desktop, serial);
    }

    fn sync(&mut self) {
        client_sync(&mut self.desktop, &mut self.harness);
    }

    fn action(&mut self, action: DesktopAction) {
        let serial = self.desktop.worker.input(Input::Action(action)).unwrap();
        advance(&mut self.desktop, serial);
    }

    fn activate(&mut self, index: usize) {
        let serial = self
            .desktop
            .worker
            .input(Input::Activate(self.clients[index].1))
            .unwrap();
        advance(&mut self.desktop, serial);
    }

    fn configure(&mut self) {
        for (index, (window, id)) in self.clients.iter().enumerate() {
            requests::configure_client_color(
                &mut self.desktop,
                &mut self.harness,
                window,
                *id,
                COLORS[index],
            );
        }
        if let Some(companion) = &self.companion {
            requests::configure_client_color(
                &mut self.desktop,
                &mut self.harness,
                &companion.window,
                companion.id,
                outputs::COLOR,
            );
        }
    }

    fn geometry(&self, index: usize) -> ItemGeometry {
        self.desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(self.clients[index].1))
            .unwrap()
            .clone()
    }

    fn sizes(&self) -> Vec<[u32; 2]> {
        self.clients
            .iter()
            .map(|(_, id)| {
                self.desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .windows
                    .iter()
                    .find(|window| window.surface == *id)
                    .unwrap()
                    .size
            })
            .collect()
    }

    fn viewport(&self) -> OutputViewport {
        self.desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .outputs
            .iter()
            .find(|output| output.id == self.output)
            .unwrap()
            .clone()
    }

    fn point(&self, index: usize, uv: [f64; 2]) -> [f64; 2] {
        let geometry = self.geometry(index);
        let viewport = self.viewport();
        let bounds = visible(&geometry, viewport.bounds).unwrap();
        let desired: [f64; 2] =
            std::array::from_fn(|axis| bounds.origin()[axis] + bounds.size()[axis] * uv[axis]);
        let center = interior(&geometry, viewport.bounds).unwrap();
        let point = std::array::from_fn(|axis| (center[axis] + desired[axis]) * 0.5);
        let point = if geometry.surface_point(point).is_some() {
            point
        } else {
            center
        };
        assert!(geometry.surface_point(point).is_some());
        assert!(viewport.bounds.contains(point));
        point
    }

    fn surface_point(&self, index: usize, uv: [f64; 2]) -> [f64; 2] {
        let item = self.geometry(index);
        let mapping = item.mapping;
        let point = mapping
            .canvas_point(std::array::from_fn(|axis| {
                mapping.surface.origin()[axis] + mapping.surface.size()[axis] * uv[axis]
            }))
            .unwrap();
        assert!(item.surface_point(point).is_some());
        assert!(
            self.viewport().bounds.contains(point),
            "The requested client side is outside the visible viewport: {point:?}"
        );
        point
    }

    fn resize_delta(&mut self, index: usize, start: [f64; 2], delta: [f64; 2]) {
        let mapping = self.geometry(index).mapping;
        let point = mapping.surface_point_unbounded(start).unwrap();
        let size = self.sizes()[index];
        let next = mapping
            .canvas_point(std::array::from_fn(|axis| {
                point[axis] + delta[axis] * mapping.surface.size()[axis] / f64::from(size[axis])
            }))
            .unwrap();
        assert!(
            self.viewport().bounds.contains(next),
            "The resize proposal leaves the tested output: {next:?}"
        );
        self.desktop.move_pointer(next, 5).unwrap();
    }

    fn relative_origin(&self, reference: usize, index: usize) -> [f64; 2] {
        let reference = self.geometry(reference).mapping;
        self.origin_in(index, reference)
    }

    fn origin_in(
        &self,
        index: usize,
        reference: lince_interface::host::ContentMapping,
    ) -> [f64; 2] {
        let item = self.geometry(index).mapping;
        let point = item.canvas_point(item.surface.origin()).unwrap();
        let point = reference.surface_point_unbounded(point).unwrap();
        std::array::from_fn(|axis| point[axis] - reference.surface.origin()[axis])
    }

    fn begin_client_move(&mut self, index: usize) {
        self.harness.client.pointer_buttons.clear();
        let start = self.point(index, [0.3, 0.3]);
        self.desktop.move_pointer(start, 1).unwrap();
        assert!(
            matches!(self.desktop.target(), crate::desktop::input::Target::Application(id, _, _) if id == self.clients[index].1)
        );
        self.desktop.button(272, true, 2).unwrap();
        self.sync();
        let serial = self.harness.client.pointer_serial;
        self.flush();
        self.desktop
            .move_pointer([start[0] + 12.0, start[1] + 12.0], 3)
            .unwrap();
        self.clients[index]
            .0
            .role
            ._move(self.harness.client.seat.as_ref().unwrap(), serial);
        self.sync();
        self.desktop.dispatch().unwrap();
        self.flush();
        let active = self.desktop.current.as_ref().unwrap().frame.input.manipulation
            .expect("A client move from a partially visible tile was rejected after normal focus/reveal");
        assert_eq!(
            active.target.item,
            ItemId::Application(self.clients[index].1)
        );
        assert_eq!(self.harness.client.pointer_buttons, vec![(272, true)]);
    }

    fn begin_modifier_resize(&mut self, index: usize, uv: [f64; 2]) -> [f64; 2] {
        let sizes = self.sizes();
        let start = self.surface_point(index, uv);
        self.harness.client.pointer_buttons.clear();
        self.desktop.move_pointer(start, 1).unwrap();
        self.desktop.key(125, true, 2).unwrap();
        self.desktop.button(273, true, 3).unwrap();
        self.desktop.key(125, false, 4).unwrap();
        self.flush();
        assert_eq!(
            self.desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .unwrap()
                .target
                .item,
            ItemId::Application(self.clients[index].1)
        );
        self.configure();
        assert_eq!(
            self.sizes(),
            sizes,
            "Beginning a resize changed client sizes without motion"
        );
        start
    }

    fn resizing(&self) -> Vec<bool> {
        self.clients
            .iter()
            .map(|(window, _)| {
                self.harness.client.window_configurations[&window.role.id()]
                    .1
                    .contains(&(xdg_toplevel::State::Resizing as u32))
            })
            .collect()
    }

    fn end_modifier_resize(&mut self, commit: bool) {
        if !commit {
            self.desktop.key(1, true, 8).unwrap();
            self.desktop.key(1, false, 9).unwrap();
        }
        self.desktop.button(273, false, 10).unwrap();
        self.flush();
        self.configure();
        assert!(
            self.desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert_eq!(self.resizing(), vec![false; 3]);
        self.sync();
        assert!(self.harness.client.pointer_buttons.is_empty());
        let spatial = self.geometry(0).mapping.corners.is_some();
        self.render(&format!("tiled-resize-final-{spatial}-{commit}"));
    }

    fn identities(&mut self) -> BTreeMap<String, String> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.flush();
            if let Ok(bytes) = std::fs::read(self.directory.join("park-windows.json")) {
                let profiles: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
                let count = self.clients.len() + usize::from(self.companion.is_some());
                if profiles.len() == count
                    && profiles.iter().all(|row| row["uid"].as_str().is_some())
                {
                    let identities: BTreeMap<_, _> = profiles
                        .into_iter()
                        .map(|row| {
                            let uid = row["uid"].as_str().unwrap().to_owned();
                            assert!(nucleus::valid_uid(&uid, "r"));
                            (row["layout"]["id"].to_string(), uid)
                        })
                        .collect();
                    assert_eq!(identities.len(), count);
                    assert_eq!(
                        identities
                            .values()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len(),
                        count
                    );
                    for (_, id) in &self.clients {
                        let target = self
                            .desktop
                            .current
                            .as_ref()
                            .unwrap()
                            .frame
                            .input
                            .pointer_targets
                            .iter()
                            .find(|target| target.item == ItemId::Application(*id))
                            .unwrap();
                        assert!(
                            identities
                                .contains_key(&serde_json::to_string(&target.placement.0).unwrap())
                        );
                    }
                    if let Some(companion) = &self.companion {
                        assert!(
                            identities.contains_key(
                                &serde_json::to_string(&companion.placement.0).unwrap()
                            )
                        );
                    }
                    return identities;
                }
            }
            assert!(
                Instant::now() < deadline,
                "The real application Records were not durably associated"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn paint(
        &mut self,
        current: &Arc<crate::runtime::Completed>,
        output: Option<&OutputViewport>,
    ) -> Vec<u8> {
        let pixels_size = output.map_or(self.size, |output| output.physical_size);
        let size = (pixels_size[0] as i32, pixels_size[1] as i32);
        let mut texture: GlesTexture = self
            .renderer
            .create_buffer(Fourcc::Argb8888, size.into())
            .unwrap();
        let presented = {
            let mut target = self.renderer.bind(&mut texture).unwrap();
            if let Some(output) = output {
                self.composition
                    .render_output(
                        &mut self.renderer,
                        &mut target,
                        output,
                        &mut self.desktop.server,
                        Some(current),
                        [0.0; 2],
                        Default::default(),
                    )
                    .unwrap()
            } else {
                self.composition
                    .render(
                        &mut self.renderer,
                        &mut target,
                        size.into(),
                        smithay::utils::Transform::Flipped180,
                        &mut self.desktop.server,
                        Some(current),
                        [0.0; 2],
                    )
                    .unwrap()
            }
        };
        presented.sync.wait().unwrap();
        if let Some(output) = output {
            self.desktop
                .output_presented(
                    output.id,
                    current.clone(),
                    Arc::new(presented),
                    crate::desktop::renderer::capture::monotonic(),
                    10,
                )
                .unwrap();
        } else {
            self.desktop.presented(presented, 10).unwrap();
        }
        let mapping = {
            let target = self.renderer.bind(&mut texture).unwrap();
            self.renderer
                .copy_framebuffer(
                    &target,
                    smithay::utils::Rectangle::from_size(size.into()),
                    Fourcc::Abgr8888,
                )
                .unwrap()
        };
        let bytes = self.renderer.map_texture(&mapping).unwrap();
        let row = pixels_size[0] as usize * 4;
        let mut pixels = Vec::with_capacity(bytes.len());
        for y in 0..pixels_size[1] as usize {
            let y = if output.is_none() && mapping.flipped() {
                pixels_size[1] as usize - y - 1
            } else {
                y
            };
            pixels.extend_from_slice(&bytes[y * row..(y + 1) * row]);
        }
        pixels
    }

    fn render(&mut self, name: &str) -> Vec<u8> {
        let current = self.desktop.current.clone().unwrap();
        self.composition
            .canvas(&mut self.renderer, &current.frame)
            .unwrap();
        let viewport = self.viewport();
        let outputs = current.frame.geometry.as_ref().unwrap().outputs.clone();
        let pixels = if outputs.len() > 1
            || viewport.raster.scale != [1.0; 2]
            || viewport.bounds.origin() != [0.0; 2]
        {
            let mut selected = None;
            for output in outputs {
                let pixels = self.paint(&current, Some(&output));
                if output.id == self.output {
                    selected = Some(pixels);
                } else {
                    self.capture_size(
                        &format!("{name}-companion-{}-{}", output.id.0, self.size[0]),
                        output.physical_size,
                        &pixels,
                    );
                    if let Some(companion) = &mut self.companion {
                        companion.verify(&current.frame, &output, &pixels);
                    }
                    if let Some(saved) = self.companions.get(&output.id) {
                        assert!(
                            pixels.as_slice() == saved.as_slice(),
                            "A gesture changed the other output's image"
                        );
                    } else {
                        self.companions.insert(output.id, pixels.clone());
                    }
                }
            }
            selected.unwrap()
        } else {
            self.paint(&current, None)
        };
        self.capture(name, &pixels);
        self.verify_clients(&current, name, &pixels);
        pixels
    }

    fn verify_clients(&self, current: &Arc<crate::runtime::Completed>, name: &str, pixels: &[u8]) {
        let viewport = self.viewport();
        for (index, (_, id)) in self.clients.iter().enumerate() {
            let Some(item) = current
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .items
                .iter()
                .find(|item| item.id == ItemId::Application(*id))
            else {
                continue;
            };
            if item.output != self.output {
                continue;
            }
            let Some(bounds) = visible(item, viewport.bounds) else {
                continue;
            };
            let point = interior(item, viewport.bounds).unwrap_or_else(|| {
                panic!(
                    "A visible client has no usable polygon interior: {item:?}, bounds={bounds:?}"
                )
            });
            if current.frame.input.modal
                || current.frame.input.keyboard_modal
                || current.frame.input.controls.iter().any(|control| control.contains(point))
            {
                continue;
            }
            if current
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .items
                .iter()
                .rev()
                .find(|item| item.output == self.output && item.surface_point(point).is_some())
                .is_none_or(|top| top.id != item.id)
            {
                continue;
            }
            let raster = viewport.raster.point(point);
            let pixel = (raster[1] as usize * self.size[0] as usize + raster[0] as usize) * 4;
            let expected = [COLORS[index][2], COLORS[index][1], COLORS[index][0]];
            if (0..3).any(|axis| pixels[pixel + axis].abs_diff(expected[axis]) >= 20) {
                let native =
                    crate::presentation::probe_pixels(current.frame.dmabuf().unwrap()).unwrap();
                self.capture(&format!("{name}-canvas"), &native);
                eprintln!(
                    "Composed frame sizes={:?}, geometry={:?}, painted={:?}",
                    self.sizes(),
                    current.frame.geometry,
                    current
                        .windows
                        .iter()
                        .map(|window| (
                            window.id,
                            window
                                .nodes
                                .iter()
                                .map(|node| node.layout.size)
                                .collect::<Vec<_>>()
                        ))
                        .collect::<Vec<_>>()
                );
            }
            assert!(
                (0..3).all(|axis| pixels[pixel + axis].abs_diff(expected[axis]) < 20),
                "Composed client {id:?} lost its pixels at {point:?}: {:?}",
                &pixels[pixel..pixel + 4]
            );
        }
    }

    fn capture(&self, name: &str, pixels: &[u8]) {
        let name = if self.output == OutputId(1) {
            name.to_owned()
        } else {
            format!("{name}-output-{}", self.output.0)
        };
        self.capture_size(&name, self.size, pixels);
    }

    fn capture_size(&self, name: &str, size: [u32; 2], pixels: &[u8]) {
        if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
            std::fs::create_dir_all(&directory).unwrap();
            let mut image = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
            for pixel in pixels.chunks_exact(4) {
                image.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(
                PathBuf::from(directory).join(format!("{name}-{}.ppm", size[0])),
                image,
            )
            .unwrap();
        }
    }

    fn normal_context_clicks(&mut self, index: usize) {
        self.harness.client.pointer_buttons.clear();
        for time in [20, 30, 40] {
            self.desktop
                .move_pointer(self.point(index, [0.5, 0.5]), time)
                .unwrap();
            self.desktop.button(273, true, time + 1).unwrap();
            self.desktop.button(273, false, time + 2).unwrap();
            self.sync();
        }
        assert_eq!(
            self.harness.client.pointer_buttons,
            [(273, true), (273, false)].repeat(3)
        );
    }
}

fn visible(item: &ItemGeometry, bounds: Rectangle) -> Option<Rectangle> {
    let mut low = item.mapping.canvas.origin();
    let mut high: [f64; 2] =
        std::array::from_fn(|axis| low[axis] + item.mapping.canvas.size()[axis]);
    if let Some(polygons) = &item.clip {
        low = std::array::from_fn(|axis| {
            polygons
                .iter()
                .flatten()
                .map(|point| point[axis])
                .fold(f64::INFINITY, f64::min)
        });
        high = std::array::from_fn(|axis| {
            polygons
                .iter()
                .flatten()
                .map(|point| point[axis])
                .fold(f64::NEG_INFINITY, f64::max)
        });
    }
    low = std::array::from_fn(|axis| low[axis].max(bounds.origin()[axis]));
    high = std::array::from_fn(|axis| high[axis].min(bounds.origin()[axis] + bounds.size()[axis]));
    Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
}

fn accent(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[2] > 140 && pixel[0] < 190 && pixel[1] > 50 && pixel[1] < 180)
        .count()
}

fn interior(item: &ItemGeometry, bounds: Rectangle) -> Option<[f64; 2]> {
    let valid = |point: [f64; 2]| bounds.contains(point) && item.surface_point(point).is_some();
    item.clip
        .as_ref()
        .and_then(|polygons| {
            polygons
                .iter()
                .filter_map(|polygon| {
                    let mut area = 0.0;
                    let mut center = [0.0; 2];
                    for (a, b) in polygon
                        .iter()
                        .zip(polygon.iter().cycle().skip(1))
                        .take(polygon.len())
                    {
                        let cross = a[0] * b[1] - b[0] * a[1];
                        area += cross;
                        for axis in 0..2 {
                            center[axis] += (a[axis] + b[axis]) * cross;
                        }
                    }
                    if area.abs() <= 1e-6 {
                        return None;
                    }
                    let point = center.map(|value| value / (3.0 * area));
                    valid(point).then_some((area.abs(), point))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, point)| point)
        })
        .or_else(|| {
            visible(item, bounds).and_then(|bounds| {
                let point =
                    std::array::from_fn(|axis| bounds.origin()[axis] + bounds.size()[axis] * 0.5);
                valid(point).then_some(point)
            })
        })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn partially_visible_tiled_client_moves_keep_the_original_press_and_render_one_destination() {
    csd(false, false);
}

fn csd(spatial: bool, multiple: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut fixture = projected::fixture(
            &runtime,
            &directory.path().join(format!("csd-{spatial}-{}", size[0])),
            size,
            spatial,
            multiple,
        );
        let name = |base: &str| {
            if spatial {
                format!("{base}-spatial")
            } else {
                base.to_owned()
            }
        };
        fixture.action(DesktopAction::CycleItemWidth);
        fixture.action(DesktopAction::CycleItemWidth);
        fixture.configure();
        let sizes = fixture.sizes();
        let identities = fixture.identities();
        fixture.render(&name("tiled-csd-partial"));
        let viewport = fixture.viewport();
        let index = (1..fixture.clients.len())
            .find(|index| {
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .geometry
                    .as_ref()
                    .unwrap()
                    .items
                    .iter()
                    .find(|item| item.id == ItemId::Application(fixture.clients[*index].1))
                    .and_then(|item| visible(item, viewport.bounds).map(|visible| (item, visible)))
                    .is_some_and(|(item, visible)| {
                        visible.size()[0] > 16.0
                            && visible.size()[0] < item.mapping.canvas.size()[0] - 8.0
                    })
            })
            .unwrap_or_else(|| {
                panic!(
                    "The fixture needs a partly visible tile: {:?}",
                    fixture.desktop.current.as_ref().unwrap().frame.geometry
                )
            });
        if !spatial {
            assert_eq!(index, 1);
        }
        let item = fixture.geometry(index);
        let anchor = index - 1;
        assert_eq!(item.mapping.corners.is_some(), spatial);
        let visible = visible(&item, viewport.bounds).unwrap();
        assert!(
            visible.size()[0] > 16.0 && visible.size()[0] < item.mapping.canvas.size()[0] - 8.0,
            "The fixture needs a partly visible tile: {item:?}, visible={visible:?}"
        );
        fixture.begin_client_move(index);
        let source = fixture.geometry(index);
        let baseline = fixture.render(&name("tiled-csd-before-destination"));
        let destination = fixture.surface_point(anchor, [0.49, 0.5]);
        fixture.desktop.move_pointer(destination, 4).unwrap();
        fixture.flush();
        fixture.configure();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_some()
        );
        assert_eq!(fixture.sizes(), sizes);
        assert_eq!(fixture.geometry(index), source);
        let preview = fixture.render(&name("tiled-csd-destination"));
        assert!(
            accent(&preview) > accent(&baseline) + 100,
            "The tiled destination outline is not visible through normal composition"
        );
        fixture.desktop.button(272, false, 5).unwrap();
        fixture.flush();
        fixture.configure();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert!(
            fixture.geometry(index).mapping.canvas.origin()[0]
                < fixture.geometry(anchor).mapping.canvas.origin()[0]
        );
        fixture.render(&name("tiled-csd-committed"));
        let committed = fixture.geometry(index);
        fixture.begin_client_move(index);
        fixture
            .desktop
            .move_pointer(fixture.surface_point(anchor, [0.51, 0.5]), 6)
            .unwrap();
        fixture.flush();
        fixture.desktop.key(1, true, 7).unwrap();
        fixture.desktop.key(1, false, 8).unwrap();
        fixture.desktop.button(272, false, 9).unwrap();
        fixture.flush();
        fixture.configure();
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert_eq!(fixture.geometry(index), committed);
        fixture.normal_context_clicks(index);
        assert_eq!(fixture.identities(), identities);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn tiled_width_and_shared_row_edges_configure_only_resized_clients_and_keep_neighbors_stable() {
    resize(false, false);
}

fn resize(spatial: bool, multiple: bool) {
    use lince_interface::host::Direction;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut fixture = projected::fixture(
            &runtime,
            &directory
                .path()
                .join(format!("resize-{spatial}-{}", size[0])),
            size,
            spatial,
            multiple,
        );
        let name = |base: &str| {
            if spatial {
                format!("{base}-spatial")
            } else {
                base.to_owned()
            }
        };
        fixture.activate(2);
        fixture.action(DesktopAction::Move(Direction::Down));
        fixture.activate(0);
        fixture.action(DesktopAction::CycleRowHeight);
        fixture.configure();
        let identities = fixture.identities();
        let initial = fixture.sizes();
        assert_eq!(initial[0][1], initial[1][1]);
        assert!(initial[0][1] < initial[2][1]);
        let next_row = fixture.relative_origin(0, 2)[1];
        assert_eq!(fixture.geometry(0).mapping.corners.is_some(), spatial);
        fixture.render(&name("tiled-resize-initial"));
        let start = fixture.begin_modifier_resize(0, [0.9, 0.5]);
        let fixed = fixture.geometry(2);
        assert_eq!(fixture.resizing(), [true, false, false]);
        fixture.resize_delta(0, start, [24.0, 16.0]);
        fixture.flush();
        fixture.configure();
        fixture.render(&name("tiled-width-preview"));
        assert_eq!(
            fixture.resizing(),
            [true, false, false],
            "Moving a neighbor during width resize must not mark it as resized"
        );
        let mut widened = initial.clone();
        widened[0][0] += 24;
        assert_eq!(fixture.sizes(), widened);
        assert_eq!(fixture.geometry(2), fixed);
        assert!((fixture.relative_origin(0, 2)[1] - next_row).abs() < 0.01);
        fixture.end_modifier_resize(true);
        assert_eq!(fixture.sizes(), widened);
        let start = fixture.begin_modifier_resize(0, [0.5, 0.99]);
        let frozen = fixture.geometry(0).mapping;
        let fixed = fixture.geometry(2);
        let before = fixture.origin_in(2, frozen)[1];
        fixture.resize_delta(0, start, [16.0, 32.0]);
        fixture.flush();
        fixture.configure();
        fixture.render(&name("tiled-row-preview"));
        assert_eq!(fixture.resizing(), [true, true, false]);
        let mut taller = widened.clone();
        taller[0][1] += 32;
        taller[1][1] += 32;
        assert_eq!(fixture.sizes(), taller);
        assert!((fixture.origin_in(2, frozen)[1] - before - 32.0).abs() < 0.01);
        assert!((fixture.relative_origin(0, 2)[1] - next_row - 32.0).abs() < 0.01);
        fixture.end_modifier_resize(false);
        assert_eq!(fixture.sizes(), widened);
        assert_eq!(fixture.geometry(2), fixed);
        assert!((fixture.relative_origin(0, 2)[1] - next_row).abs() < 0.01);
        let start = fixture.begin_modifier_resize(0, [0.5, 0.99]);
        fixture.resize_delta(0, start, [0.0, 24.0]);
        fixture.flush();
        fixture.configure();
        assert!((fixture.origin_in(2, frozen)[1] - before - 24.0).abs() < 0.01);
        fixture.end_modifier_resize(true);
        taller[0][1] -= 8;
        taller[1][1] -= 8;
        assert_eq!(fixture.sizes(), taller);
        assert!((fixture.relative_origin(0, 2)[1] - next_row - 24.0).abs() < 0.01);
        fixture.render(&name("tiled-row-committed"));
        fixture.normal_context_clicks(0);
        assert_eq!(fixture.identities(), identities);
        fixture.desktop.worker.stop().unwrap();
    }
}
