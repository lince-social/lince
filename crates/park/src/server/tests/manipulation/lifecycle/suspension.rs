use super::*;
use crate::desktop::renderer::{Composition, Presented};
use smithay::backend::{
    allocator::Fourcc,
    egl::{EGLContext, EGLDevice, EGLDisplay},
    renderer::{
        Bind, ExportMem, Offscreen, TextureMapping,
        gles::{GlesRenderer, GlesTexture},
    },
};

fn renderer() -> GlesRenderer {
    let device = EGLDevice::enumerate()
        .unwrap()
        .find(|device| !device.is_software())
        .unwrap();
    let display = unsafe { EGLDisplay::new(device) }.unwrap();
    let context = EGLContext::new(&display).unwrap();
    unsafe { GlesRenderer::new(context) }.unwrap()
}

fn present(
    fixture: &mut Fixture,
    composition: &mut Composition,
    renderer: &mut GlesRenderer,
) -> Vec<u8> {
    let current = fixture.desktop.current.clone().unwrap();
    composition.canvas(renderer, &current.frame).unwrap();
    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Argb8888, (640, 480).into())
        .unwrap();
    let presented: Presented = {
        let mut target = renderer.bind(&mut texture).unwrap();
        composition
            .render(
                renderer,
                &mut target,
                (640, 480).into(),
                smithay::utils::Transform::Flipped180,
                &mut fixture.desktop.server,
                Some(&current),
                [0.0; 2],
            )
            .unwrap()
    };
    presented.sync.wait().unwrap();
    fixture.desktop.presented(presented, 40).unwrap();
    let mapping = {
        let target = renderer.bind(&mut texture).unwrap();
        renderer
            .copy_framebuffer(
                &target,
                smithay::utils::Rectangle::from_size((640, 480).into()),
                Fourcc::Abgr8888,
            )
            .unwrap()
    };
    let bytes = renderer.map_texture(&mapping).unwrap();
    let row = 640 * 4;
    let mut pixels = Vec::with_capacity(bytes.len());
    for y in 0..480 {
        let y = if mapping.flipped() { 479 - y } else { y };
        pixels.extend_from_slice(&bytes[y * row..(y + 1) * row]);
    }
    pixels
}

fn accept_headless(
    fixture: &mut Fixture,
    composition: &mut Composition,
    renderer: &mut GlesRenderer,
) {
    let serial = barrier(&mut fixture.desktop);
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        assert!(fixture.desktop.worker.failure().is_none());
        fixture.desktop.dispatch().unwrap();
        fixture.desktop.accept(composition, renderer).unwrap();
        if fixture
            .desktop
            .current
            .as_ref()
            .is_some_and(|frame| frame.input_serial >= serial && frame.frame.dmabuf().is_none())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The compositor did not adopt a headless worker frame"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_outputs(fixture: &mut Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        fixture.desktop.dispatch().unwrap();
        fixture.flush();
        if fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .outputs
            .len()
            == count
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The worker did not adopt the current output catalogue"
        );
    }
}

fn capture(name: &str, pixels: &[u8]) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    let mut bytes = b"P6\n640 480\n255\n".to_vec();
    for pixel in pixels.chunks_exact(4) {
        bytes.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(
        std::path::PathBuf::from(directory).join(format!("{name}.ppm")),
        bytes,
    )
    .unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn output_suspension_cancels_live_resizes_preserves_clients_and_keeps_reconnected_outputs_locked() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut renderer = renderer();
    for (client, locked, queued) in [
        (false, false, false),
        (true, false, false),
        (true, false, true),
        (true, true, false),
    ] {
        eprintln!("client={client}, locked={locked}, queued={queued}");
        let mut fixture = Fixture::connected(
            &runtime,
            &directory.path().join(format!("{client}-{locked}-{queued}")),
        );
        let mut composition =
            Composition::new(lince_interface::host::WakeSignal::new(|| {})).unwrap();
        let mut monitors: Vec<_> = fixture
            .desktop
            .server
            .output_viewports()
            .into_iter()
            .map(|(name, viewport)| crate::server::Monitor {
                id: viewport.id,
                name,
                origin: viewport.bounds.origin().map(|value| value as i32),
                size: viewport.physical_size.map(|value| value as i32),
            })
            .collect();
        let mut second = monitors[0].clone();
        second.id = lince_interface::host::OutputId(2);
        second.name = "Recovery-2".into();
        second.origin = [640, 0];
        monitors.push(second);
        fixture.desktop.server.configure_outputs(&monitors).unwrap();
        fixture.desktop.server.registered_output_catalog().unwrap();
        wait_outputs(&mut fixture, 2);
        fixture.configure();
        let original = mapping(&fixture.desktop, fixture.id);
        let surface = fixture.desktop.server.windows()[0].surface.clone();
        let button = fixture.begin(client, queued);
        if !locked && !queued {
            let mut surviving = monitors[1].clone();
            surviving.origin = [0, 0];
            fixture
                .desktop
                .server
                .configure_outputs(&[surviving.clone()])
                .unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
            wait_outputs(&mut fixture, 1);
            fixture.cancelled(original);
            assert_eq!(
                fixture.desktop.current.as_ref().unwrap().frame.output,
                Some(surviving.id)
            );
        }
        let recovery = locked.then(|| Takeover::Lock.apply(&mut fixture));
        let lock_owner = fixture.desktop.server.state.lock.owner.clone();
        let keys = fixture.harness.client.keys.clone();
        fixture.desktop.server.disconnect_outputs();
        fixture.desktop.server.output_catalog(Vec::new()).unwrap();
        fixture.desktop.invalidate_outputs().unwrap();
        accept_headless(&mut fixture, &mut composition, &mut renderer);
        assert!(fixture.desktop.server.output_viewports().is_empty());
        assert!(fixture.desktop.pending.is_none());
        assert!(
            fixture
                .desktop
                .server
                .state
                .presented_geometry
                .as_ref()
                .is_none_or(|geometry| geometry.outputs.is_empty())
        );
        let frame = &fixture.desktop.current.as_ref().unwrap().frame;
        assert!(frame.geometry.as_ref().unwrap().outputs.is_empty());
        assert!(frame.geometry.as_ref().unwrap().items.is_empty());
        assert!(frame.input.editor.is_none());
        assert!(frame.input.pointer_targets.is_empty());
        assert!(frame.input.manipulation.is_none());
        assert!(
            frame
                .input
                .workspaces
                .entries
                .iter()
                .all(|workspace| !workspace.active)
        );
        let configured = frame
            .windows
            .iter()
            .find(|window| window.surface == fixture.id)
            .unwrap();
        assert_eq!(
            configured.size,
            original.surface.size().map(|value| value as u32)
        );
        assert!(!configured.state.resizing);
        let live = fixture.desktop.server.windows();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, fixture.id);
        assert_eq!(live[0].surface, surface);
        assert!(live[0].mapped);
        assert_eq!(fixture.desktop.server.state.lock.owner, lock_owner);
        let callback = Arc::new(AtomicBool::new(false));
        fixture
            .window
            .surface
            .frame(&fixture.harness.queue.handle(), callback.clone());
        fixture.window.surface.commit();
        fixture.sync();
        fixture.desktop.key(30, true, 15).unwrap();
        fixture.desktop.key(30, false, 16).unwrap();
        accept_headless(&mut fixture, &mut composition, &mut renderer);
        fixture.sync();
        assert_eq!(fixture.harness.client.keys, keys);
        assert!(!callback.load(Ordering::Acquire));
        assert_eq!(fixture.desktop.server.locked(), locked);
        fixture.desktop.server.configure_outputs(&monitors).unwrap();
        fixture.desktop.server.registered_output_catalog().unwrap();
        fixture.desktop.invalidate_outputs().unwrap();
        wait_outputs(&mut fixture, 2);
        fixture.configure();
        assert_eq!(mapping(&fixture.desktop, fixture.id), original);
        assert_eq!(fixture.desktop.server.state.lock.owner, lock_owner);
        if locked {
            let pixels = present(&mut fixture, &mut composition, &mut renderer);
            capture("output-locked-reconnect", &pixels);
            let center = (240 * 640 + 320) * 4;
            assert_eq!(&pixels[center..center + 3], &[0, 0, 0]);
            fixture.sync();
            assert!(!callback.load(Ordering::Acquire));
            recovery.unwrap().finish(&mut fixture);
            fixture.flush();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let pixels = loop {
            let pixels = present(&mut fixture, &mut composition, &mut renderer);
            fixture.sync();
            if callback.load(Ordering::Acquire) {
                break pixels;
            }
            assert!(
                Instant::now() < deadline,
                "The reconnected client was not presented through the compositor"
            );
        };
        let low = original.canvas.origin().map(|value| value.max(0.0));
        let high: [f64; 2] = std::array::from_fn(|axis| {
            (original.canvas.origin()[axis] + original.canvas.size()[axis])
                .min([640.0, 480.0][axis])
        });
        assert!((0..2).all(|axis| high[axis] > low[axis]));
        let point: [f64; 2] = std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5);
        capture(
            &format!("output-recovered-{client}-{locked}-{queued}"),
            &pixels,
        );
        let offset = (point[1] as usize * 640 + point[0] as usize) * 4;
        let pixel = &pixels[offset..offset + 3];
        assert!(
            pixel[0] > 170 && pixel[1] < 80 && pixel[2] > 170,
            "The compositor must present the surviving client pixels at {point:?}: {pixel:?}"
        );
        fixture.release_and_check_normal_input(button, client);
        assert_eq!(mapping(&fixture.desktop, fixture.id), original);
        assert_eq!(fixture.desktop.server.windows()[0].surface, surface);
        fixture.desktop.worker.stop().unwrap();
    }
}
