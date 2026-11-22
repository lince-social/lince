// non-lean-yet
// Scope: struct Content, struct RetainedCapture, struct CaptureBudget, fn retained_capture.
// BeautifulRust (baseline lines):
// - [review: §8 Unsafe] Check local SAFETY proof against the operation. Sites 211.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 35, 54, 62, 68,
//   82, 101, 138, 148, 151, 152, 233, 234, 267, 268.

use smithay::{
    backend::allocator::{Buffer, Fourcc, Modifier},
    reexports::wayland_server::protocol::wl_shm::Format,
    wayland::{dmabuf::get_dmabuf, shm::with_buffer_contents},
};

use super::*;
use crate::server::ClientBuffer;

struct Content {
    device: wgpu::Device,
    texture: wgpu::Texture,
    opaque: bool,
    capture: std::sync::Mutex<Option<std::result::Result<Arc<RetainedCapture>, String>>>,
}

pub(crate) struct RetainedCapture {
    pub(crate) image: ExportedImage,
    _budget: CaptureBudget,
}

struct CaptureBudget {
    used: Arc<std::sync::atomic::AtomicU64>,
    bytes: u64,
}

impl Drop for CaptureBudget {
    fn drop(&mut self) {
        self.used
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Copies validated, mapped SHM rows into a tightly packed owned upload.
///
/// # Safety
/// Every row beginning at `source` with the given stride must remain mapped and
/// readable for the call. The caller must have checked the size and row bounds.
unsafe fn read_shm_rows(source: *const u8, stride: usize, row: usize, height: usize) -> Vec<u8> {
    let bytes = row * height;
    let mut pixels = Vec::<u8>::with_capacity(bytes);
    // SAFETY: The caller validated source rows. The fresh allocation has room for
    // all packed rows and cannot overlap the client pool. No reference to the
    // client memory is created. The copies initialize every byte before set_len.
    unsafe {
        if stride == row {
            std::ptr::copy_nonoverlapping(source, pixels.as_mut_ptr(), bytes);
        } else {
            for y in 0..height {
                std::ptr::copy_nonoverlapping(
                    source.add(y * stride),
                    pixels.as_mut_ptr().add(y * row),
                    row,
                );
            }
        }
        pixels.set_len(bytes);
    }
    pixels
}

#[cfg(feature = "nested")]
pub(crate) fn retained_capture(
    buffer: &ClientBuffer,
) -> std::result::Result<Option<Arc<RetainedCapture>>, String> {
    let Some(content) = buffer.cached::<Content>() else {
        return Ok(None);
    };
    let result = content
        .capture
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    result.transpose()
}

pub(crate) fn retire_capture(buffer: &ClientBuffer) {
    if let Some(content) = buffer.cached::<Content>() {
        let capture = content
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        drop(capture);
    }
}

impl VulkanRenderer {
    #[cfg(all(test, feature = "nested", feature = "server-tests"))]
    pub(crate) fn capture_test_bytes(&self) -> u64 {
        self.capture_bytes
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn retain_capture(&self, buffer: &ClientBuffer) -> bool {
        let Some(content) = buffer.cached::<Content>() else {
            return false;
        };
        let captured = content
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if let Some(captured) = captured {
            return captured.map_or(true, |capture| capture.image.ready().unwrap_or(true));
        }
        let bytes_per_pixel = if content.texture.format() == wgpu::TextureFormat::Rgba16Float {
            8
        } else {
            4
        };
        let bytes = u64::from(content.texture.width())
            * u64::from(content.texture.height())
            * bytes_per_pixel;
        if self
            .capture_bytes
            .try_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |used| {
                    used.checked_add(bytes)
                        .filter(|total| *total <= 256 * 1024 * 1024)
                },
            )
            .is_err()
        {
            return false;
        }
        let budget = CaptureBudget {
            used: self.capture_bytes.clone(),
            bytes,
        };
        let image = if content.device != self.device {
            Err("The retained capture belongs to another render device".to_owned())
        } else {
            self.export_copy(&content.texture)
                .map(|image| {
                    let capture = Arc::new(RetainedCapture {
                        image,
                        _budget: budget,
                    });
                    self.retain(capture.clone());
                    capture
                })
                .map_err(|error| error.to_string())
        };
        *content
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(image);
        false
    }

    pub(crate) fn imports_ready(&self) -> bool {
        self.waits
            .borrow()
            .values()
            .any(crate::graphics::wait::Watch::ready)
    }

    pub(crate) fn retain_imports(&self, windows: &[Arc<crate::server::WindowSnapshot>]) {
        let mut waits = self.waits.borrow_mut();
        if waits.is_empty() {
            return;
        }
        let live: std::collections::HashSet<_> = windows
            .iter()
            .flat_map(|window| &window.nodes)
            .map(|node| (node.content.buffer().clone(), node.content.version))
            .collect();
        waits.retain(|key, _| live.contains(key));
    }

    pub(crate) fn import_surface(
        &self,
        node: &crate::server::SurfaceNode,
        previous: Option<(
            &wgpu::Texture,
            smithay::backend::renderer::utils::CommitCounter,
        )>,
    ) -> Result<Option<(wgpu::Texture, bool)>> {
        let buffer: &ClientBuffer = &node.content;
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
        let damage = node.buffer_damage_since(previous.map(|(_, commit)| commit));
        let target = previous.map(|(texture, _)| texture);
        if let Ok(dmabuf) = get_dmabuf(buffer.buffer()) {
            let bytes_per_pixel = if dmabuf.format().code == Fourcc::Abgr16161616f {
                8_u64
            } else {
                4
            };
            let size = dmabuf.size();
            let bytes = u64::try_from(size.w)
                .ok()
                .zip(u64::try_from(size.h).ok())
                .and_then(|(width, height)| width.checked_mul(height))
                .and_then(|pixels| pixels.checked_mul(bytes_per_pixel));
            if !matches!(
                dmabuf.format().code,
                Fourcc::Argb8888
                    | Fourcc::Xrgb8888
                    | Fourcc::Abgr8888
                    | Fourcc::Abgr2101010
                    | Fourcc::Abgr16161616f
            ) || dmabuf.format().modifier != Modifier::Linear
                || dmabuf.num_planes() != 1
                || bytes.is_none_or(|bytes| bytes > 256 * 1024 * 1024)
            {
                return Err(ProbeFailure::at(
                    "client_import",
                    "unsupported DMA-BUF format",
                ));
            }
            let source = match import::acquire(dmabuf, "client_acquire")? {
                import::Acquire::Ready(source) => source,
                import::Acquire::Pending(fd) => {
                    let key = (buffer.buffer().clone(), buffer.version);
                    let mut waits = self.waits.borrow_mut();
                    if !waits.contains_key(&key) {
                        let watch = self
                            .reactor
                            .descriptor(fd.try_clone_to_owned().map_err(|error| {
                                ProbeFailure::at("client_acquire_events", error)
                            })?)
                            .map_err(|error| ProbeFailure::at("client_acquire_events", error))?;
                        waits.insert(key, watch);
                    }
                    return Ok(None);
                }
            };
            {
                let mut waits = self.waits.borrow_mut();
                if !waits.is_empty() {
                    waits.remove(&(buffer.buffer().clone(), buffer.version));
                }
            }
            let texture = self.import_acquired_dmabuf(source, target, Some(&damage))?;
            let opaque = dmabuf.format().code == Fourcc::Xrgb8888;
            buffer.cache(Content {
                device: self.device.clone(),
                texture: texture.clone(),
                opaque,
                capture: Default::default(),
            });
            return Ok(Some((texture, opaque)));
        }
        let started = std::time::Instant::now();
        let (uploads, size, opaque, preserve) =
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
                let bounds = smithay::utils::Rectangle::from_size((data.width, data.height).into());
                let previous = target.filter(|texture| {
                    texture.width() == data.width as u32
                        && texture.height() == data.height as u32
                        && texture.format() == wgpu::TextureFormat::Bgra8Unorm
                        && texture.usage().contains(wgpu::TextureUsages::COPY_SRC)
                });
                let mut regions = if previous.is_some() {
                    damage
                        .iter()
                        .filter_map(|damage| damage.intersection(bounds))
                        .filter(|region| !region.is_empty())
                        .collect::<Vec<_>>()
                } else {
                    vec![bounds]
                };
                let total = regions.iter().try_fold(0_usize, |total, region| {
                    total.checked_add(region.size.w as usize * region.size.h as usize * 4)
                });
                if total.is_none_or(|total| total > bytes) || regions.contains(&bounds) {
                    regions = vec![bounds];
                }
                let preserve = previous.is_some() && regions != [bounds];
                let mut uploads = Vec::with_capacity(regions.len());
                for region in regions {
                    let row = region.size.w as usize * 4;
                    let stride = data.stride as usize;
                    let offset = data.offset as usize
                        + region.loc.y as usize * stride
                        + region.loc.x as usize * 4;
                    // SAFETY: The callback holds the pool mapped. The validated pool
                    // bounds cover every row in this clipped rectangle.
                    let pixels = unsafe {
                        read_shm_rows(pointer.add(offset), stride, row, region.size.h as usize)
                    };
                    uploads.push((region, pixels));
                }
                Ok((
                    uploads,
                    [data.width as u32, data.height as u32],
                    data.format == Format::Xrgb8888,
                    preserve,
                ))
            })
            .map_err(|error| ProbeFailure::at("client_shm", error))??;
        crate::runtime::trace_frame("shm-read", started);
        let started = std::time::Instant::now();
        let previous = target.filter(|_| preserve);
        if uploads.is_empty()
            && let Some(previous) = previous
        {
            buffer.cache(Content {
                device: self.device.clone(),
                texture: previous.clone(),
                opaque,
                capture: Default::default(),
            });
            return Ok(Some((previous.clone(), opaque)));
        }
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
        if let Some(previous) = previous {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_texture(
                previous.as_image_copy(),
                texture.as_image_copy(),
                texture.size(),
            );
            self.queue.submit([encoder.finish()]);
        }
        for (region, pixels) in uploads {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d {
                        x: region.loc.x as u32,
                        y: region.loc.y as u32,
                        z: 0,
                    },
                    ..texture.as_image_copy()
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(region.size.w as u32 * 4),
                    rows_per_image: Some(region.size.h as u32),
                },
                wgpu::Extent3d {
                    width: region.size.w as u32,
                    height: region.size.h as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        buffer.cache(Content {
            device: self.device.clone(),
            texture: texture.clone(),
            opaque,
            capture: Default::default(),
        });
        crate::runtime::trace_frame("shm-upload", started);
        Ok(Some((texture, opaque)))
    }
}
