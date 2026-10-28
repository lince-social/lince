use super::*;

#[test]
fn texture_damage_uses_destination_coordinates_after_output_and_control_clipping() {
    let rectangle = |x, y, w, h| Rect::new((x, y).into(), (w, h).into());
    assert_eq!(
        local_damage(rectangle(900, 70, 160, 100), rectangle(800, 30, 400, 300)),
        Some(rectangle(0, 0, 160, 100))
    );
    assert_eq!(
        local_damage(rectangle(900, 10, 160, 100), rectangle(800, 30, 400, 300)),
        Some(rectangle(0, 20, 160, 80))
    );
    assert_eq!(
        local_damage(rectangle(-20, -10, 160, 100), rectangle(0, 0, 400, 300)),
        Some(rectangle(20, 10, 140, 90))
    );
    assert_eq!(
        local_damage(rectangle(900, 70, 160, 100), rectangle(0, 0, 400, 300)),
        None
    );
}

#[test]
#[ignore = "requires hardware EGL with RGBA16F and RGB10A2 rendering/readback"]
fn actual_linear_alpha_and_final_hdr_encoding_preserve_precision_and_use_the_declared_transfer_once() {
    use lince_interface::host::color::{ColorMode, Description, OutputColor};
    use smithay::backend::{egl::{EGLContext, EGLDevice, EGLDisplay}, renderer::ExportMem};
    let device = EGLDevice::enumerate().unwrap().find(|device| !device.is_software()).expect("A hardware EGL device is required");
    let display = unsafe { EGLDisplay::new(device) }.unwrap();
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
    let mut composition = Composition::new(lince_interface::host::WakeSignal::new(|| {})).unwrap();
    composition.prepare_color(&mut renderer).unwrap();
    assert!(composition.linear_color_available(), "{:?}", composition.color_failure);
    let size: Size<i32, Physical> = (4, 4).into();
    let damage = Rect::from_size(size);
    let mut source: GlesTexture = renderer.create_buffer(Fourcc::Argb8888, (4, 4).into()).unwrap();
    {
        let mut target = renderer.bind(&mut source).unwrap();
        let mut frame = renderer.render(&mut target, size, Transform::Normal).unwrap();
        frame.clear(smithay::backend::renderer::Color32F::new(0.5, 0.5, 0.5, 0.5), &[damage]).unwrap();
        frame.finish().unwrap().wait().unwrap();
    }
    let mut working: GlesTexture = renderer.create_buffer(Fourcc::Abgr16161616f, (4, 4).into()).unwrap();
    composition.linear_composition = true;
    {
        let mut target = renderer.bind(&mut working).unwrap();
        let mut frame = renderer.render(&mut target, size, Transform::Normal).unwrap();
        frame.clear(smithay::backend::renderer::Color32F::BLACK, &[damage]).unwrap();
        composition.color_texture(&mut frame, Description::default(), false, false).unwrap();
        frame.render_texture_from_to(&source, Rect::from_size(source.size().to_f64()), damage, &[damage], &[], Transform::Normal, 1.0, None, &[]).unwrap();
        frame.clear_tex_program_override();
        frame.finish().unwrap().wait().unwrap();
    }
    composition.linear_composition = false;
    let mut sdr: GlesTexture = renderer.create_buffer(Fourcc::Argb8888, (4, 4).into()).unwrap();
    {
        let mut target = renderer.bind(&mut sdr).unwrap();
        composition.encode_output(&mut renderer, &mut target, &working, size, Transform::Normal, OutputColor::default()).unwrap().wait().unwrap();
        let mapping = renderer.copy_framebuffer(&target, Rect::from_size((4, 4).into()), Fourcc::Argb8888).unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        for pixel in bytes.chunks_exact(4) {
            assert!(pixel[..3].iter().all(|channel| (i32::from(*channel) - 188).abs() <= 1), "{pixel:?}");
            assert_eq!(pixel[3], 255);
        }
    }
    {
        let mut target = renderer.bind(&mut working).unwrap();
        let mut frame = renderer.render(&mut target, size, Transform::Normal).unwrap();
        frame.clear(smithay::backend::renderer::Color32F::new(12.5, 12.5, 12.5, 1.0), &[damage]).unwrap();
        frame.finish().unwrap().wait().unwrap();
        let mapping = renderer.copy_framebuffer(&target, Rect::from_size((4, 4).into()), Fourcc::Abgr16161616f).unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        assert_eq!(bytes.len(), 4 * 4 * 8);
        for pixel in bytes.chunks_exact(8) {
            let channels = std::array::from_fn::<_, 4, _>(|channel| u16::from_ne_bytes([pixel[channel * 2], pixel[channel * 2 + 1]]));
            assert_eq!(channels, [0x4a40, 0x4a40, 0x4a40, 0x3c00]);
        }
    }
    let mut hdr: GlesTexture = renderer.create_buffer(Fourcc::Abgr2101010, (4, 4).into()).unwrap();
    {
        let mut target = renderer.bind(&mut hdr).unwrap();
        composition.encode_output(&mut renderer, &mut target, &working, size, Transform::Normal,
            OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 }).unwrap().wait().unwrap();
        let mapping = renderer.copy_framebuffer(&target, Rect::from_size((4, 4).into()), Fourcc::Abgr2101010).unwrap();
        let bytes = renderer.map_texture(&mapping).unwrap();
        for pixel in bytes.chunks_exact(4) {
            let packed = u32::from_ne_bytes(pixel.try_into().unwrap());
            for channel in [packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023] {
                assert!((i64::from(channel) - 769).abs() <= 1, "{channel}");
            }
            assert_eq!(packed >> 30, 3);
        }
    }
}

#[cfg(feature = "server-tests")]
pub(crate) fn popup_pixels(
    window: crate::server::WindowSnapshot,
    item: lince_interface::host::ItemGeometry,
    clips: Vec<Rectangle>,
    size: [i32; 2],
) -> io::Result<Vec<u8>> {
    use smithay::backend::{
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{ExportMem, TextureMapping},
    };
    let device = EGLDevice::enumerate()
        .map_err(error)?
        .find(|device| !device.is_software())
        .ok_or_else(|| io::Error::other("The popup probe requires a hardware EGL device"))?;
    let display = unsafe { EGLDisplay::new(device) }.map_err(error)?;
    let context = EGLContext::new(&display).map_err(error)?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(error)?;
    let mut composition = Composition::new(lince_interface::host::WakeSignal::new(|| {}))?;
    composition.prepare_color(&mut renderer)?;
    let mut scenes = Vec::new();
    for popup in &window.popups {
        for node in &popup.nodes {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !composition.import(&mut renderer, node)? {
                if Instant::now() >= deadline {
                    return Err(io::Error::other("The popup buffer did not become ready"));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        scenes.push(PopupScene {
            owner: PopupOwner::Application(window.id),
            surface: popup.surface.clone(),
            output: item.output,
            mapping: popup
                .mapping(&window, &item)
                .ok_or_else(|| io::Error::other("Invalid popup mapping"))?,
            nodes: popup.nodes.clone(),
            clips: clips.clone(),
        });
    }
    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Argb8888, (size[0], size[1]).into())
        .map_err(error)?;
    let mapping = {
        let mut target = renderer.bind(&mut texture).map_err(error)?;
        let mut frame = renderer
            .render(
                &mut target,
                (size[0], size[1]).into(),
                Transform::Flipped180,
            )
            .map_err(error)?;
        frame
            .clear(
                smithay::backend::renderer::Color32F::TRANSPARENT,
                &[Rect::from_size((size[0], size[1]).into())],
            )
            .map_err(error)?;
        for scene in &scenes {
            composition.capture_popup_draw(&mut frame, scene, RasterMapping::default(), Rect::from_size((size[0], size[1]).into()))?;
        }
        frame.finish().map_err(error)?.wait().map_err(error)?;
        renderer
            .copy_framebuffer(
                &target,
                Rect::from_size((size[0], size[1]).into()),
                Fourcc::Argb8888,
            )
            .map_err(error)?
    };
    let bytes = renderer.map_texture(&mapping).map_err(error)?;
    let row = size[0] as usize * 4;
    let mut pixels = Vec::with_capacity(bytes.len());
    for y in 0..size[1] as usize {
        let y = if mapping.flipped() {
            size[1] as usize - y - 1
        } else {
            y
        };
        pixels.extend_from_slice(&bytes[y * row..(y + 1) * row]);
    }
    Ok(pixels)
}
