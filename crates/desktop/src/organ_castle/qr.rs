use super::*;

pub(super) const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "nokhwa",
        author: "l1npengtul and contributors",
        license: include_str!("../../licenses/nokhwa-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "qrcode-rust",
        author: "Kenneth Yip and contributors",
        license: include_str!("../../licenses/qrcode-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "rqrr",
        author: "Wanja B. (WanzenBug), Daniel Beer, and contributors",
        license: include_str!("../../licenses/rqrr-LICENSES.txt"),
    },
];

#[derive(Component)]
pub(super) struct ScanTarget(pub Entity);

pub(super) fn pixels(text: &str) -> Result<(u32, Vec<u8>), String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    let modules = code.width();
    let scale = 4;
    let width = (modules + 8) * scale;
    let mut pixels = vec![255u8; width * width * 4];
    for y in 0..modules {
        for x in 0..modules {
            if code[(x, y)] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let index = (((y + 4) * scale + dy) * width + (x + 4) * scale + dx) * 4;
                    pixels[index..index + 3].fill(0);
                }
            }
        }
    }
    Ok((width as u32, pixels))
}

pub(super) fn code(world: &mut World, parent: Entity, text: &str) {
    label(world, parent, text);
    let status = label(world, parent, "");
    panel::button(world, parent, status, "Copy code", Copy(text.into()));
    match pixels(text) {
        Ok((width, pixels)) => {
            world.init_resource::<Assets<Image>>();
            let mut image = Image::new(
                bevy::render::render_resource::Extent3d {
                    width,
                    height: width,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                pixels,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::MAIN_WORLD
                    | bevy::asset::RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = bevy::image::ImageSampler::nearest();
            let image = world.resource_mut::<Assets<Image>>().add(image);
            world.spawn((
                ChildOf(parent),
                ImageNode::new(image),
                Node {
                    width: percent(100),
                    max_width: px((width as f32).min(400.0)),
                    aspect_ratio: Some(1.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
        }
        Err(error) => {
            label(
                world,
                parent,
                &format!("QR unavailable: {error}. Copy the code instead."),
            );
        }
    }
}

#[derive(Clone)]
struct Copy(String);
impl Action for Copy {
    fn apply(&self, world: &mut World, entity: Entity) {
        let copied = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .is_some_and(|mut clipboard| clipboard.set_text(self.0.clone()).is_ok());
        panel::status(
            world,
            entity,
            if copied {
                "Copied"
            } else {
                "Clipboard is unavailable"
            },
        );
    }
}

pub(super) fn scanner(world: &mut World, owner: Entity, parent: Entity, target: Entity) {
    let form = form(
        world,
        owner,
        parent,
        "Read QR image",
        json!({"action":"scan-file","path":""}),
        vec![Field("/path", "QR image path", Kind::Text, json!(""))],
        None,
    );
    world.entity_mut(form).insert(ScanTarget(target));
    {
        let camera = forms::form(
            world,
            owner,
            parent,
            "Scan with camera",
            json!({"action":"scan-camera", "camera":0}),
            vec![Field("/camera", "Camera number", Kind::Number, json!(0))],
            None,
        );
        world.entity_mut(camera).insert(ScanTarget(target));
        panel::button(world, parent, camera, "Stop camera scan", CancelScan);
    }
}

pub(super) fn decode_image(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("QR image is larger than 16 MiB".into());
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if u64::from(width) * u64::from(height) > 16_000_000 {
        return Err("QR image exceeds 16 million pixels".into());
    }
    engine::pairing::decode_qr(bytes)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No QR code found".into())
}

pub(super) fn scan_file(world: &mut World, entity: Entity, payload: &Value) -> Result<(), String> {
    let path = payload["path"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("Choose a QR image path")?
        .to_string();
    let handle = tokio::runtime::Handle::try_current().map_err(|_| "Runtime is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn_blocking(move || {
        use std::io::Read;
        let result = (|| {
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Choose an image file".into());
            }
            let mut bytes = Vec::new();
            file.take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            decode_image(&bytes).map(|text| json!({"scanned":text}))
        })();
        let _ = tx.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    begin_job(world, entity, rx);
    Ok(())
}

#[derive(Clone)]
struct CancelScan;
impl Action for CancelScan {
    fn apply(&self, world: &mut World, entity: Entity) {
        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).remove::<Job>();
            finish(world, entity, Err("Camera scan stopped".into()));
        }
    }
}

pub(super) fn scan_camera(
    world: &mut World,
    entity: Entity,
    payload: &Value,
) -> Result<(), String> {
    let camera_index = payload["camera"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or("Choose a camera number")?;
    let handle = tokio::runtime::Handle::try_current().map_err(|_| "Runtime is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn_blocking(move || {
        let result = (|| {
            use image::ImageEncoder;
            use nokhwa::{
                Camera,
                pixel_format::RgbFormat,
                utils::{
                    CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
                    Resolution,
                },
            };
            let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
                CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 15),
            ));
            let mut camera =
                Camera::new(CameraIndex::Index(camera_index), format).map_err(|e| e.to_string())?;
            if u64::from(camera.resolution().width()) * u64::from(camera.resolution().height())
                > 4_000_000
            {
                return Err("Choose a camera mode below four million pixels".into());
            }
            camera.open_stream().map_err(|e| e.to_string())?;
            let started = std::time::Instant::now();
            while started.elapsed().as_secs() < 30 && !tx.is_closed() {
                let frame = camera
                    .frame()
                    .map_err(|e| e.to_string())?
                    .decode_image::<RgbFormat>()
                    .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                image::codecs::png::PngEncoder::new(&mut bytes)
                    .write_image(
                        frame.as_raw(),
                        frame.width(),
                        frame.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| e.to_string())?;
                if let Ok(text) = decode_image(&bytes) {
                    return Ok(json!({"scanned":text}));
                }
            }
            Err("No QR code seen. Try again or read an image file.".into())
        })();
        let _ = tx.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    begin_job(world, entity, rx);
    let output = world.get::<forms::Form>(entity).unwrap().output;
    report(
        world,
        output,
        "Point the camera at a QR code. Scanning stops after 30 seconds.",
    );
    Ok(())
}
