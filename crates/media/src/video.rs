use std::sync::{Arc, Mutex};

use crate::{MediaError, Result};

pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;

#[derive(Clone)]
pub struct VideoFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

impl VideoFrame {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        if width == 0
            || height == 0
            || width > MAX_WIDTH
            || height > MAX_HEIGHT
            || rgba.len() != width as usize * height as usize * 4
        {
            return Err(MediaError("Invalid video frame size".into()));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

#[derive(Clone, Default)]
pub struct LatestVideo(Arc<Mutex<Option<VideoFrame>>>);

impl LatestVideo {
    pub fn publish(&self, frame: VideoFrame) {
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(frame);
    }

    pub fn take(&self) -> Option<VideoFrame> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    pub fn clear(&self) {
        self.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_renderers_only_retain_the_latest_frame() {
        let frames = LatestVideo::default();
        for value in 0..255 {
            frames.publish(VideoFrame::new(1, 1, vec![value; 4]).unwrap());
        }
        assert_eq!(frames.take().unwrap().rgba, vec![254; 4]);
        assert!(frames.take().is_none());
        frames.publish(VideoFrame::new(1, 1, vec![1; 4]).unwrap());
        frames.clear();
        assert!(frames.take().is_none());
    }

    #[test]
    fn reject_invalid_or_unbounded_video_dimensions() {
        assert!(VideoFrame::new(u32::MAX, u32::MAX, vec![]).is_err());
        assert!(VideoFrame::new(0, 1, vec![]).is_err());
        assert!(VideoFrame::new(2, 2, vec![0; 15]).is_err());
    }
}
