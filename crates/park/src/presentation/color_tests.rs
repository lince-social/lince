use super::*;

#[test]
#[ignore = "requires hardware Vulkan, native Bevy rendering and linear RGBA16F sharing"]
fn actual_native_camera_and_export_preserve_linear_values_above_sdr_white() {
    std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let _entered = runtime.enter();
        let directory = tempfile::tempdir().unwrap();
        let mut canvas = Presentation::new([8, 8], directory.path().into(), WakeSignal::new(|| {})).unwrap();
        canvas.prepare_linear_output(true).unwrap();
        assert!(canvas.linear_output, "The fixture requires actual native linear output support");
        for _ in 0..4 { canvas.probe_frame().unwrap(); }
        let camera = canvas.app.world().resource::<native::topology::presentation::SceneCamera>().0;
        assert!(canvas.app.world().get::<bevy::camera::Hdr>(camera).is_some());
        assert_eq!(canvas.app.world().get::<bevy::camera::CompositingSpace>(camera), Some(&bevy::camera::CompositingSpace::Linear));
        canvas.app.world_mut().spawn((Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..Default::default() },
            BackgroundColor(Color::linear_rgba(2.0, 0.5, 0.25, 1.0)), GlobalZIndex(i32::MAX), UiTargetCamera(camera)));
        for _ in 0..4 { canvas.probe_frame().unwrap(); }
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(frame.encoding, lince_interface::host::color::NativeEncoding::LinearRelative);
        frame.image.as_ref().unwrap().wait_for_probe().unwrap();
        let imported = canvas.renderer.import_dmabuf(frame.dmabuf().unwrap()).unwrap();
        assert_eq!(imported.format(), wgpu::TextureFormat::Rgba16Float);
        let buffer = canvas.renderer.device.create_buffer(&wgpu::BufferDescriptor { label: Some("park-native-linear-readback"),
            size: 2048, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = canvas.renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(imported.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(8) } }, imported.size());
        canvas.renderer.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = sender.send(result); });
        canvas.renderer.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) }).unwrap();
        receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap().unwrap();
        let bytes = slice.get_mapped_range();
        let offset = 4 * 256 + 4 * 8;
        let channels = std::array::from_fn::<_, 4, _>(|channel| u16::from_le_bytes([bytes[offset + channel * 2], bytes[offset + channel * 2 + 1]]));
        assert_eq!(channels, [0x4000, 0x3800, 0x3400, 0x3c00]);
        drop(bytes); buffer.unmap();
    }).unwrap().join().unwrap();
}
