use super::*;
use base64::Engine;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const MAX_BYTES: usize = 8 * 1024 * 1024;
type Pixels = Result<(u32, u32, Vec<u8>), String>;

#[derive(Component)]
struct Picture {
    source: String,
    label: Entity,
}

#[derive(Resource, Default)]
struct Downloads {
    active: HashMap<String, Arc<Mutex<mpsc::Receiver<Pixels>>>>,
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str, alt: &str) {
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
            crate::icons::Tooltip(alt.into()),
        ))
        .id();
    let label = crate::edit_mode::label(
        world,
        owner,
        if alt.is_empty() {
            "Loading image…"
        } else {
            alt
        },
        14.0,
    );
    world.entity_mut(owner).insert(Picture {
        source: source.into(),
        label,
    });
}

fn url(source: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(source).map_err(|_| "Invalid image URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Images need an HTTP(S) URL or an embedded image.".into());
    }
    Ok(url)
}

fn decode(bytes: Vec<u8>) -> Pixels {
    if bytes.len() > MAX_BYTES {
        return Err("Image exceeds 8 MB.".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    Ok((image.width(), image.height(), image.into_raw()))
}

async fn load(source: &str) -> Pixels {
    let bytes = if let Some(data) = source.strip_prefix("data:") {
        let (format, payload) = data.split_once(',').ok_or("Invalid embedded image")?;
        if !matches!(
            format,
            "image/png;base64" | "image/jpeg;base64" | "image/gif;base64" | "image/webp;base64"
        ) || payload.len() > MAX_BYTES * 4 / 3 + 4
        {
            return Err("Unsupported or oversized embedded image.".into());
        }
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|e| e.to_string())?
    } else {
        let requested_url = url(source)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || url(attempt.url().as_str()).is_err() {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client
            .get(requested_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
        {
            return Err("Image exceeds 8 MB.".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
                return Err("Image exceeds 8 MB.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        bytes
    };
    decode(bytes)
}

pub(super) fn update(world: &mut World) {
    world.init_resource::<Downloads>();
    let completed: Vec<_> = world
        .resource::<Downloads>()
        .active
        .iter()
        .filter_map(
            |(source, receiver)| match receiver.lock().ok()?.try_recv() {
                Ok(result) => Some((source.clone(), result)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((source.clone(), Err("Image loader stopped.".into())))
                }
                Err(_) => None,
            },
        )
        .collect();
    for (source, result) in completed {
        world.resource_mut::<Downloads>().active.remove(&source);
        let targets: Vec<_> = world
            .query::<(Entity, &Picture)>()
            .iter(world)
            .filter(|(_, image)| image.source == source)
            .map(|(entity, image)| (entity, image.label))
            .collect();
        let image = result.as_ref().ok().map(|(width, height, pixels)| {
            world.init_resource::<Assets<Image>>();
            world.resource_mut::<Assets<Image>>().add(Image::new(
                bevy::render::render_resource::Extent3d {
                    width: *width,
                    height: *height,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                pixels.clone(),
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            ))
        });
        for (entity, label) in targets {
            world.entity_mut(entity).remove::<Picture>();
            if let Some(image) = image.clone() {
                world.despawn(label);
                world.spawn((
                    ImageNode::new(image),
                    Node {
                        width: px(result.as_ref().unwrap().0 as f32),
                        max_width: percent(100),
                        height: Val::Auto,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(entity),
                ));
            } else if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = format!(
                    "Image could not be loaded: {}",
                    result.as_ref().unwrap_err()
                );
            }
        }
    }
    let capacity = 4_usize.saturating_sub(world.resource::<Downloads>().active.len());
    let sources: std::collections::HashSet<_> = world
        .query::<&Picture>()
        .iter(world)
        .filter(|image| {
            !world
                .resource::<Downloads>()
                .active
                .contains_key(&image.source)
        })
        .take(capacity)
        .map(|image| image.source.clone())
        .collect();
    for source in sources {
        let (sender, receiver) = mpsc::channel();
        world
            .resource_mut::<Downloads>()
            .active
            .insert(source.clone(), Arc::new(Mutex::new(receiver)));
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|runtime| runtime.block_on(load(&source)));
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_sources_exclude_files_scripts_and_credentials() {
        for source in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:secret@example.com/a.png",
            "ftp://example.com/a.png",
        ] {
            assert!(url(source).is_err());
        }
        assert!(url("https://example.com/picture.png").is_ok());
        assert!(decode(vec![0; MAX_BYTES + 1]).is_err());
        assert!(decode(b"not an image".to_vec()).is_err());
        let (width, height, pixels) =
            decode(include_bytes!("../../../../assets/logo/black_in_white.png").to_vec()).unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
    }

    #[test]
    fn embedded_images_render_and_markdown_preserves_image_urls_and_captions() {
        let blocks = crate::description::markup::parse(
            "# Message\n\n![A diagram](https://example.com/a.png)\n\nAfter the image.",
        );
        assert_eq!(blocks[0].heading, 1);
        assert_eq!(
            blocks[1].image.as_deref(),
            Some("https://example.com/a.png")
        );
        assert_eq!(blocks[1].runs[0].text, "A diagram");
        let source = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode(include_bytes!("../../../../assets/logo/black_in_white.png"))
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (width, height, pixels) = runtime.block_on(load(&source)).unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert!(
            runtime
                .block_on(load("data:text/html;base64,PHNjcmlwdD4="))
                .is_err()
        );
    }
}
