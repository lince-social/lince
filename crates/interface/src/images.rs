use base64::Engine;

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 2048;

pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn url(source: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(source).map_err(|_| "Invalid image URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(
            "Images need an HTTP(S) URL without credentials, or embedded PNG/JPEG data".into(),
        );
    }
    Ok(url)
}

pub fn decode(bytes: Vec<u8>) -> Result<Pixels, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Image exceeds 4 MiB".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err("Use a PNG or JPEG image".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("Image could not be decoded: {error}"))?
        .thumbnail(1024, 1024)
        .into_rgba8();
    Ok(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

pub async fn load(source: &str) -> Result<Pixels, String> {
    let bytes = if let Some(data) = source.strip_prefix("data:") {
        let (format, payload) = data.split_once(',').ok_or("Invalid embedded image")?;
        if !matches!(format, "image/png;base64" | "image/jpeg;base64")
            || payload.len() > MAX_BYTES.div_ceil(3) * 4
        {
            return Err("Embedded images must be PNG/JPEG and at most 4 MiB".into());
        }
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|_| "Invalid embedded image encoding")?
    } else {
        let source = url(source)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 || url(attempt.url().as_str()).is_err() {
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
        bytes
    };
    decode(bytes)
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
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        assert!(decode(bytes.into_inner()).is_err());
    }

    #[test]
    fn decodes_and_bounds_display_pixels() {
        let image = image::RgbaImage::new(2048, 1024);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let pixels = decode(bytes.into_inner()).unwrap();
        assert_eq!((pixels.width, pixels.height), (1024, 512));
        assert_eq!(pixels.rgba.len(), 1024 * 512 * 4);
    }
}
