use super::*;
use super::super::{capture_lifetime, magnification::clients, opening};
use crate::{graphics::{ExportedImage, VulkanRenderer}, launch::Capability};
use lince_interface::host::color::{ColorMode, NativeEncoding, OutputColor, Transfer};
use smithay::backend::renderer::ExportMem;
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_manager_v1 as manager, wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_image_description_v1::WpImageDescriptionV1,
};

const A: OutputId = OutputId(1);
const B: OutputId = OutputId(2);
const B_SIZE: [u32; 2] = [800, 600];
const TO_2020: [[f64; 3]; 3] = [
    [0.6274038959, 0.3292830384, 0.0433130657],
    [0.0690972894, 0.9195403951, 0.0113623156],
    [0.0163914389, 0.0880133079, 0.8955952532],
];
const HALF: [[u16; 3]; 5] = [
    [0x3400; 3], [0x3800; 3], [0x4000; 3], [0x4a40; 3], [0xb400, 0x3800, 0x4000],
];
const LINEAR: [[f64; 3]; 5] = [[0.25; 3], [0.5; 3], [2.0; 3], [12.5; 3], [-0.25, 0.5, 2.0]];
const PACKED: [[u32; 3]; 5] = [[256; 3], [513; 3], [700; 3], [767; 3], [256, 513, 767]];

#[derive(Clone, Copy, Debug)]
enum Samples { Float, Packed, Overlay }

struct Upload { buffer: wl_buffer::WlBuffer, _image: ExportedImage }

fn upload(renderer: &VulkanRenderer, client: &Client, queue: &QueueHandle<Client>, size: [u32; 2], samples: Samples) -> Upload {
    let format = if matches!(samples, Samples::Packed) { wgpu::TextureFormat::Rgb10a2Unorm } else { wgpu::TextureFormat::Rgba16Float };
    let stride = size[0] * if matches!(samples, Samples::Packed) { 4 } else { 8 };
    let mut pixels = Vec::with_capacity(stride as usize * size[1] as usize);
    for _ in 0..size[1] {
        for x in 0..size[0] {
            let band = (x as usize * 5 / size[0] as usize).min(4);
            match samples {
                Samples::Packed => {
                    let [r, g, b] = PACKED[band];
                    pixels.extend_from_slice(&(r | (g << 10) | (b << 20) | (3 << 30)).to_le_bytes());
                }
                Samples::Float | Samples::Overlay => {
                    let rgba = if matches!(samples, Samples::Overlay) { [0x3800; 4] } else { let [r, g, b] = HALF[band]; [r, g, b, 0x3c00] };
                    for channel in rgba { pixels.extend_from_slice(&channel.to_le_bytes()); }
                }
            }
        }
    }
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("park-real-color-client"), size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
    });
    renderer.queue.write_texture(texture.as_image_copy(), &pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(size[1]) }, texture.size());
    let image = renderer.export_copy(&texture).unwrap(); image.wait_for_probe().unwrap();
    let buffer = dmabuf_tests::buffer(client, queue, &image.dmabuf);
    Upload { buffer, _image: image }
}

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Fixture {
    let mut server = Server::new([1800, 1000]).unwrap();
    server.configure_scaled_outputs(&[
        Monitor { id: A, name: "Color-A".into(), origin: [120, 40], size: [1000, 720] },
        Monitor { id: B, name: "Color-B".into(), origin: [920, 80], size: B_SIZE.map(|value| value as i32) },
    ], &std::collections::BTreeMap::from([(A, 1.25), (B, 2.0)])).unwrap();
    server.registered_output_catalog().unwrap();
    let harness = Harness::connect_authorized(server, Some(vec![Capability::Capture, Capability::WindowList]));
    assert!(harness.client.color.manager.is_none());
    let mut fixture = Fixture::with_linear_output(harness, runtime, directory, [1800, 1000]);
    fixture.desktop.commands(directory.join("park.json"), directory.join("wayland-color"));
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2))
        && fixture.client.color.manager.is_some() && fixture.client.dmabuf.manager.is_some());
    assert!(fixture.composition.linear_color_available());
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.encoding, NativeEncoding::LinearRelative);
    fixture.exchange();
    for format in [Fourcc::Abgr16161616f, Fourcc::Abgr2101010] {
        assert!(fixture.client.dmabuf.modifiers.contains(&(format as u32, 0)),
            "The complete importer intersection must advertise {format:?}: Vulkan={:?}, EGL={:?}, advertised={:?}",
            fixture.desktop.current.as_ref().unwrap().frame.client_formats, fixture.renderer.egl_context().dmabuf_texture_formats(), fixture.client.dmabuf.modifiers);
    }
    assert!(fixture.client.color.transfers.contains(&manager::TransferFunction::ExtLinear));
    assert!(fixture.client.color.transfers.contains(&manager::TransferFunction::St2084Pq));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
        world.get_mut::<native::topology::view::View>(root).unwrap().spatial = false;
    });
    fixture
}

fn output(fixture: &mut Fixture, id: OutputId) {
    let serial = fixture.desktop.worker.input(Input::Output(id)).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(id)));
}

fn description(fixture: &mut Fixture, key: u32, transfer: Transfer) -> WpImageDescriptionV1 {
    let qh = fixture.queue.handle();
    let params = fixture.client.color.manager.as_ref().unwrap().create_parametric_creator(&qh, ());
    params.set_primaries_named(manager::Primaries::Srgb);
    params.set_tf_named(match transfer { Transfer::Linear => manager::TransferFunction::ExtLinear, Transfer::Pq => manager::TransferFunction::St2084Pq,
        Transfer::Gamma22 => manager::TransferFunction::Gamma22, Transfer::Srgb => manager::TransferFunction::Srgb });
    if matches!(transfer, Transfer::Linear | Transfer::Pq) { params.set_luminances(0, 10000, 80); }
    let image = params.create(&qh, key);
    fixture.exchange();
    fixture.until(|fixture| fixture.client.color.ready.contains_key(&key));
    image
}

fn source(fixture: &Fixture, id: SurfaceId) -> crate::server::SurfaceNode {
    fixture.desktop.server.windows().into_iter().find(|window| window.id == id).unwrap().nodes[0].clone()
}

fn unobstructed(fixture: &mut Fixture, output: OutputId) {
    let bounds = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|candidate| candidate.id == output).unwrap().bounds;
    fixture.desktop.move_pointer(bounds.origin().map(|value| value + 8.0), 59).unwrap();
    fixture.inspect(|_| ());
}

fn committed(fixture: &mut Fixture, id: SurfaceId, version: u64, transfer: Transfer) {
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().find(|window| window.id == id)
        .is_some_and(|window| window.nodes[0].content.version == version && window.nodes[0].color.transfer == transfer)));
}

fn candidate(fixture: &mut Fixture, output: OutputId) -> Vec<u8> {
    let frame = fixture.desktop.current.as_ref().unwrap().clone();
    let viewport = frame.frame.geometry.as_ref().unwrap().outputs.iter().find(|viewport| viewport.id == output).unwrap();
    let size = viewport.source_size().map(|value| value as i32);
    let mut texture: GlesTexture = fixture.renderer.create_buffer(Fourcc::Abgr2101010, (size[0], size[1]).into()).unwrap();
    let mut target = fixture.renderer.bind(&mut texture).unwrap();
    let presented = fixture.composition.render_output(&mut fixture.renderer, &mut target, viewport, &mut fixture.desktop.server,
        Some(frame.as_ref()), fixture.desktop.pointer, OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 }).unwrap();
    presented.sync.wait().unwrap();
    let mapping = fixture.renderer.copy_framebuffer(&target, smithay::utils::Rectangle::from_size((size[0], size[1]).into()), Fourcc::Abgr2101010).unwrap();
    let bytes = fixture.renderer.map_texture(&mapping).unwrap();
    let stride = size[0] as usize * 4;
    let mut result = Vec::with_capacity(bytes.len());
    for row in 0..size[1] as usize {
        result.extend_from_slice(&bytes[row * stride..(row + 1) * stride]);
    }
    result
}

fn decode_pq(code: u32) -> f64 {
    let power = (f64::from(code) / 1023.0).powf(32.0 / 2523.0);
    10000.0 * ((power - 3424.0 / 4096.0).max(0.0) / (2413.0 / 128.0 - (2392.0 / 128.0) * power)).powf(16384.0 / 2610.0)
}

fn encode_pq(nits: f64) -> i32 {
    let power = (nits.clamp(0.0, 1000.0) / 10000.0).powf(2610.0 / 16384.0);
    ((((3424.0 / 4096.0 + (2413.0 / 128.0) * power) / (1.0 + (2392.0 / 128.0) * power)).powf(2523.0 / 32.0)) * 1023.0).round() as i32
}

fn encode_sdr(nits: f64) -> i32 {
    let linear = (nits / 203.0).clamp(0.0, 1.0);
    ((if linear <= 0.0031308 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }) * 255.0).round() as i32
}

#[track_caller]
fn sample(pixels: &[u8], size: [u32; 2], point: [f64; 2], expected: [i32; 3], ten_bit: bool, bgra: bool) {
    let point = point.map(|value| value.round() as i32);
    assert!((0..2).all(|axis| point[axis] > 1 && point[axis] < size[axis] as i32 - 2));
    for y in point[1] - 1..=point[1] + 1 {
        for x in point[0] - 1..=point[0] + 1 {
            let offset = (y as usize * size[0] as usize + x as usize) * 4;
            let actual = if ten_bit {
                let packed = u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap());
                assert_eq!(packed >> 30, 3);
                [packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023].map(|value| value as i32)
            } else {
                assert_eq!(pixels[offset + 3], 255);
                (if bgra { [pixels[offset + 2], pixels[offset + 1], pixels[offset]] } else { [pixels[offset], pixels[offset + 1], pixels[offset + 2]] }).map(i32::from)
            };
            assert!((0..3).all(|channel| (actual[channel] - expected[channel]).abs() <= 1), "Real color sample at {point:?}: {actual:?} != {expected:?}");
        }
    }
}

fn check(fixture: &mut Fixture, id: SurfaceId, size: [u32; 2], samples: Samples, transfer: Transfer, bands: usize) -> Vec<[f64; 2]> {
    eprintln!("Checking real color client: {samples:?}, {transfer:?}, {}", source(fixture, id).content.version);
    unobstructed(fixture, B);
    let item = clients::geometry(fixture, id);
    let (viewport, sdr) = fixture.output_pixels(B);
    let hdr = candidate(fixture, B);
    let (capture_source, session) = capture_lifetime::output_session(fixture);
    let pending = capture_lifetime::Pending::new(fixture, &session, B_SIZE);
    let captured = capture_lifetime::completed(fixture, &pending);
    let qh = fixture.queue.handle();
    let title = fixture.desktop.server.windows().into_iter().find(|window| window.id == id).unwrap().title;
    let handle = fixture.client.foreign_handles.iter().find(|handle| fixture.client.foreign_names.get(&handle.id()) == Some(&title)).unwrap();
    let direct_source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(handle, &qh, ());
    let direct_session = fixture.client.capture.manager.as_ref().unwrap().create_session(&direct_source,
        wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options::empty(), &qh, ());
    fixture.exchange();
    let direct_pending = capture_lifetime::Pending::new(fixture, &direct_session, size);
    assert!(fixture.desktop.server.capture_demand().windows.contains(&id));
    let direct_pixels = capture_lifetime::completed(fixture, &direct_pending);
    let mut points = Vec::new();
    for band in 0..bands {
        let local = [f64::from(size[0]) * (band as f64 + 0.5) / 5.0, f64::from(size[1]) * 0.5];
        let point = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        let nits = if matches!(samples, Samples::Packed) { PACKED[band].map(decode_pq) }
            else if transfer == Transfer::Gamma22 { LINEAR[band].map(|value| value.clamp(0.0, 1.0).powf(2.2) * 203.0) }
            else { LINEAR[band].map(|value| value * 80.0) };
        let sdr_expected = nits.map(encode_sdr);
        let hdr_expected = std::array::from_fn(|row| encode_pq((0..3).map(|column| TO_2020[row][column] * nits[column]).sum()));
        sample(&sdr, viewport.source_size(), point, sdr_expected, false, false);
        sample(&captured, B_SIZE, point, sdr_expected, false, true);
        sample(&direct_pixels, size, local, sdr_expected, false, true);
        sample(&hdr, viewport.source_size(), point, hdr_expected, true, false);
        points.push(point);
    }
    pending.destroy(fixture); session.destroy(); capture_source.destroy(); fixture.exchange();
    direct_pending.destroy(fixture); direct_session.destroy(); direct_source.destroy(); fixture.exchange();
    points
}

fn overlays(fixture: &mut Fixture, renderer: &VulkanRenderer, point: [f64; 2]) {
    let mut baseline = None;
    let server_before = fixture.desktop.server.color_test_resources();
    for cycle in 0..20 {
        let qh = fixture.queue.handle();
        let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let layer = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, fixture.client.output.as_ref(),
            zwlr_layer_shell_v1::Layer::Overlay, "park-linear-color-fixture".into(), &qh, ());
        layer.set_size(32, 24); layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
        layer.set_exclusive_zone(-1);
        layer.set_margin((point[1] / 2.0 - 12.0).floor() as i32, 0, 0, (point[0] / 2.0 - 16.0).floor() as i32);
        surface.commit(); fixture.exchange();
        let color = fixture.client.color.manager.as_ref().unwrap().get_surface(&surface, &qh, ());
        let image = description(fixture, 100 + cycle, Transfer::Linear);
        color.set_image_description(&image, manager::RenderIntent::Perceptual); image.destroy();
        let upload = upload(renderer, &fixture.client, &qh, [32, 24], Samples::Overlay);
        surface.attach(Some(&upload.buffer), 0, 0); surface.damage_buffer(0, 0, 32, 24); surface.commit(); fixture.exchange();
        let snapshot = fixture.desktop.server.layers().into_iter().find(|layer| layer.namespace == "park-linear-color-fixture").unwrap();
        let target = snapshot.surface; let version = snapshot.nodes[0].content.version;
        fixture.until(|fixture| fixture.composition.color_test_surface_ready(&target, version));
        let (_, sdr) = fixture.output_pixels(B);
        let hdr = candidate(fixture, B);
        let nits = 50.0;
        sample(&sdr, B_SIZE, point, [encode_sdr(nits); 3], false, false);
        sample(&hdr, B_SIZE, point, [encode_pq(nits); 3], true, false);
        layer.destroy(); color.destroy(); surface.destroy(); upload.buffer.destroy(); fixture.exchange();
        fixture.until(|fixture| fixture.composition.color_test_resources()[1..6] == [0; 5]);
        let actual = fixture.composition.color_test_resources();
        assert!(actual[7] <= 256 * 1024 * 1024);
        if let Some(before) = baseline { assert_eq!(actual, before, "Color overlay resources grew after settled cycle {cycle}"); } else { baseline = Some(actual); }
        let server = fixture.desktop.server.color_test_resources();
        assert_eq!(&server[..3], &server_before[..3]);
        assert!(server[3] <= server_before[3].max(1));
    }
}

#[test]
#[ignore = "requires hardware Vulkan, RGBA16F DMA-BUF import and ten-bit EGL rendering/readback"]
fn negotiated_float_and_ten_bit_wayland_clients_preserve_wide_gamut_precision_metadata_only_updates_alpha_and_companion_pixels_without_resizing() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = create(&runtime, directory.path());
    let renderer = VulkanRenderer::new().unwrap();
    output(&mut fixture, A);
    let (first, initial, first_id) = opening::map(&mut fixture);
    first.role.set_max_size(180, 120); first.surface.commit(); fixture.exchange();
    let mut first_buffers = vec![initial];
    let first_size = clients::repaint(&mut fixture, &first, first_id, &mut first_buffers);
    let first_item = clients::geometry(&fixture, first_id);
    let (first_view, first_pixels) = fixture.output_pixels(A);
    let first_points: [[f64; 2]; 3] = std::array::from_fn(|index| {
        let local = std::array::from_fn(|axis| f64::from(first_size[axis] / 2) + f64::from(clients::MARKERS[index].0[axis]));
        let near = first_view.raster.point(first_item.mapping.canvas_point(local).unwrap());
        clients::marker(&first_pixels, first_view.physical_size, near, 1.0, clients::MARKERS[index].1, false)
    });
    let first_mapping = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(A).unwrap().clone();
    output(&mut fixture, B);
    let (window, initial, id) = opening::map(&mut fixture);
    window.role.set_title("Negotiated high precision".into()); window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Free))).unwrap();
    fixture.desktop.send(Input::Application(id)).unwrap(); fixture.inspect(|_| ());
    let mut retained = vec![initial];
    let size = clients::repaint(&mut fixture, &window, id, &mut retained);
    let geometry = clients::geometry(&fixture, id);
    let configure = fixture.client.window_configure_sizes[&window.role.id()].len();
    let qh = fixture.queue.handle();
    let color: WpColorManagementSurfaceV1 = fixture.client.color.manager.as_ref().unwrap().get_surface(&window.surface, &qh, ());
    for (key, samples, transfer) in [(1, Samples::Float, Transfer::Linear), (2, Samples::Packed, Transfer::Pq)] {
        let image = description(&mut fixture, key, transfer);
        let upload = upload(&renderer, &fixture.client, &qh, size, samples);
        color.set_image_description(&image, manager::RenderIntent::Perceptual); image.destroy();
        window.surface.attach(Some(&upload.buffer), 0, 0); window.surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); window.surface.commit(); fixture.exchange();
        let version = source(&fixture, id).content.version;
        committed(&mut fixture, id, version, transfer);
        let points = check(&mut fixture, id, size, samples, transfer, 5);
        if matches!(samples, Samples::Float) {
            overlays(&mut fixture, &renderer, points[0]);
            let image = description(&mut fixture, 51, Transfer::Gamma22);
            color.set_image_description(&image, manager::RenderIntent::Perceptual); image.destroy();
            assert_eq!(source(&fixture, id).color.transfer, Transfer::Linear);
            window.surface.commit(); fixture.exchange(); committed(&mut fixture, id, version, Transfer::Gamma22);
            check(&mut fixture, id, size, samples, Transfer::Gamma22, 2);
            let image = description(&mut fixture, 52, Transfer::Linear);
            color.set_image_description(&image, manager::RenderIntent::Perceptual); image.destroy();
            window.surface.commit(); fixture.exchange(); committed(&mut fixture, id, version, Transfer::Linear);
            check(&mut fixture, id, size, samples, Transfer::Linear, 5);
        }
        assert_eq!(source(&fixture, id).content.version, version);
        assert_eq!(clients::geometry(&fixture, id), geometry);
        assert!(fixture.client.window_configure_sizes[&window.role.id()][configure..].iter().all(|configured| *configured == size.map(|value| value as i32)));
        let local = [f64::from(size[0]) * 0.1, f64::from(size[1]) * 0.5];
        clients::pointer_click(&mut fixture, B, points[0], &window.surface, local);
        fixture.client.key_recipients.clear();
        fixture.desktop.key(30, true, 40).unwrap(); fixture.desktop.key(30, false, 41).unwrap(); fixture.inspect(|_| ());
        assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
        let (_, pixels) = fixture.output_pixels(A);
        for index in 0..3 {
            clients::compare(clients::marker(&pixels, first_view.physical_size, first_points[index], 1.0, clients::MARKERS[index].1, false),
                first_points[index], "companion's public pixels under client color changes");
        }
        assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(A), Some(&first_mapping));
        assert_eq!(clients::geometry(&fixture, first_id), first_item);
        assert_eq!(fixture.desktop.server.display_snapshot().color[&B].effective.unwrap().mode, ColorMode::Srgb);
        upload.buffer.destroy(); fixture.exchange();
    }
    color.destroy(); window.role.destroy(); window.xdg.destroy(); window.surface.destroy(); fixture.exchange();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != id));
    assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
    assert!(fixture.desktop.worker.failure().is_none());
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan, DMA-BUF import and EGL rendering"]
fn an_unimportable_advertised_precision_buffer_disconnects_only_its_client_and_fresh_pixels_input_and_resources_recover() {
    use smithay::backend::allocator::{Modifier, dmabuf::{Dmabuf, DmabufFlags}};
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = create(&runtime, directory.path());
    output(&mut fixture, B);
    let (window, initial, id) = opening::map(&mut fixture);
    window.role.set_max_size(240, 180); window.surface.commit();
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Free))).unwrap();
    fixture.desktop.send(Input::Application(id)).unwrap();
    let mut retained = vec![initial];
    let size = clients::repaint(&mut fixture, &window, id, &mut retained);
    let baseline = fixture.composition.color_test_resources();
    let leases = fixture.desktop.server.color_test_resources()[0];
    for cycle in 0..20 {
        let (server, stream) = std::os::unix::net::UnixStream::pair().unwrap();
        fixture.desktop.server.insert_client(server).unwrap();
        let connection = Connection::from_socket(stream).unwrap();
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut client = Client::default();
        synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
        let qh = queue.handle();
        let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
        let role = xdg.get_toplevel(&qh, ());
        let app_id = format!("social.lince.park.unimportable-{cycle}");
        role.set_app_id(app_id.clone()); surface.commit();
        synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).unwrap();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&[0; 8].repeat(64 * 64)).unwrap();
        let mut builder = Dmabuf::builder((64, 64), Fourcc::Abgr16161616f, Modifier::Linear, DmabufFlags::empty());
        assert!(builder.add_plane(file.into(), 0, 0, 64 * 8));
        let invalid = builder.build().unwrap();
        let buffer = dmabuf_tests::buffer(&client, &qh, &invalid);
        surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 64, 64); surface.commit();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if synchronize(&mut fixture.desktop.server, &connection, &mut queue, &mut client).is_err() { break; }
            fixture.tick();
            assert!(fixture.desktop.worker.failure().is_none(), "An invalid client buffer killed the native worker");
            assert!(Instant::now() < deadline, "The unimportable client was neither refused nor disconnected");
            std::thread::sleep(Duration::from_millis(2));
        }
        fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.app_id != app_id)
            && fixture.composition.color_test_resources()[1..6] == [0; 5]);
        assert!(fixture.desktop.running);
        assert_eq!(fixture.desktop.server.color_test_resources()[0], leases);
        assert_eq!(fixture.composition.color_test_resources(), baseline);
        fixture.desktop.send(Input::Application(id)).unwrap(); fixture.inspect(|_| ());
        unobstructed(&mut fixture, B);
        let item = clients::geometry(&fixture, id);
        let (viewport, pixels) = fixture.output_pixels(B);
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(clients::MARKERS[0].0[axis]));
        let near = viewport.raster.point(item.mapping.canvas_point(local).unwrap());
        let point = clients::marker(&pixels, viewport.source_size(), near, 1.0, clients::MARKERS[0].1, false);
        clients::pointer_click(&mut fixture, B, point, &window.surface, local);
        fixture.client.key_recipients.clear(); fixture.desktop.key(30, true, 60).unwrap(); fixture.desktop.key(30, false, 61).unwrap(); fixture.inspect(|_| ());
        assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let pending = capture_lifetime::Pending::new(&mut fixture, &session, B_SIZE);
        let captured = capture_lifetime::completed(&mut fixture, &pending);
        clients::compare(clients::marker(&captured, B_SIZE, point, 1.0, clients::MARKERS[0].1, true), point, "public capture after an independent import failure");
        pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
    }
    let (fresh, buffer, fresh_id) = opening::map(&mut fixture);
    fresh.role.set_max_size(180, 120); fresh.surface.commit(); fixture.desktop.send(Input::Application(fresh_id)).unwrap();
    let mut buffers = vec![buffer]; clients::repaint(&mut fixture, &fresh, fresh_id, &mut buffers);
    fixture.client.key_recipients.clear(); fixture.desktop.key(30, true, 70).unwrap(); fixture.desktop.key(30, false, 71).unwrap(); fixture.inspect(|_| ());
    assert_eq!(fixture.client.key_recipients, [(Some(fresh.surface.id()), 30, true), (Some(fresh.surface.id()), 30, false)]);
    assert!(fixture.desktop.worker.failure().is_none());
    fixture.desktop.worker.stop().unwrap();
}
