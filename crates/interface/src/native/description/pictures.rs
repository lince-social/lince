use super::raster::{self, Pixels};
use super::*;
use base64::Engine;
use std::sync::Arc;

const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Component)]
struct Picture {
    source: Arc<str>,
    label: Entity,
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str, alt: &str) {
    let frame = crate::native::embedded_media::frame(
        world,
        parent,
        if alt.is_empty() { "Image" } else { alt },
    );
    let owner = frame.owner;
    crate::native::edit_mode::label(world, frame.controls, source, 12.0);
    let label = crate::native::edit_mode::label(
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

#[cfg(test)]
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

pub(super) fn decode(bytes: Vec<u8>) -> Pixels {
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

fn load(source: &str) -> Pixels {
    let _load = nucleus::performance::Span::new("description-picture-load");
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
        let uri = lince_resources::address(source)?;
        if uri.scheme() == "file" {
            let path = uri
                .to_file_path()
                .map_err(|_| "Invalid local image address")?;
            let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(
                &mut std::io::Read::take(file, MAX_BYTES as u64 + 1),
                &mut bytes,
            )
            .map_err(|error| error.to_string())?;
            bytes
        } else {
            let library = engine::sources::library().map_err(|error| error.to_string())?;
            let file = library.http_file(source, Default::default(), MAX_BYTES as u64, false)?;
            std::fs::read(file.path).map_err(|error| error.to_string())?
        }
    };
    decode(bytes)
}

impl raster::Request for Picture {
    const CAPACITY: usize = 4;
    const STOPPED: &'static str = "Image loader stopped.";

    fn source(&self) -> &Arc<str> {
        &self.source
    }

    fn label(&self) -> Entity {
        self.label
    }

    fn render(source: &str) -> Pixels {
        load(source)
    }

    fn layout(width: u32) -> Node {
        Node {
            width: px(width as f32),
            max_width: percent(100),
            height: Val::Auto,
            flex_shrink: 0.0,
            ..default()
        }
    }

    fn failure(_: &str, error: &str) -> String {
        format!("Image could not be loaded: {error}")
    }
}

pub(super) fn update(world: &mut World) {
    raster::update::<Picture>(world);
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
        let (width, height, pixels) = decode(
            include_bytes!("../../../../../institute/assets/logo/black_in_white.png").to_vec(),
        )
        .unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
    }

    #[test]
    fn embedded_images_render_and_markdown_preserves_image_urls_and_captions() {
        let blocks = crate::native::description::markup::parse(
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
            base64::engine::general_purpose::STANDARD.encode(include_bytes!(
                "../../../../../institute/assets/logo/black_in_white.png"
            ))
        );
        let (width, height, pixels) = load(&source).unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert!(load("data:text/html;base64,PHNjcmlwdD4=").is_err());
    }
}
