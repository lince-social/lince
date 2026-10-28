use crate::{Entry, Library, MediaInput, Resource, Result, fingerprint};
use std::process::{Command, Stdio};

pub fn preview(library: &Library, resource: &Resource, width: u32, height: u32) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err("Choose a bounded video preview size".into());
    }
    let retained = retained(library, resource)?;
    let server = if retained.is_none() {
        Some(MediaInput::open(library, resource)?)
    } else {
        None
    };
    let input = retained
        .as_ref()
        .map(|entry| entry.path.to_string_lossy().into_owned())
        .unwrap_or_else(|| server.as_ref().unwrap().uri.clone());
    let start = if retained.is_some() {
        0
    } else {
        resource.locator.start_ms.unwrap_or(0)
    };
    let output=Command::new("ffmpeg").args(["-nostdin","-hide_banner","-loglevel","error","-rw_timeout","10000000","-ss"])
        .arg(format!("{:.3}",start as f64/1000.0)).args(["-i",&input,"-an","-frames:v","1","-vf"])
        .arg(format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"))
        .args(["-pix_fmt","rgba","-f","rawvideo","pipe:1"]).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|error|format!("FFmpeg video preview: {error}"))?;
    if !output.status.success() || output.stdout.len() != width as usize * height as usize * 4 {
        return Err("The referenced video frame is unavailable".into());
    }
    Ok(output.stdout)
}

fn key(resource: &Resource, version: &str, start: u64, end: u64) -> String {
    format!("{}{version}", prefix(resource, start, end))
}

fn prefix(resource: &Resource, start: u64, end: u64) -> String {
    format!(
        "lince-segment:{}:",
        fingerprint(
            format!(
                "{}:{}:{start}:{end}",
                resource.uri,
                resource.hash.as_deref().unwrap_or("")
            )
            .as_bytes()
        )
    )
}

pub fn cached(library: &Library, resource: &Resource) -> Result<Option<Entry>> {
    resource.locator.end_ms.map_or(Ok(None), |end| {
        library.latest(&prefix(
            resource,
            resource.locator.start_ms.unwrap_or(0),
            end,
        ))
    })
}

pub fn retained(library: &Library, resource: &Resource) -> Result<Option<Entry>> {
    let Some(end) = resource.locator.end_ms else {
        return Ok(None);
    };
    let version = match MediaInput::open(library, resource) {
        Ok(input) => input.version,
        Err(_) => return cached(library, resource),
    };
    library.find(&Resource {
        uri: key(
            resource,
            &version,
            resource.locator.start_ms.unwrap_or(0),
            end,
        ),
        ..Default::default()
    })
}

pub fn retain(library: &Library, resource: &Resource) -> Result<Entry> {
    resource.validate()?;
    let end = resource
        .locator
        .end_ms
        .ok_or("Choose the end of the interval to keep")?;
    let start = resource.locator.start_ms.unwrap_or(0);
    if let Some(entry) = retained(library, resource)? {
        return Ok(entry);
    }
    let server = MediaInput::open(library, resource)?;
    let identity = key(resource, &server.version, start, end);
    let directory = library.cache().join("video");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-max_alloc",
            "268435456",
            "-rw_timeout",
            "10000000",
            "-ss",
        ])
        .arg(format!("{:.3}", start as f64 / 1000.0))
        .args(["-i", &server.uri, "-t"])
        .arg(format!("{:.3}", (end - start) as f64 / 1000.0))
        .args([
            "-map", "0:v:0", "-map", "0:a?", "-c:v", "mpeg4", "-q:v", "4", "-c:a", "aac", "-f",
            "matroska",
        ])
        .arg(temporary.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("FFmpeg is required for interval retention: {error}"))?;
    if !status.success() {
        return Err("The source could not provide this video interval. Keep the complete source or check the selected interval.".into());
    }
    library.register(temporary.path(), &identity, "video/x-matroska", None)
}
