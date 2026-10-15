use super::*;
use lince_interface::host::{ItemId, SurfaceId};
use native::actions::Action;

pub fn client_probe() -> Result<(), ProbeFailure> {
    let runtime =
        tokio::runtime::Runtime::new().map_err(|error| input_failure(&error.to_string()))?;
    let _entered = runtime.enter();
    let directory = tempfile_directory()?;
    let mut canvas = Presentation::new(
        [1024, 640],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )?;
    for _ in 0..8 {
        canvas.frame()?;
    }
    let world = canvas.world_mut();
    let root = world
        .query_filtered::<Entity, With<native::container::BoxRoot>>()
        .iter(world)
        .next()
        .ok_or_else(|| input_failure("the native canvas is unavailable"))?;
    native::edit_mode::EditAction::Open.apply(world, root);
    native::edit_mode::EditAction::AddSand(native::sand_store::SandKind::EditableText)
        .apply(world, root);
    native::edit_mode::EditAction::Close.apply(world, root);
    for _ in 0..8 {
        canvas.frame()?;
    }
    let surface = SurfaceId(1);
    canvas.open_window(surface, "Live graphics client", "park-graphics-proof")?;
    let renderer = canvas.renderer.clone();
    let mut report = crate::graphics::probe();
    if let Some(failure) = report.failure.take() {
        return Err(failure);
    }
    crate::graphics::with_client(&renderer, &mut report, |texture| {
        canvas.surface_texture(surface, texture)?;
        let mut frame = None;
        for _ in 0..8 {
            frame = Some(canvas.frame()?);
        }
        let pixels = probe_pixels(frame.as_ref().unwrap().dmabuf())?;
        let geometry = canvas
            .geometry()
            .ok_or_else(|| input_failure("no published application geometry"))?;
        let application = geometry
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(surface))
            .ok_or_else(|| input_failure("the live client has no application Castle"))?;
        let native = geometry
            .items
            .iter()
            .find(|item| matches!(item.id, ItemId::Native(_)))
            .ok_or_else(|| input_failure("the native Sand has no geometry"))?;
        let center = |rectangle: lince_interface::host::Rectangle| {
            std::array::from_fn::<_, 2, _>(|axis| {
                rectangle.origin()[axis] + rectangle.size()[axis] * 0.5
            })
        };
        let point = center(application.mapping.canvas);
        if application.mapping.surface_point(point)
            != Some([
                f64::from(texture.width()) * 0.5,
                f64::from(texture.height()) * 0.5,
            ])
        {
            return Err(input_failure(
                "the application content/input mapping disagrees",
            ));
        }
        let pixel = |point: [f64; 2]| -> Result<&[u8], ProbeFailure> {
            if point[0] < 0.0 || point[1] < 0.0 || point[0] >= 1024.0 || point[1] >= 640.0 {
                return Err(input_failure(
                    "the focused application is outside the canvas",
                ));
            }
            let offset = (point[1].floor() as usize * 1024 + point[0].floor() as usize) * 4;
            Ok(&pixels[offset..offset + 4])
        };
        let client_pixel = pixel(point)?;
        if texture.width() != 192
            && client_pixel
                .iter()
                .zip([0_u8, 64, 255, 255])
                .any(|(actual, expected)| actual.abs_diff(expected) > 5)
        {
            return Err(input_failure(&format!(
                "unexpected application pixels: {client_pixel:?}"
            )));
        }
        if texture.width() == 192 && (client_pixel[2] < 120 || client_pixel[3] < 250) {
            return Err(input_failure(&format!(
                "translucent content did not composite: {client_pixel:?}"
            )));
        }
        if pixel(center(native.mapping.canvas))? == [0_u8, 64, 255, 255] {
            return Err(input_failure("application content covered the native Sand"));
        }
        Ok(())
    })?;
    canvas.close_window(surface);
    for _ in 0..3 {
        canvas.frame()?;
    }
    if canvas.geometry().is_some_and(|geometry| {
        geometry
            .items
            .iter()
            .any(|item| item.id == ItemId::Application(surface))
    }) {
        return Err(input_failure("the closed application left a tile"));
    }
    Ok(())
}
