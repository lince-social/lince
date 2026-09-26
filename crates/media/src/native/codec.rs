use std::ptr::NonNull;

use rav1d::include::dav1d::{
    data::Dav1dData,
    dav1d::{Dav1dContext, Dav1dSettings},
    headers::DAV1D_PIXEL_LAYOUT_I420,
    picture::Dav1dPicture,
};
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_get_picture,
    dav1d_open, dav1d_picture_unref, dav1d_send_data,
};
use rav1e::prelude::*;

use super::error;
use crate::{MediaError, Result, video::VideoFrame};

pub const MAX_PACKET: usize = 1_048_576;

pub struct VideoEncoder {
    context: Context<u8>,
    width: usize,
    height: usize,
}

impl VideoEncoder {
    pub fn new(width: usize, height: usize, bitrate: i32) -> Result<Self> {
        let mut encoder = EncoderConfig::with_speed_preset(10);
        encoder.width = width;
        encoder.height = height;
        encoder.low_latency = true;
        encoder.time_base = Rational::new(1, 10);
        encoder.min_key_frame_interval = 0;
        encoder.max_key_frame_interval = 20;
        encoder.bitrate = bitrate;
        encoder.quantizer = 110;
        encoder.speed_settings.rdo_lookahead_frames = 1;
        let context = Config::new()
            .with_encoder_config(encoder)
            .with_threads(1)
            .new_context()
            .map_err(error)?;
        Ok(Self {
            context,
            width,
            height,
        })
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn encode(&mut self, input: &VideoFrame, keyframe: bool) -> Result<Option<Vec<u8>>> {
        let mut frame = self.context.new_frame();
        for y in 0..self.height {
            for x in 0..self.width {
                let offset = ((y * input.height as usize / self.height) * input.width as usize
                    + x * input.width as usize / self.width)
                    * 4;
                let r = input.rgba[offset] as i32;
                let g = input.rgba[offset + 1] as i32;
                let b = input.rgba[offset + 2] as i32;
                let stride = frame.planes[0].cfg.stride;
                frame.planes[0].data[y * stride + x] =
                    ((66 * r + 129 * g + 25 * b + 128) / 256 + 16).clamp(0, 255) as u8;
                if x % 2 == 0 && y % 2 == 0 {
                    let stride = frame.planes[1].cfg.stride;
                    frame.planes[1].data[(y / 2) * stride + x / 2] =
                        ((-38 * r - 74 * g + 112 * b + 128) / 256 + 128).clamp(0, 255) as u8;
                    let stride = frame.planes[2].cfg.stride;
                    frame.planes[2].data[(y / 2) * stride + x / 2] =
                        ((112 * r - 94 * g - 18 * b + 128) / 256 + 128).clamp(0, 255) as u8;
                }
            }
        }
        let params = FrameParameters {
            frame_type_override: if keyframe {
                FrameTypeOverride::Key
            } else {
                FrameTypeOverride::No
            },
            ..Default::default()
        };
        self.context.send_frame((frame, params)).map_err(error)?;
        loop {
            match self.context.receive_packet() {
                Ok(packet) if packet.data.len() <= MAX_PACKET => return Ok(Some(packet.data)),
                Ok(_) => return Err(MediaError("Encoded video exceeded the frame limit".into())),
                Err(EncoderStatus::Encoded) => continue,
                Err(EncoderStatus::NeedMoreData) => return Ok(None),
                Err(err) => return Err(error(err)),
            }
        }
    }
}

pub struct VideoDecoder(Option<Dav1dContext>);

impl VideoDecoder {
    pub fn new() -> Result<Self> {
        let mut settings = std::mem::MaybeUninit::<Dav1dSettings>::uninit();
        unsafe {
            dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap());
        }
        let mut settings = unsafe { settings.assume_init() };
        settings.n_threads = 1;
        settings.max_frame_delay = 1;
        settings.apply_grain = 0;
        settings.frame_size_limit = 1920 * 1080;
        let mut context = None;
        let result = unsafe {
            dav1d_open(
                Some(NonNull::from(&mut context)),
                Some(NonNull::from(&mut settings)),
            )
        };
        if result.0 < 0 {
            return Err(MediaError("AV1 decoder initialization failed".into()));
        }
        Ok(Self(context))
    }

    pub fn decode(&mut self, packet: &[u8]) -> Result<Option<VideoFrame>> {
        if packet.is_empty() || packet.len() > MAX_PACKET {
            return Err(MediaError("Invalid AV1 frame size".into()));
        }
        let mut data = Dav1dData::default();
        let buffer = unsafe { dav1d_data_create(Some(NonNull::from(&mut data)), packet.len()) };
        if buffer.is_null() {
            return Err(MediaError("AV1 buffer allocation failed".into()));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(packet.as_ptr(), buffer, packet.len());
        }
        let sent = unsafe { dav1d_send_data(self.0, Some(NonNull::from(&mut data))) };
        unsafe {
            dav1d_data_unref(Some(NonNull::from(&mut data)));
        }
        if sent.0 < 0 {
            return Err(MediaError("Invalid AV1 packet".into()));
        }
        let mut picture = Dav1dPicture::default();
        let got = unsafe { dav1d_get_picture(self.0, Some(NonNull::from(&mut picture))) };
        if got.0 < 0 {
            return Ok(None);
        }
        let result = Self::frame(&picture);
        unsafe {
            dav1d_picture_unref(Some(NonNull::from(&mut picture)));
        }
        result.map(Some)
    }

    fn frame(picture: &Dav1dPicture) -> Result<VideoFrame> {
        let p = &picture.p;
        if p.w <= 0
            || p.h <= 0
            || p.w > 1920
            || p.h > 1080
            || p.bpc != 8
            || p.layout != DAV1D_PIXEL_LAYOUT_I420
        {
            return Err(MediaError("Unsupported AV1 frame format".into()));
        }
        let width = p.w as usize;
        let height = p.h as usize;
        let mut planes = Vec::with_capacity(3);
        for index in 0..3 {
            let rows = if index == 0 {
                height
            } else {
                height.div_ceil(2)
            };
            let columns = if index == 0 { width } else { width.div_ceil(2) };
            let stride = picture.stride[usize::from(index > 0)];
            if stride < columns as isize || stride > 65536 {
                return Err(MediaError("Invalid decoded video stride".into()));
            }
            let ptr = picture.data[index].ok_or_else(|| MediaError("Missing AV1 plane".into()))?;
            let mut plane = Vec::with_capacity(rows * columns);
            for row in 0..rows {
                let row = unsafe {
                    std::slice::from_raw_parts(
                        ptr.as_ptr().cast::<u8>().add(row * stride as usize),
                        columns,
                    )
                };
                plane.extend_from_slice(row);
            }
            planes.push(plane);
        }
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let l = planes[0][y * width + x] as i32 - 16;
                let c = (y / 2) * width.div_ceil(2) + x / 2;
                let u = planes[1][c] as i32 - 128;
                let v = planes[2][c] as i32 - 128;
                rgba.extend_from_slice(&[
                    ((298 * l + 409 * v + 128) >> 8).clamp(0, 255) as u8,
                    ((298 * l - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8,
                    ((298 * l + 516 * u + 128) >> 8).clamp(0, 255) as u8,
                    255,
                ]);
            }
        }
        VideoFrame::new(width as u32, height as u32, rgba)
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        unsafe {
            dav1d_close(Some(NonNull::from(&mut self.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_decoder_rejects_oversized_and_invalid_packets() {
        let mut decoder = VideoDecoder::new().unwrap();
        assert!(decoder.decode(&[]).is_err());
        assert!(decoder.decode(&vec![0; MAX_PACKET + 1]).is_err());
        assert!(decoder.decode(&[255; 64]).is_err());
    }
}
