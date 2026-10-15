use super::*;
use crate::server::ClientBuffer;
use smithay::{
    backend::allocator::{Buffer, Fourcc, Modifier},
    reexports::wayland_server::protocol::wl_shm::Format,
    wayland::{dmabuf::get_dmabuf, shm::with_buffer_contents},
};

struct Content {
    device: wgpu::Device,
    texture: wgpu::Texture,
    opaque: bool,
}

impl VulkanRenderer {
    pub(crate) fn import_client(
        &self,
        buffer: &ClientBuffer,
        target: Option<&wgpu::Texture>,
    ) -> Result<Option<(wgpu::Texture, bool)>> {
        if let Some(content) = buffer.cached::<Content>() {
            if content.device != self.device {
                return Err(ProbeFailure::at(
                    "client_import",
                    "the cached client content belongs to another render device",
                ));
            }
            return Ok(Some((content.texture.clone(), content.opaque)));
        }
        if buffer.is_released() {
            return Err(ProbeFailure::at(
                "client_import",
                "the client buffer was released without retaining its pixels",
            ));
        }
        if let Ok(dmabuf) = get_dmabuf(buffer.buffer()) {
            if dmabuf.format().code != Fourcc::Argb8888
                || dmabuf.format().modifier != Modifier::Linear
                || dmabuf.num_planes() != 1
            {
                return Err(ProbeFailure::at(
                    "client_import",
                    "unsupported DMA-BUF format",
                ));
            }
            for fd in dmabuf.handles() {
                let mut poll = [rustix::event::PollFd::new(
                    &fd,
                    rustix::event::PollFlags::IN,
                )];
                rustix::event::poll(
                    &mut poll,
                    Some(&rustix::time::Timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    }),
                )
                .map_err(|error| ProbeFailure::at("client_acquire", error))?;
                if !poll[0].revents().contains(rustix::event::PollFlags::IN) {
                    return Ok(None);
                }
            }
            let texture = self.import_dmabuf_into(dmabuf, target)?;
            buffer.cache(Content {
                device: self.device.clone(),
                texture: texture.clone(),
                opaque: false,
            });
            return Ok(Some((texture, false)));
        }
        let (pixels, size, opaque) =
            with_buffer_contents(buffer.buffer(), |pointer, length, data| {
                if data.width <= 0
                    || data.height <= 0
                    || data.width > 16384
                    || data.height > 16384
                    || data.offset < 0
                    || data.stride < data.width * 4
                    || !matches!(data.format, Format::Argb8888 | Format::Xrgb8888)
                {
                    return Err(ProbeFailure::at(
                        "client_shm",
                        "unsupported SHM geometry or format",
                    ));
                }
                let row = data.width as usize * 4;
                let bytes = row
                    .checked_mul(data.height as usize)
                    .filter(|bytes| *bytes <= 256 * 1024 * 1024)
                    .ok_or_else(|| {
                        ProbeFailure::at("client_shm", "client buffer exceeds the byte limit")
                    })?;
                let end = (data.stride as usize)
                    .checked_mul(data.height as usize - 1)
                    .and_then(|value| value.checked_add(data.offset as usize))
                    .and_then(|value| value.checked_add(row))
                    .ok_or_else(|| ProbeFailure::at("client_shm", "SHM dimensions overflow"))?;
                if end > length {
                    return Err(ProbeFailure::at(
                        "client_shm",
                        "SHM contents exceed their pool",
                    ));
                }
                let mut pixels = vec![0; bytes];
                for y in 0..data.height as usize {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            pointer.add(data.offset as usize + y * data.stride as usize),
                            pixels.as_mut_ptr().add(y * row),
                            row,
                        )
                    };
                }
                Ok((
                    pixels,
                    [data.width as u32, data.height as u32],
                    data.format == Format::Xrgb8888,
                ))
            })
            .map_err(|error| ProbeFailure::at("client_shm", error))??;
        let texture = target
            .filter(|texture| {
                texture.width() == size[0]
                    && texture.height() == size[1]
                    && texture.format() == wgpu::TextureFormat::Bgra8Unorm
                    && texture.usage().contains(wgpu::TextureUsages::COPY_DST)
            })
            .cloned()
            .unwrap_or_else(|| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("park_client_upload"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            });
        self.queue.write_texture(
            texture.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 4),
                rows_per_image: Some(size[1]),
            },
            texture.size(),
        );
        buffer.cache(Content {
            device: self.device.clone(),
            texture: texture.clone(),
            opaque,
        });
        Ok(Some((texture, opaque)))
    }
}
