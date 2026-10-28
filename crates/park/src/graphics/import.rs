use super::*;
use smithay::backend::allocator::{Buffer, Fourcc, Modifier};
use std::os::fd::{AsRawFd, IntoRawFd};

impl VulkanRenderer {
    pub(crate) fn import_dmabuf(&self, buffer: &Dmabuf) -> Result<wgpu::Texture> {
        self.import_dmabuf_into(buffer, None)
    }

    pub(super) fn import_dmabuf_into(
        &self,
        buffer: &Dmabuf,
        target: Option<&wgpu::Texture>,
    ) -> Result<wgpu::Texture> {
        self.import_dmabuf_regions(buffer, target, None)
    }

    pub(super) fn import_dmabuf_regions(
        &self,
        buffer: &Dmabuf,
        target: Option<&wgpu::Texture>,
        damage: Option<&[smithay::utils::Rectangle<i32, smithay::utils::Buffer>]>,
    ) -> Result<wgpu::Texture> {
        for fd in buffer.handles() {
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            rustix::event::poll(&mut poll, Some(&timeout))
                .map_err(|error| ProbeFailure::at("dmabuf_acquire", error))?;
            if !poll[0].revents().contains(rustix::event::PollFlags::IN) {
                return Err(ProbeFailure::at(
                    "dmabuf_acquire",
                    "client DMA-BUF did not become readable",
                ));
            }
        }
        let size = buffer.size();
        let (vk_format, texture_format, bytes_per_pixel) = match buffer.format().code {
            Fourcc::Argb8888 => (vk::Format::B8G8R8A8_UNORM, wgpu::TextureFormat::Bgra8Unorm, 4),
            Fourcc::Abgr8888 => (vk::Format::R8G8B8A8_UNORM, wgpu::TextureFormat::Rgba8Unorm, 4),
            Fourcc::Abgr16161616f => (vk::Format::R16G16B16A16_SFLOAT, wgpu::TextureFormat::Rgba16Float, 8),
            Fourcc::Abgr2101010 => (vk::Format::A2B10G10R10_UNORM_PACK32, wgpu::TextureFormat::Rgb10a2Unorm, 4),
            _ => return Err(ProbeFailure::at("vulkan_import", "This DMA-BUF color format is not implemented")),
        };
        if buffer.num_planes() != 1
            || buffer.format().modifier != Modifier::Linear
            || size.w <= 0
            || size.h <= 0
            || size.w > 16384
            || size.h > 16384
        {
            return Err(ProbeFailure::at(
                "vulkan_import",
                "Only single-plane linear BGRA8, RGBA8, RGB10A2 or RGBA16F buffers with valid dimensions are implemented",
            ));
        }
        let fd = buffer
            .handles()
            .next()
            .unwrap()
            .try_clone_to_owned()
            .map_err(|error| ProbeFailure::at("vulkan_import_fd", error))?;
        let layout = vk::SubresourceLayout::default()
            .offset(u64::from(buffer.offsets().next().unwrap()))
            .row_pitch(u64::from(buffer.strides().next().unwrap()));
        let buffer_size = rustix::fs::seek(&fd, rustix::fs::SeekFrom::End(0))
            .map_err(|error| ProbeFailure::at("vulkan_import_size", error))?;
        rustix::fs::seek(&fd, rustix::fs::SeekFrom::Start(0))
            .map_err(|error| ProbeFailure::at("vulkan_import_size", error))?;
        validate_plane(
            size.w as u64,
            size.h as u64,
            layout.row_pitch,
            layout.offset,
            buffer_size,
            bytes_per_pixel,
        )?;
        let layouts = [layout];
        let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(0)
            .plane_layouts(&layouts);
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(vk::Extent3D {
                width: size.w as u32,
                height: size.h as u32,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut modifier)
            .push_next(&mut external);
        let image = unsafe { self.raw_device.create_image(&image_info, None) }
            .map_err(|error| ProbeFailure::at("vulkan_import_image", error))?;
        let imported = (|| {
            let requirements = unsafe { self.raw_device.get_image_memory_requirements(image) };
            if requirements.size > buffer_size {
                return Err(ProbeFailure::at(
                    "vulkan_import_size",
                    "the DMA-BUF is smaller than the Vulkan image memory requirement",
                ));
            }
            let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
            unsafe {
                khr::external_memory_fd::Device::new(&self.raw_instance, &self.raw_device)
                    .get_memory_fd_properties(
                        vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                        fd.as_raw_fd(),
                        &mut fd_properties,
                    )
            }
            .map_err(|error| ProbeFailure::at("vulkan_import_memory", error))?;
            let mask = requirements.memory_type_bits & fd_properties.memory_type_bits;
            let memory_type = (0..32)
                .find(|index| mask & (1 << index) != 0)
                .ok_or_else(|| {
                    ProbeFailure::at("vulkan_import_memory", "no compatible DMA-BUF memory type")
                })?;
            let mut import = vk::ImportMemoryFdInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
                .fd(fd.as_raw_fd());
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let info = vk::MemoryAllocateInfo::default()
                .allocation_size(buffer_size)
                .memory_type_index(memory_type)
                .push_next(&mut import)
                .push_next(&mut dedicated);
            unsafe { self.raw_device.allocate_memory(&info, None) }
                .map_err(|error| ProbeFailure::at("vulkan_import_memory", error))
        })();
        let memory = match imported {
            Ok(memory) => {
                let _ = fd.into_raw_fd();
                memory
            }
            Err(error) => {
                unsafe { self.raw_device.destroy_image(image, None) };
                return Err(error);
            }
        };
        let allocation = Allocation {
            image,
            memory,
            raw_device: self.raw_device.clone(),
            _device: self.device.clone(),
        };
        unsafe { self.raw_device.bind_image_memory(image, memory, 0) }
            .map_err(|error| ProbeFailure::at("vulkan_import_bind", error))?;
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING;
        let previous = target.filter(|_| damage.is_some())
            .filter(|texture| {
                texture.width() == size.w as u32
                    && texture.height() == size.h as u32
                    && texture.format() == texture_format
                    && texture.usage().contains(usage)
            });
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("park_client_content"),
                    size: wgpu::Extent3d {
                        width: size.w as u32,
                        height: size.h as u32,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: texture_format,
                    usage,
                    view_formats: &[],
                });
        let view = texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(previous) = previous {
            encoder.copy_texture_to_texture(previous.as_image_copy(), texture.as_image_copy(), texture.size());
        }
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if previous.is_some() { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit([encoder.finish()]);
        let destination = {
            let destination = unsafe { texture.as_hal::<Vulkan>() }
                .ok_or_else(|| ProbeFailure::at("vulkan_import_copy", "Vulkan HAL unavailable"))?;
            unsafe { destination.raw_handle() }
        };
        self.submit_raw("vulkan_import_copy", |command| {
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let acquire = vk::ImageMemoryBarrier::default()
                .image(image)
                .subresource_range(range)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                .dst_queue_family_index(self.queue_family)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
            let destination_barrier = vk::ImageMemoryBarrier::default()
                .image(destination)
                .subresource_range(range)
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            let layers = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            let bounds = smithay::utils::Rectangle::from_size((size.w, size.h).into());
            let changed = if previous.is_some() { damage.unwrap_or(&[]) } else { std::slice::from_ref(&bounds) };
            let regions: Vec<_> = changed.iter().filter_map(|region| region.intersection(bounds)).map(|region| vk::ImageCopy::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offset(vk::Offset3D { x: region.loc.x, y: region.loc.y, z: 0 })
                .dst_offset(vk::Offset3D { x: region.loc.x, y: region.loc.y, z: 0 })
                .extent(vk::Extent3D {
                    width: region.size.w as u32,
                    height: region.size.h as u32,
                    depth: 1,
                })).collect();
            let release = vk::ImageMemoryBarrier::default()
                .image(image)
                .subresource_range(range)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(self.queue_family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_READ);
            let restore = vk::ImageMemoryBarrier::default()
                .image(destination)
                .subresource_range(range)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            unsafe {
                self.raw_device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[acquire, destination_barrier],
                );
                if !regions.is_empty() { self.raw_device.cmd_copy_image(
                    command,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    destination,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &regions,
                ); }
                self.raw_device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[release, restore],
                );
            }
        })?;
        self.retain((allocation, texture.clone()));
        Ok(texture)
    }

    pub(crate) fn export_copy(&self, source: &wgpu::Texture) -> Result<ExportedImage> {
        let target = self.create_shared_image_format(source.width(), source.height(), source.format())?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            target.texture.as_image_copy(),
            source.size(),
        );
        self.queue.submit([encoder.finish()]);
        let submission = self.release(
            target.allocation.clone(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
        )?;
        self.retain((source.clone(), target.texture.clone()));
        Ok(ExportedImage {
            completion: self.completion.track(submission)?,
            dmabuf: target.dmabuf,
            _allocation: target.allocation,
        })
    }
}

fn validate_plane(width: u64, height: u64, stride: u64, offset: u64, length: u64, bytes_per_pixel: u64) -> Result<()> {
    if !matches!(bytes_per_pixel, 4 | 8) { return Err(ProbeFailure::at("vulkan_import_size", "Unsupported pixel size")); }
    let row = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| ProbeFailure::at("vulkan_import_size", "row size overflow"))?;
    let end = height
        .checked_sub(1)
        .and_then(|rows| rows.checked_mul(stride))
        .and_then(|value| value.checked_add(offset))
        .and_then(|value| value.checked_add(row));
    if width == 0
        || stride < row
        || stride % bytes_per_pixel != 0
        || offset % bytes_per_pixel != 0
        || end.is_none_or(|end| end > length)
    {
        return Err(ProbeFailure::at(
            "vulkan_import_size",
            "Invalid color-buffer layout or plane outside the backing allocation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporting_color_buffers_preserves_precision_and_channel_order_without_reinterpreting_unsupported_formats() {
        for (source, texture, vk_format, code) in [
            (wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Bgra8Unorm, vk::Format::B8G8R8A8_UNORM, Fourcc::Argb8888),
            (wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureFormat::Rgba8Unorm, vk::Format::R8G8B8A8_UNORM, Fourcc::Abgr8888),
            (wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::Rgba16Float, vk::Format::R16G16B16A16_SFLOAT, Fourcc::Abgr16161616f),
            (wgpu::TextureFormat::Rgb10a2Unorm, wgpu::TextureFormat::Rgb10a2Unorm, vk::Format::A2B10G10R10_UNORM_PACK32, Fourcc::Abgr2101010),
        ] {
            assert_eq!(shared_format(source).unwrap(), (texture, vk_format, code));
        }
        assert!(shared_format(wgpu::TextureFormat::Rgba32Float).is_err());
        assert!(shared_format(wgpu::TextureFormat::Depth32Float).is_err());
    }

    #[test]
    fn importing_a_plane_checks_the_entire_backing_range_and_alignment() {
        assert!(validate_plane(64, 32, 256, 0, 8192, 4).is_ok());
        assert!(validate_plane(63, 32, 256, 4, 8192, 4).is_ok());
        for (width, height, stride, offset, length) in [
            (64, 32, 252, 0, 8192),
            (64, 32, 256, 4, 8192),
            (64, 32, 257, 0, 16384),
            (64, 32, 256, 1, 16384),
            (64, u64::MAX, 256, 0, u64::MAX),
            (64, 32, 256, u64::MAX - 3, u64::MAX),
        ] {
            assert!(validate_plane(width, height, stride, offset, length, 4).is_err());
        }
    }

    #[test]
    fn high_precision_planes_require_eight_byte_pixels_and_reject_truncation_or_misaligned_offsets() {
        assert!(validate_plane(64, 32, 512, 0, 16384, 8).is_ok());
        assert!(validate_plane(63, 32, 512, 8, 16384, 8).is_ok());
        for (stride, offset, length) in [(256, 0, 16384), (512, 0, 16383), (512, 4, 32768), (516, 0, 32768)] {
            assert!(validate_plane(64, 32, stride, offset, length, 8).is_err());
        }
        assert!(validate_plane(1, 1, 8, 0, 8, 3).is_err());
    }

    #[test]
    #[ignore = "requires hardware Vulkan and linear RGBA16F DMA-BUF support"]
    fn half_float_export_and_reimport_preserve_values_above_sdr_white() {
        let renderer = VulkanRenderer::new().unwrap();
        let image = renderer.create_shared_image_format(4, 4, wgpu::TextureFormat::Rgba16Float).unwrap();
        let exported = renderer.clear(image, wgpu::Color { r: 2.0, g: 0.5, b: 0.25, a: 1.0 }).unwrap();
        exported.wait_for_probe().unwrap();
        assert_eq!(exported.dmabuf.format().code, Fourcc::Abgr16161616f);
        let imported = renderer.import_dmabuf(&exported.dmabuf).unwrap();
        assert_eq!(imported.format(), wgpu::TextureFormat::Rgba16Float);
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor { label: Some("park-hdr-precision-readback"),
            size: 1024, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(imported.as_image_copy(), wgpu::TexelCopyBufferInfo {
            buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(4) },
        }, imported.size());
        renderer.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = sender.send(result); });
        renderer.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) }).unwrap();
        receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap().unwrap();
        let pixels = slice.get_mapped_range();
        for y in 0..4 {
            for x in 0..4 {
                let offset = y * 256 + x * 8;
                let values = std::array::from_fn::<_, 4, _>(|channel| {
                    let offset = offset + channel * 2;
                    u16::from_le_bytes([pixels[offset], pixels[offset + 1]])
                });
                assert_eq!(values, [0x4000, 0x3800, 0x3400, 0x3c00]);
            }
        }
        drop(pixels); buffer.unmap();
    }

    #[test]
    #[ignore = "requires hardware Vulkan and linear RGB10A2 DMA-BUF support"]
    fn packed_ten_bit_export_and_reimport_preserve_channel_positions_and_all_precision() {
        let renderer = VulkanRenderer::new().unwrap();
        let image = renderer.create_shared_image_format(4, 4, wgpu::TextureFormat::Rgb10a2Unorm).unwrap();
        let exported = renderer.clear(image, wgpu::Color { r: 256.0 / 1023.0, g: 513.0 / 1023.0, b: 767.0 / 1023.0, a: 1.0 }).unwrap();
        exported.wait_for_probe().unwrap();
        assert_eq!(exported.dmabuf.format().code, Fourcc::Abgr2101010);
        let imported = renderer.import_dmabuf(&exported.dmabuf).unwrap();
        assert_eq!(imported.format(), wgpu::TextureFormat::Rgb10a2Unorm);
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor { label: Some("park-ten-bit-precision-readback"),
            size: 1024, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(imported.as_image_copy(), wgpu::TexelCopyBufferInfo {
            buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(4) },
        }, imported.size());
        renderer.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = sender.send(result); });
        renderer.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(5)) }).unwrap();
        receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap().unwrap();
        let pixels = slice.get_mapped_range();
        for y in 0..4 {
            for x in 0..4 {
                let offset = y * 256 + x * 4;
                let pixel = u32::from_le_bytes(pixels[offset..offset + 4].try_into().unwrap());
                assert_eq!([pixel & 1023, (pixel >> 10) & 1023, (pixel >> 20) & 1023, pixel >> 30], [256, 513, 767, 3]);
            }
        }
        drop(pixels); buffer.unmap();
    }
}
