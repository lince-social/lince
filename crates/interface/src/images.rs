//! Bounded PNG and JPEG previews from embedded or HTTP(S) sources.

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader, Limits};
use reqwest::{Client, Url, redirect::Policy};
use std::{io::Cursor, time::Duration};

/// Maximum encoded file size before decoding.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Maximum input dimensions during decode.
pub const MAX_DIMENSION: u32 = 2048;
const MAX_DECODE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PREVIEW_DIMENSION: u32 = 1024;
const MAX_REDIRECTS: usize = 3;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);

/// Row-major RGBA8 image data and its preview dimensions.
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn allowed(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
}

fn url(source: &str) -> Result<Url, String> {
    let url = Url::parse(source).map_err(|_| "Invalid image URL")?;
    allowed(&url).then_some(url).ok_or_else(|| {
        "Images need an HTTP(S) URL without credentials, or embedded PNG/JPEG data".into()
    })
}

/// Decodes a supported image and scales it to fit within the preview dimensions.
///
/// # Errors
///
/// Returns a display message for an unsupported, malformed, or oversized image.
pub fn decode(bytes: Vec<u8>) -> Result<Pixels, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Image exceeds 4 MiB".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
        return Err("Use a PNG or JPEG image".into());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("Image could not be decoded: {error}"))?
        .thumbnail(MAX_PREVIEW_DIMENSION, MAX_PREVIEW_DIMENSION)
        .into_rgba8();
    Ok(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// Loads a bounded image preview from an embedded data URL or an HTTP(S) URL.
///
/// # Errors
///
/// Returns a display message for invalid sources, network errors, or decode failures.
pub async fn load(source: &str) -> Result<Pixels, String> {
    let bytes = if let Some(data) = source.strip_prefix("data:") {
        embedded_bytes(data)?
    } else {
        download(url(source)?).await?
    };
    decode(bytes)
}

fn embedded_bytes(data: &str) -> Result<Vec<u8>, String> {
    let (format, payload) = data.split_once(',').ok_or("Invalid embedded image")?;
    if !matches!(format, "image/png;base64" | "image/jpeg;base64")
        || payload.len() > MAX_BYTES.div_ceil(3) * 4
    {
        return Err("Embedded images must be PNG/JPEG and at most 4 MiB".into());
    }
    STANDARD
        .decode(payload)
        .map_err(|_| "Invalid embedded image encoding".into())
}

async fn download(source: Url) -> Result<Vec<u8>, String> {
    let client = Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS || !allowed(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(source)
        .send()
        .await
        .map_err(|_| "Could not download image")?
        .error_for_status()
        .map_err(|_| "The image server returned an error")?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BYTES as u64)
    {
        return Err("Image exceeds 4 MiB".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Image download was interrupted")?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err("Image exceeds 4 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_sources_and_excessive_images() {
        for source in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:pass@example.com/a.png",
            "ftp://example.com/a.png",
        ] {
            assert!(url(source).is_err());
        }
        assert!(url("https://example.com/a.png").is_ok());
        assert!(decode(vec![0; MAX_BYTES + 1]).is_err());
        assert!(decode(b"not an image".to_vec()).is_err());
        let image = image::RgbaImage::new(MAX_DIMENSION + 1, 1);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        assert!(decode(bytes.into_inner()).is_err());
    }

    #[test]
    fn decodes_and_bounds_display_pixels() {
        let image = image::RgbaImage::new(2048, 1024);
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        let pixels = decode(bytes.into_inner()).unwrap();
        assert_eq!((pixels.width, pixels.height), (1024, 512));
        assert_eq!(pixels.rgba.len(), 1024 * 512 * 4);
    }
}
