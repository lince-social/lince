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
            composition.draw_popup(&mut frame, scene)?;
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
