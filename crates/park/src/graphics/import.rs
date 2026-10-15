use super::*;
use smithay::backend::allocator::{Buffer, Fourcc, Modifier};
use std::os::fd::{AsRawFd, IntoRawFd};

impl VulkanRenderer {
    pub(super) fn import_dmabuf(&self, buffer: &Dmabuf) -> Result<wgpu::Texture> {
        self.import_dmabuf_into(buffer, None)
    }

    pub(super) fn import_dmabuf_into(
        &self,
        buffer: &Dmabuf,
        target: Option<&wgpu::Texture>,
    ) -> Result<wgpu::Texture> {
        for fd in buffer.handles() {
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::time::Timespec {
                tv_sec: 5,
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
        if buffer.num_planes() != 1
            || buffer.format().code != Fourcc::Argb8888
            || buffer.format().modifier != Modifier::Linear
            || size.w <= 0
            || size.h <= 0
            || size.w > 16384
            || size.h > 16384
        {
            return Err(ProbeFailure::at(
                "vulkan_import",
                "only single-plane linear ARGB8888 buffers with valid dimensions are implemented",
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
        )?;
        let layouts = [layout];
        let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(0)
            .plane_layouts(&layouts);
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
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
        let texture = target
            .filter(|texture| {
                texture.width() == size.w as u32
                    && texture.height() == size.h as u32
                    && texture.format() == wgpu::TextureFormat::Bgra8Unorm
                    && texture.usage().contains(usage)
            })
            .cloned()
            .unwrap_or_else(|| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("park_client_content"),
                    size: wgpu::Extent3d {
                        width: size.w as u32,
                        height: size.h as u32,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage,
                    view_formats: &[],
                })
            });
        let view = texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| ProbeFailure::at("vulkan_import_initialize", error))?;
        let destination = unsafe { texture.as_hal::<Vulkan>() }
            .ok_or_else(|| ProbeFailure::at("vulkan_import_copy", "Vulkan HAL unavailable"))?;
        let destination = unsafe { destination.raw_handle() };
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
            let region = vk::ImageCopy::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .extent(vk::Extent3D {
                    width: size.w as u32,
                    height: size.h as u32,
                    depth: 1,
                });
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
                self.raw_device.cmd_copy_image(
                    command,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    destination,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
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
        drop(allocation);
        Ok(texture)
    }

    pub(crate) fn export_copy(&self, source: &wgpu::Texture) -> Result<ExportedImage> {
        let target = self.create_shared_image(source.width(), source.height())?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            target.texture.as_image_copy(),
            source.size(),
        );
        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| ProbeFailure::at("export_copy", error))?;
        self.release(
            target.allocation.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
        )?;
        Ok(ExportedImage {
            dmabuf: target.dmabuf,
            _allocation: target.allocation,
        })
    }
}

fn validate_plane(width: u64, height: u64, stride: u64, offset: u64, length: u64) -> Result<()> {
    let row = width
        .checked_mul(4)
        .ok_or_else(|| ProbeFailure::at("vulkan_import_size", "row size overflow"))?;
    let end = height
        .checked_sub(1)
        .and_then(|rows| rows.checked_mul(stride))
        .and_then(|value| value.checked_add(offset))
        .and_then(|value| value.checked_add(row));
    if width == 0
        || stride < row
        || stride % 4 != 0
        || offset % 4 != 0
        || end.is_none_or(|end| end > length)
    {
        return Err(ProbeFailure::at(
            "vulkan_import_size",
            "invalid ARGB8888 layout or plane outside the backing allocation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_a_plane_checks_the_entire_backing_range_and_alignment() {
        assert!(validate_plane(64, 32, 256, 0, 8192).is_ok());
        assert!(validate_plane(63, 32, 256, 4, 8192).is_ok());
        for (width, height, stride, offset, length) in [
            (64, 32, 252, 0, 8192),
            (64, 32, 256, 4, 8192),
            (64, 32, 257, 0, 16384),
            (64, 32, 256, 1, 16384),
            (64, u64::MAX, 256, 0, u64::MAX),
            (64, 32, 256, u64::MAX - 3, u64::MAX),
        ] {
            assert!(validate_plane(width, height, stride, offset, length).is_err());
        }
    }
}
