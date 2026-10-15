use super::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
};

const MAX_FILE: u64 = 8 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_packages_are_resized_and_unsafe_or_oversized_files_are_rejected() {
        let source = tempfile::tempdir().unwrap();
        let storage = tempfile::tempdir().unwrap();
        let path = source.path().join("source.png");
        image::RgbaImage::from_pixel(2048, 512, image::Rgba([120, 180, 220, 255]))
            .save(&path)
            .unwrap();
        let id = package(storage.path(), &path).unwrap();
        assert!(model::Texture::Asset { id: id.clone() }.valid());
        let decoded = decode(&storage.path().join(id).join("source.png")).unwrap();
        assert_eq!((decoded.width, decoded.height), (1024, 256));
        assert_eq!(&decoded.rgba[..4], &[120, 180, 220, 255]);
        let oversized = source.path().join("oversized.png");
        std::fs::File::create(&oversized)
            .unwrap()
            .set_len(MAX_FILE + 1)
            .unwrap();
        assert!(decode(&oversized).is_err());
        assert!(decode(source.path()).is_err());
        std::fs::write(&path, b"untrusted non-image bytes").unwrap();
        assert!(package(storage.path(), &path).is_err());
    }
}

struct Pending {
    key: model::Texture,
    receiver: Mutex<mpsc::Receiver<Result<crate::native::media_sand::Pixels, String>>>,
}

struct Import {
    owner: Entity,
    directory: PathBuf,
    previous: Option<model::Material>,
    receiver: Mutex<mpsc::Receiver<Result<String, String>>>,
}

#[derive(Resource, Default)]
struct Jobs {
    pending: Vec<Pending>,
    failed: HashMap<model::Texture, String>,
    import: Option<Import>,
}

fn decode(path: &Path) -> Result<crate::native::media_sand::Pixels, String> {
    let file = crate::native::media_sand::open_file(path, MAX_FILE)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Texture files are limited to 8 MiB.".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("Choose a PNG, JPEG or WebP texture.".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| error.to_string())?;
    let pixels = decoded
        .thumbnail(model::MAX_TEXTURE_SIDE, model::MAX_TEXTURE_SIDE)
        .into_rgba8();
    Ok(crate::native::media_sand::Pixels {
        width: pixels.width(),
        height: pixels.height(),
        rgba: pixels.into_raw(),
    })
}

fn package(directory: &Path, source: &Path) -> Result<String, String> {
    let pixels = decode(source)?;
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let destination = directory.join(&id);
    std::fs::create_dir(&destination).map_err(|error| error.to_string())?;
    let result = image::save_buffer_with_format(
        destination.join("source.png"),
        &pixels.rgba,
        pixels.width,
        pixels.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| error.to_string());
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(destination);
        return Err(error);
    }
    Ok(id)
}

pub(super) fn import(world: &mut World, owner: Entity, source: PathBuf) -> Result<(), String> {
    if world
        .get_resource::<Jobs>()
        .is_some_and(|jobs| jobs.import.is_some())
    {
        return Err("Wait for the current texture import.".into());
    }
    let directory = world
        .get_resource::<super::super::assets::AssetDirectory>()
        .ok_or("Asset storage is unavailable.")?
        .0
        .clone();
    let previous = world.get::<Material>(owner).map(|value| value.0.clone());
    let (sender, receiver) = mpsc::channel();
    let worker_directory = directory.clone();
    std::thread::Builder::new()
        .name("sand-texture-import".into())
        .spawn(move || {
            let _ = sender.send(package(&worker_directory, &source));
        })
        .map_err(|error| error.to_string())?;
    world.get_resource_or_init::<Jobs>().import = Some(Import {
        owner,
        directory,
        previous,
        receiver: Mutex::new(receiver),
    });
    Ok(())
}

pub(super) fn retry(world: &mut World, key: &model::Texture) {
    if let Some(mut jobs) = world.get_resource_mut::<Jobs>() {
        jobs.failed.remove(key);
    }
}

pub(super) fn retain(world: &mut World, used: &HashSet<model::Texture>) {
    if let Some(mut jobs) = world.get_resource_mut::<Jobs>() {
        jobs.failed.retain(|key, _| used.contains(key));
    }
}

pub(super) fn status(world: &World, key: Option<&model::Texture>) -> String {
    let Some(jobs) = world.get_resource::<Jobs>() else {
        return String::new();
    };
    if jobs.import.is_some() {
        return "Importing texture…".into();
    }
    key.and_then(|key| jobs.failed.get(key))
        .cloned()
        .unwrap_or_default()
}

fn upload(world: &mut World, pixels: crate::native::media_sand::Pixels) -> Handle<Image> {
    let mut image = Image::new(
        Extent3d {
            width: pixels.width,
            height: pixels.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..default()
    });
    world.resource_mut::<Assets<Image>>().add(image)
}

pub(super) fn poll(world: &mut World, textures: &mut HashMap<model::Texture, Handle<Image>>) {
    let mut jobs = world.remove_resource::<Jobs>().unwrap_or_default();
    jobs.pending.retain(
        |pending| match pending.receiver.lock().unwrap().try_recv() {
            Ok(Ok(pixels)) => {
                textures.insert(pending.key.clone(), upload(world, pixels));
                false
            }
            Ok(Err(error)) => {
                jobs.failed.insert(
                    pending.key.clone(),
                    format!("Texture unavailable: {error}. Using plain material; Retry to reload."),
                );
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                jobs.failed.insert(
                    pending.key.clone(),
                    "Texture loading stopped; Retry to reload.".into(),
                );
                false
            }
        },
    );
    let imported =
        jobs.import
            .as_ref()
            .and_then(|import| match import.receiver.lock().unwrap().try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Texture import stopped.".into()))
                }
            });
    if let Some(result) = imported {
        let import = jobs.import.take().unwrap();
        match result {
            Ok(id)
                if world.get_entity(import.owner).is_ok()
                    && world.get::<Material>(import.owner).map(|value| &value.0)
                        == import.previous.as_ref() =>
            {
                let mut material = import.previous.unwrap_or(model::Material {
                    lit: true,
                    ..default()
                });
                material.texture = model::Texture::Asset { id };
                world.entity_mut(import.owner).insert(Material(material));
            }
            Ok(id) => {
                let _ = std::fs::remove_dir_all(import.directory.join(id));
            }
            Err(error) => crate::native::notifications::report(world, "Sand texture", &error),
        }
    }
    if (!jobs.pending.is_empty() || jobs.import.is_some())
        && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
    {
        wake.after(std::time::Duration::from_millis(100));
    }
    world.insert_resource(jobs);
}

pub(super) fn get(
    world: &mut World,
    textures: &mut HashMap<model::Texture, Handle<Image>>,
    used: &mut HashSet<model::Texture>,
    kind: &model::Texture,
) -> Option<Handle<Image>> {
    if matches!(kind, model::Texture::Plain) || !kind.valid() {
        return None;
    }
    if !used.contains(kind) && used.len() >= model::MAX_TEXTURES {
        return None;
    }
    used.insert(kind.clone());
    if let model::Texture::Asset { id } = kind {
        if let Some(image) = textures.get(kind) {
            return Some(image.clone());
        }
        let directory = world
            .get_resource::<super::super::assets::AssetDirectory>()?
            .0
            .clone();
        let mut jobs = world.get_resource_or_init::<Jobs>();
        if jobs.pending.len() >= 2
            || jobs.failed.contains_key(kind)
            || jobs.pending.iter().any(|pending| &pending.key == kind)
        {
            return None;
        }
        let source = directory.join(id).join("source.png");
        let (sender, receiver) = mpsc::channel();
        if let Err(error) = std::thread::Builder::new()
            .name("sand-texture-load".into())
            .spawn(move || {
                let _ = sender.send(decode(&source));
            })
        {
            jobs.failed.insert(kind.clone(), error.to_string());
            return None;
        }
        jobs.pending.push(Pending {
            key: kind.clone(),
            receiver: Mutex::new(receiver),
        });
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(std::time::Duration::from_millis(100));
        }
        return None;
    }
    Some(
        textures
            .entry(kind.clone())
            .or_insert_with(|| {
                let mut pixels = Vec::with_capacity(64 * 64 * 4);
                for y in 0..64u32 {
                    for x in 0..64u32 {
                        let value = match kind {
                            model::Texture::Grain => {
                                180 + ((x.wrapping_mul(1973) ^ y.wrapping_mul(9277))
                                    .wrapping_mul(26699)
                                    % 76) as u8
                            }
                            model::Texture::Tiles => {
                                if x % 16 == 0 || y % 16 == 0 {
                                    100
                                } else {
                                    240
                                }
                            }
                            model::Texture::Brushed => 175 + ((y * 37 + x / 8) % 80) as u8,
                            _ => 255,
                        };
                        pixels.extend_from_slice(&[value, value, value, 255]);
                    }
                }
                let mut image = Image::new(
                    Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    pixels,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..default()
                });
                world.resource_mut::<Assets<Image>>().add(image)
            })
            .clone(),
    )
}
