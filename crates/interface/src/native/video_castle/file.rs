use super::*;
use image::{AnimationDecoder, ImageDecoder};
use std::io::BufReader;

struct Take {
    frames: Vec<(u64, image::RgbaImage)>,
    duration: u64,
}

fn read(path: &str) -> Result<Take, String> {
    let input = crate::native::media_sand::open_file(std::path::Path::new(path), 32 * 1024 * 1024)?;
    let mut decoder = image::codecs::gif::GifDecoder::new(BufReader::new(input))
        .map_err(|_| "Choose a GIF video file")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1920);
    limits.max_image_height = Some(1080);
    limits.max_alloc = Some(16 * 1024 * 1024);
    decoder
        .set_limits(limits)
        .map_err(|_| "Video dimensions exceed 1920 × 1080")?;
    let mut frames = Vec::new();
    let mut bytes = 0;
    let mut duration = 0;
    for frame in decoder.into_frames() {
        let frame = frame.map_err(|_| "Cannot decode GIF video")?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = (u64::from(numerator) / u64::from(denominator.max(1))).max(20);
        let image = frame.into_buffer();
        bytes += image.as_raw().len();
        if frames.len() >= 600 || bytes > 64 * 1024 * 1024 {
            return Err("Video exceeds 600 frames or 64 MiB decoded".into());
        }
        frames.push((duration, image));
        duration += delay;
    }
    if frames.is_empty() {
        return Err("Video has no frames".into());
    }
    Ok(Take { frames, duration })
}

pub(super) async fn play(
    settings: &Settings,
    output: &Reply,
    wake: &Option<crate::native::wake::WakeSignal>,
) -> Result<(), String> {
    let path = settings.source.clone();
    let take = tokio::task::spawn_blocking(move || read(&path))
        .await
        .map_err(|error| error.to_string())??;
    let start = settings.position_ms.min(take.duration);
    let clock = tokio::time::Instant::now();
    let first = take
        .frames
        .iter()
        .rposition(|(at, _)| *at <= start)
        .unwrap_or(0);
    for (index, (at, image)) in take.frames.iter().enumerate().skip(first) {
        if *at > start {
            tokio::time::sleep_until(clock + Duration::from_millis(*at - start)).await;
        }
        publish(
            output,
            Frame {
                pixels: Some(crate::native::media_sand::Pixels {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.as_raw().clone(),
                }),
                status: format!("Video · frame {} / {}", index + 1, take.frames.len()),
                position_ms: Some(*at),
                skipped: 0,
            },
            wake,
        );
    }
    tokio::time::sleep_until(clock + Duration::from_millis(take.duration - start)).await;
    publish(
        output,
        Frame {
            pixels: None,
            status: "End of video · seek to 0 to replay".into(),
            position_ms: Some(take.duration),
            skipped: 0,
        },
        wake,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(width: u32, height: u32) -> (tempfile::TempDir, String) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video.gif");
        let mut encoder =
            image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        encoder
            .encode_frames(
                [image::Rgba([255, 0, 0, 255]), image::Rgba([0, 255, 0, 255])]
                    .into_iter()
                    .map(|color| {
                        image::Frame::from_parts(
                            image::RgbaImage::from_pixel(width, height, color),
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(100, 1),
                        )
                    }),
            )
            .unwrap();
        (directory, path.to_string_lossy().into_owned())
    }

    #[tokio::test]
    async fn gif_playback_seeks_to_the_right_frame_and_reports_end_position() {
        let (_directory, path) = fixture(1, 1);
        let output: Reply = Default::default();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let sampled = observed.clone();
        let frames = output.clone();
        let wake = Some(crate::native::wake::WakeSignal::new(move || {
            if let Some(frame) = frames.lock().unwrap().as_ref()
                && let Some(pixels) = &frame.pixels
            {
                sampled.lock().unwrap().push(pixels.rgba.clone());
            }
        }));
        play(
            &Settings {
                source: path,
                file: true,
                paused: false,
                position_ms: 100,
                ..default()
            },
            &output,
            &wake,
        )
        .await
        .unwrap();
        assert_eq!(*observed.lock().unwrap(), vec![vec![0, 255, 0, 255]]);
        assert_eq!(
            output.lock().unwrap().as_ref().unwrap().position_ms,
            Some(200)
        );
    }

    #[test]
    fn oversized_video_dimensions_are_rejected_before_collecting_frames() {
        let (_directory, path) = fixture(1, 1081);
        assert!(read(&path).is_err());
    }
}
