use super::*;
use crate::{desktop::{Desktop, renderer}, runtime::{CanvasWorker, Configuration, Input}, server::Server};
use smithay::{backend::{allocator::Fourcc, egl::{EGLContext, EGLDevice, EGLDisplay}, renderer::{Bind, ExportMem, Offscreen, TextureMapping, gles::{GlesRenderer, GlesTexture}}}, utils::Transform};

const TEST: &str = "presentation::accessibility::platform::dispatcher::real_desktop_presentation_gates_platform_text_and_reader_callbacks_survive_worker_pressure";
const FIELD: &str = "Display magnification factor (1–4)";

mod outputs;

struct Fixture {
    desktop: Desktop,
    renderer: GlesRenderer,
    composition: renderer::Composition,
    texture: GlesTexture,
}

impl Fixture {
    fn new(runtime: &tokio::runtime::Runtime, directory: &Path) -> Self {
        fs::create_dir_all(directory).unwrap();
        let path = directory.join("park.json"); fs::write(&path, b"{}").unwrap();
        let worker = CanvasWorker::spawn(Configuration { showoff: false, size: [1000, 720], directory: directory.into(), runtime: runtime.handle().clone(), cell: None }).unwrap();
        let mut server = Server::new([1000, 720]).unwrap();
        server.registered_output_catalog().unwrap();
        let mut desktop = Desktop::new(server, worker, [1000, 720], 1, true);
        desktop.commands(path, directory.join("wayland-accessibility-fixture"));
        let device = EGLDevice::enumerate().unwrap().find(|device| !device.is_software()).unwrap();
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let context = EGLContext::new(&display).unwrap();
        let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
        let texture = renderer.create_buffer(Fourcc::Argb8888, (1000, 720).into()).unwrap();
        Self { desktop, renderer, texture, composition: renderer::Composition::new(WakeSignal::new(|| {})).unwrap() }
    }

    fn collect(&mut self) {
        self.desktop.dispatch().unwrap();
        assert!(self.desktop.worker.is_running());
        assert!(self.desktop.worker.failure().is_none());
        self.desktop.accept(&mut self.composition, &mut self.renderer).unwrap();
    }

    fn tick(&mut self) {
        self.collect();
        if self.desktop.pending.as_ref().is_some_and(|pending| pending.frame.ready().unwrap())
            || self.desktop.pending.is_none() && self.desktop.current.is_some() && self.desktop.redraw
        {
            let presented = {
                let mut target = self.renderer.bind(&mut self.texture).unwrap();
                self.composition.render(&mut self.renderer, &mut target, (1000, 720).into(), Transform::Normal,
                    &mut self.desktop.server, self.desktop.pending.as_deref().or(self.desktop.current.as_deref()), self.desktop.pointer).unwrap()
            };
            presented.sync.wait().unwrap();
            self.desktop.presented(presented, 0).unwrap();
            self.desktop.redraw = false;
        }
    }

    fn until<T>(&mut self, mut observed: impl FnMut(&Self) -> Result<Option<T>>) -> Result<T> {
        wait(|| { self.tick(); observed(self) })
    }

    fn press(&mut self, code: u32) { self.desktop.key(code, true, 10).unwrap(); self.desktop.key(code, false, 11).unwrap(); }

    fn edit(&mut self) {
        self.desktop.key(56, true, 1).unwrap(); self.press(18); self.desktop.key(56, false, 2).unwrap();
    }

    fn pause(&self) -> (u64, std::sync::mpsc::SyncSender<()>) {
        let (entered, observed) = std::sync::mpsc::sync_channel(1);
        let (release, resume) = std::sync::mpsc::sync_channel(1);
        let serial = self.desktop.worker.input(Input::Test(Box::new(move |_| {
            entered.send(()).unwrap(); resume.recv_timeout(Duration::from_secs(10)).unwrap();
        }))).unwrap();
        observed.recv_timeout(Duration::from_secs(5)).unwrap();
        (serial, release)
    }

    fn fill(&self) -> u64 {
        self.fill_from(0)
    }

    fn fill_from(&self, mut count: usize) -> u64 {
        let mut last = 0;
        loop {
            match self.desktop.worker.input(Input::Pointer([0.0; 2])) {
                Ok(serial) => { count += 1; last = serial; assert!(count <= 128); }
                Err(std::sync::mpsc::TrySendError::Full(_)) => break,
                Err(_) => panic!("The accessibility worker disconnected under input pressure"),
            }
        }
        assert_eq!(count, 128);
        last
    }

    fn draw_output(&mut self, frame: &std::sync::Arc<crate::runtime::Completed>, id: lince_interface::host::OutputId) -> (std::sync::Arc<renderer::Presented>, Vec<u8>, Duration) {
        let output = frame.frame.geometry.as_ref().unwrap().outputs.iter().find(|output| output.id == id).unwrap();
        let size = output.source_size().map(|value| i32::try_from(value).unwrap());
        let mut texture: GlesTexture = self.renderer.create_buffer(Fourcc::Argb8888, (size[0], size[1]).into()).unwrap();
        let (presented, pixels) = {
            let mut target = self.renderer.bind(&mut texture).unwrap();
            let presented = self.composition.render_output(&mut self.renderer, &mut target, output, &mut self.desktop.server, Some(frame.as_ref()), self.desktop.pointer, Default::default()).unwrap();
            presented.sync.wait().unwrap();
            let mapping = self.renderer.copy_framebuffer(&target, smithay::utils::Rectangle::from_size((size[0], size[1]).into()), Fourcc::Abgr8888).unwrap();
            let source = self.renderer.map_texture(&mapping).unwrap();
            let stride = size[0] as usize * 4;
            let mut pixels = Vec::with_capacity(source.len());
            for row in 0..size[1] as usize {
                let row = if mapping.flipped() { size[1] as usize - row - 1 } else { row };
                pixels.extend_from_slice(&source[row * stride..(row + 1) * stride]);
            }
            (presented, pixels)
        };
        (std::sync::Arc::new(presented), pixels, renderer::capture::monotonic())
    }

    fn present_output(&mut self, id: lince_interface::host::OutputId, frame: std::sync::Arc<crate::runtime::Completed>, presented: std::sync::Arc<renderer::Presented>, time: Duration) {
        self.desktop.output_presented(id, frame, presented, time, 0).unwrap();
    }

    fn read<T: Send + 'static>(&mut self, read: impl FnOnce(&mut Presentation) -> T + Send + 'static) -> T {
        let (sent, received) = std::sync::mpsc::sync_channel(1);
        let serial = self.desktop.worker.input(Input::Test(Box::new(move |canvas| { sent.send(read(canvas)).ok().unwrap(); }))).unwrap();
        self.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ()))).unwrap();
        received.recv_timeout(Duration::from_secs(1)).unwrap()
    }
}

fn child(root: PathBuf) -> Result<()> {
    let provider = Provider::new(&root)?;
    provider.status.set_property("ScreenReaderEnabled", true)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?;
    let _entered = runtime.enter();
    let mut fixture = Fixture::new(&runtime, &root.join("profile"));
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.input.control.context.is_some()).map(|_| ())))?;
    let serial = fixture.desktop.worker.input(Input::Test(Box::new(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|space| space.id).collect::<Vec<_>>();
        for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
    }))).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ())))?;
    fixture.edit();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.input.modal).map(|_| ())))?;
    let park = fixture.until(|_| Ok(named(&provider.bus, "Park")?))?;
    assert!(proxy(&provider.bus, &park, "org.a11y.atspi.Action")?.call::<_, _, bool>("DoAction", &(0_i32,))?);
    let toggle = fixture.until(|_| Ok(named(&provider.bus, "Edit display magnification")?))?;
    assert!(proxy(&provider.bus, &toggle, "org.a11y.atspi.Action")?.call::<_, _, bool>("DoAction", &(0_i32,))?);
    let field = fixture.until(|_| Ok(named_role(&provider.bus, FIELD, Some(atspi_common::Role::Entry))?))?;
    let reader = proxy(&provider.bus, &field, "org.a11y.atspi.Text")?;
    fixture.until(|_| Ok((reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))? == "1").then_some(())))?;
    assert_eq!(reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, "1");
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(0_i32,))?);
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().and_then(|frame| frame.frame.input.editor.as_ref())
        .filter(|editor| editor.selection == [0, 0]).map(|_| ())))?;
    fixture.press(3);
    wait(|| {
        fixture.collect();
        Ok(fixture.desktop.pending.as_ref().filter(|pending| pending.frame.ready().unwrap()
            && pending.frame.input.editor.as_ref().is_some_and(|editor| editor.surrounding == "21")).map(|_| ()))
    })?;
    assert_eq!(reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, "1", "An unpresented native image cannot replace the reader's displayed text");
    fixture.until(|_| Ok((reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))? == "21").then_some(())))?;
    let (_, release) = fixture.pause();
    let serial = fixture.fill();
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(2_i32,))?);
    release.send(()).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial
        && frame.frame.input.editor.as_ref().is_some_and(|editor| editor.selection == [2, 2])).map(|_| ())))?;
    let (_, release) = fixture.pause();
    let serial = fixture.fill();
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(0_i32,))?);
    provider.status.set_property("ScreenReaderEnabled", false)?;
    wait(|| Ok(named_role(&provider.bus, FIELD, Some(atspi_common::Role::Entry))?.is_none().then_some(())))?;
    release.send(()).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ())))?;
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.editor.as_ref().unwrap().selection, [2, 2]);
    provider.status.set_property("ScreenReaderEnabled", true)?;
    let returned = fixture.until(|_| Ok(named_role(&provider.bus, FIELD, Some(atspi_common::Role::Entry))?))?;
    assert_ne!(returned, field);
    assert_eq!(proxy(&provider.bus, &returned, "org.a11y.atspi.Text")?.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, "21");
    fixture.edit();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| !frame.frame.input.modal).map(|_| ())))?;
    fixture.until(|fixture| Ok(fixture.desktop.worker.test_activity().0.then_some(())))?;
    let publications = fixture.desktop.worker.test_activity().1;
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        fixture.tick();
        assert_eq!(fixture.desktop.worker.test_activity().1, publications, "A settled reader/tree must not publish frames without new work");
        thread::sleep(Duration::from_millis(5));
    }
    fixture.desktop.worker.stop()?;
    wait(|| Ok(named(&provider.bus, "Lince Park")?.is_none().then_some(())))?;
    Ok(())
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn real_desktop_presentation_gates_platform_text_and_reader_callbacks_survive_worker_pressure() -> Result<()> { run(TEST, child) }
