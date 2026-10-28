use super::*;
use super::super::{capture_lifetime, configuration, magnification::clients, opening, privacy};
use lince_interface::host::{color::OutputColor, display::ColorStatus};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options;

fn generation(fixture: &mut Fixture) -> u64 {
    let previous = fixture.desktop.server.display_snapshot().color[&OutputId(2)].revision;
    let revision = previous + 1;
    fixture.desktop.server.output_color(OutputId(2), ColorStatus { prepared: Some(OutputColor::default()),
        effective: Some(OutputColor::default()), revision, ..Default::default() });
    revision
}


#[test]
#[ignore = "requires hardware Vulkan and EGL; color-generation retirement does not claim physical HDR confirmation"]
fn twenty_color_generations_retire_output_and_cursor_readbacks_preserve_raw_window_sources_and_exclude_private_pixels_with_fresh_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = rotation::create(&runtime, directory.path());
    rotation::output(&mut fixture, OutputId(2));
    let (window, initial, id) = opening::map(&mut fixture);
    window.role.set_title("Generation source".into()); window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Application(id)).unwrap(); fixture.inspect(|_| ());
    let mut buffers = vec![initial];
    let size = clients::repaint(&mut fixture, &window, id, &mut buffers);
    let item = clients::geometry(&fixture, id);
    let qh = fixture.queue.handle();
    let handle = fixture.client.foreign_handles.iter().find(|handle| fixture.client.foreign_names.get(&handle.id()).is_some_and(|name| name == "Generation source")).unwrap().clone();
    let (viewport, _) = fixture.output_pixels(OutputId(2));
    let local = size.map(|value| f64::from(value / 2));
    let point = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
    clients::pointer_click(&mut fixture, OutputId(2), point, &window.surface, local);
    let pointer = fixture.client.pointer.as_ref().unwrap().clone();
    let cursor = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let artwork = privacy::buffer(&fixture, [20, 12], [0, 96, 255, 255]);
    pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&cursor), 5, 3);
    cursor.attach(Some(&artwork), 0, 0); cursor.damage_buffer(0, 0, 20, 12); cursor.commit(); fixture.exchange(); fixture.inspect(|_| ());
    let mut baseline = None;
    for cycle in 0..20 {
        eprintln!("Color generation cycle {cycle}: output retirement");
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let old = capture_lifetime::Pending::new(&mut fixture, &session, [800, 600]);
        capture_lifetime::submitted(&mut fixture, &old);
        generation(&mut fixture);
        capture_lifetime::rejected(&mut fixture, &old);
        old.destroy(&mut fixture);
        let fresh = capture_lifetime::Pending::new(&mut fixture, &session, [800, 600]);
        let pixels = capture_lifetime::completed(&mut fixture, &fresh);
        let (_, color) = clients::MARKERS[0];
        let offset = clients::MARKERS[0].0;
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(offset[axis]));
        let near = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        clients::compare(clients::marker(&pixels, [800, 600], near, 1.0, color, true), near, "fresh public source after output color generation");
        fresh.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        let source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(&handle, &qh, ());
        let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ()); fixture.exchange();
        let direct = capture_lifetime::Pending::new(&mut fixture, &session, size);
        eprintln!("Color generation cycle {cycle}: raw direct source");
        capture_lifetime::submitted(&mut fixture, &direct);
        generation(&mut fixture);
        let pixels = capture_lifetime::completed(&mut fixture, &direct);
        let near = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(offset[axis]));
        clients::compare(clients::marker(&pixels, size, near, 1.0, color, true), near, "raw source independent of monitor color generation");
        direct.destroy(&mut fixture);
        let private = capture_lifetime::Pending::new(&mut fixture, &session, size);
        eprintln!("Color generation cycle {cycle}: private retirement");
        capture_lifetime::submitted(&mut fixture, &private);
        privacy::exclude_window(&mut fixture, id, true);
        capture_lifetime::rejected(&mut fixture, &private);
        private.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let filtered = capture_lifetime::Pending::new(&mut fixture, &session, [800, 600]);
        let pixels = capture_lifetime::completed(&mut fixture, &filtered);
        let point = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        for y in (point[1] - 4.0).floor() as usize..(point[1] + 4.0).ceil() as usize {
            for x in (point[0] - 3.0).floor() as usize..(point[0] + 3.0).ceil() as usize {
                let offset = (y * 800 + x) * 4;
                assert!(!(pixels[offset] < 20 && pixels[offset + 1] < 20 && pixels[offset + 2] > 235),
                    "A private application's source marker leaked after a color generation");
            }
        }
        filtered.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        privacy::exclude_window(&mut fixture, id, false);
        configuration::application_input(&mut fixture, id, &window);
        let output = fixture.client.output.as_ref().unwrap().clone();
        let source = fixture.client.capture.outputs.as_ref().unwrap().create_source(&output, &qh, ());
        let standalone = fixture.client.capture.manager.as_ref().unwrap().create_pointer_cursor_session(&source, &pointer, &qh, ());
        let session = standalone.get_capture_session(&qh, ()); fixture.exchange();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap(); fixture.exchange();
        let cursor_size = *fixture.client.capture.sizes.last().unwrap();
        assert_eq!(cursor_size, [25, 15]);
        let old = capture_lifetime::Pending::new(&mut fixture, &session, cursor_size);
        capture_lifetime::submitted(&mut fixture, &old);
        generation(&mut fixture);
        capture_lifetime::rejected(&mut fixture, &old); old.destroy(&mut fixture);
        let fresh = capture_lifetime::Pending::new(&mut fixture, &session, cursor_size);
        let pixels = capture_lifetime::completed(&mut fixture, &fresh);
        assert!(pixels.chunks_exact(4).all(|pixel| pixel[0] < 20 && (76..=116).contains(&pixel[1]) && pixel[2] > 235));
        fresh.destroy(&mut fixture); session.destroy(); standalone.destroy(); source.destroy(); fixture.exchange();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.tick();
            fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
            if fixture.desktop.server.capture_demand().is_empty() && fixture.composition.capture_test_state() == [0; 4] { break; }
            assert!(Instant::now() < deadline, "Color-generation jobs retained GPU readbacks or demand");
        }
        let actual = fixture.composition.color_test_resources();
        if let Some(before) = baseline { assert_eq!(actual, before, "Settled generation cycles retained compositor resources"); }
        else { baseline = Some(actual); }
        assert_eq!(clients::geometry(&fixture, id), item);
        assert!(fixture.desktop.running);
    }
    pointer.set_cursor(fixture.client.pointer_enter_serial, None, 0, 0); cursor.destroy(); artwork.destroy(); fixture.exchange();
    configuration::application_input(&mut fixture, id, &window);
    fixture.desktop.worker.stop().unwrap();
}
