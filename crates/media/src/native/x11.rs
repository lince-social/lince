use x11rb::{
    connection::Connection,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{ConnectionExt as _, ImageFormat, ImageOrder},
    },
    rust_connection::RustConnection,
};

use super::error;
use crate::{
    MediaError, Result,
    video::{MAX_HEIGHT, MAX_WIDTH, VideoFrame},
};

struct Monitor {
    id: u64,
    label: String,
    root: u32,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    masks: [u32; 3],
}

pub fn available() -> bool {
    std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

fn monitors(connection: &RustConnection) -> Result<Vec<Monitor>> {
    let mut monitors = Vec::new();
    for screen in &connection.setup().roots {
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| &depth.visuals)
            .find(|visual| visual.visual_id == screen.root_visual)
            .ok_or_else(|| MediaError("Screen pixel format is unavailable".into()))?;
        let masks = [visual.red_mask, visual.green_mask, visual.blue_mask];
        if masks.contains(&0) {
            return Err(MediaError("Unsupported indexed screen pixel format".into()));
        }
        let listed = connection
            .randr_get_monitors(screen.root, true)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        if let Some(listed) = listed.filter(|reply| !reply.monitors.is_empty()) {
            for monitor in listed.monitors.into_iter().take(32) {
                let name = connection
                    .get_atom_name(monitor.name)
                    .map_err(error)?
                    .reply()
                    .map_err(error)?
                    .name;
                monitors.push(Monitor {
                    id: (screen.root as u64) << 32 | monitor.name as u64,
                    label: String::from_utf8_lossy(&name).into_owned(),
                    root: screen.root,
                    x: monitor.x,
                    y: monitor.y,
                    width: monitor.width,
                    height: monitor.height,
                    masks,
                });
            }
        } else {
            monitors.push(Monitor {
                id: (screen.root as u64) << 32,
                label: format!("Screen {}", monitors.len() + 1),
                root: screen.root,
                x: 0,
                y: 0,
                width: screen.width_in_pixels,
                height: screen.height_in_pixels,
                masks,
            });
        }
    }
    Ok(monitors)
}

pub fn screens() -> Result<Vec<(u64, String)>> {
    let (connection, _) = x11rb::connect(None).map_err(error)?;
    Ok(monitors(&connection)?
        .into_iter()
        .map(|monitor| (monitor.id, monitor.label))
        .collect())
}

pub struct Capture {
    connection: RustConnection,
    monitor: Monitor,
}

impl Capture {
    pub fn start(id: Option<u64>) -> Result<Self> {
        let (connection, _) = x11rb::connect(None).map_err(error)?;
        let monitor = monitors(&connection)?
            .into_iter()
            .find(|monitor| Some(monitor.id) == id)
            .ok_or_else(|| MediaError("Choose an available screen".into()))?;
        if monitor.width == 0
            || monitor.height == 0
            || monitor.width > 16384
            || monitor.height > 16384
        {
            return Err(MediaError("Unsupported screen size".into()));
        }
        Ok(Self {
            connection,
            monitor,
        })
    }

    pub fn frame(&self) -> Result<VideoFrame> {
        let monitor = &self.monitor;
        let image = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                monitor.root,
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
                u32::MAX,
            )
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let setup = self.connection.setup();
        let format = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == image.depth)
            .ok_or_else(|| MediaError("Unsupported screen format".into()))?;
        let bytes = format.bits_per_pixel as usize / 8;
        if !(2..=4).contains(&bytes) || format.scanline_pad == 0 {
            return Err(MediaError("Unsupported screen pixel format".into()));
        }
        let stride = (monitor.width as usize * format.bits_per_pixel as usize)
            .div_ceil(format.scanline_pad as usize)
            * format.scanline_pad as usize
            / 8;
        if image.data.len() < stride * monitor.height as usize {
            return Err(MediaError("Incomplete screen frame".into()));
        }
        let ratio = (MAX_WIDTH as f64 / monitor.width as f64)
            .min(MAX_HEIGHT as f64 / monitor.height as f64)
            .min(1.0);
        let width = (monitor.width as f64 * ratio).max(1.0) as u32;
        let height = (monitor.height as f64 * ratio).max(1.0) as u32;
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * monitor.height as usize / height as usize * stride
                    + x * monitor.width as usize / width as usize * bytes;
                let mut pixel = 0u32;
                for index in 0..bytes {
                    let shift = if setup.image_byte_order == ImageOrder::LSB_FIRST {
                        index
                    } else {
                        bytes - 1 - index
                    } * 8;
                    pixel |= (image.data[offset + index] as u32) << shift;
                }
                for mask in monitor.masks {
                    let shift = mask.trailing_zeros();
                    rgba.push(
                        (((pixel & mask) >> shift) as u64 * 255 / (mask >> shift) as u64) as u8,
                    );
                }
                rgba.push(255);
            }
        }
        VideoFrame::new(width, height, rgba)
    }
}
