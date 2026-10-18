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
    pub(crate) fn imports_ready(&self) -> bool {
        self.waits.borrow().values().any(crate::graphics::wait::Watch::ready)
    }

    pub(crate) fn retain_imports(&self, windows: &[crate::server::WindowSnapshot]) {
        let live: std::collections::HashSet<_> = windows.iter().flat_map(|window| &window.nodes)
            .map(|node| (node.content.buffer().clone(), node.content.version)).collect();
        self.waits.borrow_mut().retain(|key, _| live.contains(key));
    }

    pub(crate) fn import_surface(
        &self,
        node: &crate::server::SurfaceNode,
        previous: Option<(&wgpu::Texture, smithay::backend::renderer::utils::CommitCounter)>,
    ) -> Result<Option<(wgpu::Texture, bool)>> {
        let buffer: &ClientBuffer = &node.content;
        let damage = node.buffer_damage_since(previous.map(|(_, commit)| commit));
        let target = previous.map(|(texture, _)| texture);
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
                    let key = (buffer.buffer().clone(), buffer.version);
                    let mut waits = self.waits.borrow_mut();
                    if !waits.contains_key(&key) {
                        let watch = self.reactor.descriptor(fd.try_clone_to_owned().map_err(|error| ProbeFailure::at("client_acquire_events", error))?)
                            .map_err(|error| ProbeFailure::at("client_acquire_events", error))?;
                        waits.insert(key, watch);
                    }
                    return Ok(None);
                }
            }
            self.waits.borrow_mut().remove(&(buffer.buffer().clone(), buffer.version));
            let texture = self.import_dmabuf_regions(dmabuf, target, Some(&damage))?;
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
        let previous = target
            .filter(|texture| {
                texture.width() == size[0]
                    && texture.height() == size[1]
                    && texture.format() == wgpu::TextureFormat::Bgra8Unorm
                    && texture.usage().contains(wgpu::TextureUsages::COPY_SRC)
            });
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
                });
        let bounds = smithay::utils::Rectangle::from_size((size[0] as i32, size[1] as i32).into());
        let regions = if let Some(previous) = previous {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_texture(previous.as_image_copy(), texture.as_image_copy(), texture.size());
            self.queue.submit([encoder.finish()]);
            damage.into_iter().filter_map(|damage| damage.intersection(bounds)).collect::<Vec<_>>()
        } else { vec![bounds] };
        for region in regions {
            let offset = (region.loc.y as usize * size[0] as usize + region.loc.x as usize) * 4;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { x: region.loc.x as u32, y: region.loc.y as u32, z: 0 }, ..texture.as_image_copy() },
                &pixels[offset..],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 4), rows_per_image: Some(size[1]) },
                wgpu::Extent3d { width: region.size.w as u32, height: region.size.h as u32, depth_or_array_layers: 1 },
            );
        }
        buffer.cache(Content {
            device: self.device.clone(),
            texture: texture.clone(),
            opaque,
        });
        Ok(Some((texture, opaque)))
    }
}
