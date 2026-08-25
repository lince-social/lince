use ash::vk;
use cef::{AcceleratedPaintInfo, ColorType};
use std::fmt::{Display, Formatter};
use wgpu::hal::api;

const REQUIRED_EXTENSIONS: [&[u8]; 3] = [
    b"VK_KHR_external_memory_fd\0",
    b"VK_EXT_external_memory_dma_buf\0",
    b"VK_EXT_image_drm_format_modifier\0",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmaBufDescriptor {
    pub fd: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u64,
    pub allocation_size: u64,
    pub modifier: u64,
    pub format: vk::Format,
}

impl DmaBufDescriptor {
    pub fn from_cef(info: &AcceleratedPaintInfo) -> Result<Self, VulkanInteropError> {
        if info.plane_count != 1 {
            return Err(VulkanInteropError::new(format!(
                "accelerated CEF paint requires one DMA-BUF plane, received {}",
                info.plane_count
            )));
        }
        let width = u32::try_from(info.extra.coded_size.width)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| VulkanInteropError::new("CEF paint width must be positive"))?;
        let height = u32::try_from(info.extra.coded_size.height)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| VulkanInteropError::new("CEF paint height must be positive"))?;
        let plane = &info.planes[0];
        if plane.fd < 0 {
            return Err(VulkanInteropError::new(
                "CEF paint supplied an invalid DMA-BUF descriptor",
            ));
        }
        if plane.stride == 0 || plane.size == 0 {
            return Err(VulkanInteropError::new(
                "CEF paint supplied an empty DMA-BUF layout",
            ));
        }
        let format = if info.format == ColorType::BGRA_8888 {
            vk::Format::B8G8R8A8_UNORM
        } else if info.format == ColorType::RGBA_8888 {
            vk::Format::R8G8B8A8_UNORM
        } else {
            return Err(VulkanInteropError::new(format!(
                "CEF paint color format {:?} is unsupported",
                info.format
            )));
        };
        Ok(Self {
            fd: plane.fd,
            width,
            height,
            stride: plane.stride,
            offset: plane.offset,
            allocation_size: plane.size,
            modifier: info.modifier,
            format,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VulkanCopyFacts {
    pub received_frames: u64,
    pub copied_frames: u64,
    pub composed_frames: u64,
    pub refused_frames: u64,
    pub latest_width: u32,
    pub latest_height: u32,
    pub latest_modifier: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VulkanInteropError {
    detail: String,
}

impl VulkanInteropError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for VulkanInteropError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for VulkanInteropError {}

struct OwnedImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    width: u32,
    height: u32,
    format: vk::Format,
    initialized: bool,
}

struct ImportedImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
}

pub struct VulkanCopyTarget {
    device: wgpu::Device,
    owned: Option<OwnedImage>,
    facts: VulkanCopyFacts,
    last_error: Option<String>,
}

impl VulkanCopyTarget {
    pub fn new(device: wgpu::Device) -> Result<Self, VulkanInteropError> {
        let hal_device = unsafe { device.as_hal::<api::Vulkan>() }
            .ok_or_else(|| VulkanInteropError::new("CEF acceleration requires Vulkan"))?;
        for required in REQUIRED_EXTENSIONS {
            if !hal_device
                .enabled_device_extensions()
                .iter()
                .any(|enabled| enabled.to_bytes_with_nul() == required)
            {
                return Err(VulkanInteropError::new(format!(
                    "host Vulkan device is missing {}",
                    String::from_utf8_lossy(&required[..required.len() - 1])
                )));
            }
        }
        drop(hal_device);
        Ok(Self {
            device,
            owned: None,
            facts: VulkanCopyFacts::default(),
            last_error: None,
        })
    }

    pub fn facts(&self) -> VulkanCopyFacts {
        self.facts
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn copy_from_cef(&mut self, info: &AcceleratedPaintInfo) -> Result<(), VulkanInteropError> {
        self.facts.received_frames += 1;
        let result = self.copy_from_cef_inner(info);
        match &result {
            Ok(()) => {
                self.facts.copied_frames += 1;
                self.last_error = None;
            }
            Err(error) => {
                self.facts.refused_frames += 1;
                self.last_error = Some(error.to_string());
            }
        }
        result
    }

    fn copy_from_cef_inner(
        &mut self,
        info: &AcceleratedPaintInfo,
    ) -> Result<(), VulkanInteropError> {
        let descriptor = DmaBufDescriptor::from_cef(info)?;
        let device = self.device.clone();
        let hal_device = unsafe { device.as_hal::<api::Vulkan>() }
            .ok_or_else(|| VulkanInteropError::new("host WGPU device stopped exposing Vulkan"))?;
        let raw = hal_device.raw_device();
        let replace = self.owned.as_ref().is_none_or(|owned| {
            owned.width != descriptor.width
                || owned.height != descriptor.height
                || owned.format != descriptor.format
        });
        if replace {
            if let Some(owned) = self.owned.take() {
                unsafe {
                    raw.destroy_image(owned.image, None);
                    raw.free_memory(owned.memory, None);
                }
            }
            self.owned = Some(create_owned_image(&hal_device, &descriptor)?);
        }
        let imported = create_imported_image(&hal_device, &descriptor)?;
        let owned = self
            .owned
            .as_mut()
            .ok_or_else(|| VulkanInteropError::new("CEF copy target was not allocated"))?;
        let copy_result = submit_cef_copy(&hal_device, &descriptor, &imported, owned);
        unsafe {
            raw.destroy_image(imported.image, None);
            raw.free_memory(imported.memory, None);
        }
        copy_result?;
        owned.initialized = true;
        self.facts.latest_width = descriptor.width;
        self.facts.latest_height = descriptor.height;
        self.facts.latest_modifier = descriptor.modifier;
        Ok(())
    }

    pub fn compose_into(
        &mut self,
        target: &wgpu::Texture,
        target_size: [u32; 2],
        target_rect: [u32; 4],
    ) -> Result<(), VulkanInteropError> {
        let result = self.compose_into_inner(target, target_size, target_rect);
        match &result {
            Ok(()) => {
                self.facts.composed_frames += 1;
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
        result
    }

    fn compose_into_inner(
        &self,
        target: &wgpu::Texture,
        target_size: [u32; 2],
        target_rect: [u32; 4],
    ) -> Result<(), VulkanInteropError> {
        let owned = self
            .owned
            .as_ref()
            .filter(|owned| owned.initialized)
            .ok_or_else(|| VulkanInteropError::new("CEF has not supplied a GPU frame yet"))?;
        validate_target_rect(target_size, target_rect)?;
        let device = self.device.clone();
        let hal_device = unsafe { device.as_hal::<api::Vulkan>() }
            .ok_or_else(|| VulkanInteropError::new("host WGPU device stopped exposing Vulkan"))?;
        let hal_texture = unsafe { target.as_hal::<api::Vulkan>() }
            .ok_or_else(|| VulkanInteropError::new("host surface texture is not Vulkan"))?;
        let target_image = unsafe { hal_texture.raw_handle() };
        submit_composition(&hal_device, owned, target_image, target_rect)
    }
}

impl Drop for VulkanCopyTarget {
    fn drop(&mut self) {
        let Some(owned) = self.owned.take() else {
            return;
        };
        let device = self.device.clone();
        let Some(hal_device) = (unsafe { device.as_hal::<api::Vulkan>() }) else {
            return;
        };
        unsafe {
            let _ = hal_device.raw_device().device_wait_idle();
            hal_device.raw_device().destroy_image(owned.image, None);
            hal_device.raw_device().free_memory(owned.memory, None);
        }
    }
}

fn validate_target_rect(
    target_size: [u32; 2],
    target_rect: [u32; 4],
) -> Result<(), VulkanInteropError> {
    let [x, y, width, height] = target_rect;
    if width == 0 || height == 0 {
        return Err(VulkanInteropError::new(
            "CEF composition rectangle must be nonempty",
        ));
    }
    if x.checked_add(width)
        .is_none_or(|right| right > target_size[0])
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > target_size[1])
    {
        return Err(VulkanInteropError::new(
            "CEF composition rectangle exceeds the host surface",
        ));
    }
    Ok(())
}

fn create_owned_image(
    hal_device: &wgpu::hal::vulkan::Device,
    descriptor: &DmaBufDescriptor,
) -> Result<OwnedImage, VulkanInteropError> {
    let raw = hal_device.raw_device();
    let create = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(descriptor.format)
        .extent(vk::Extent3D {
            width: descriptor.width,
            height: descriptor.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let image = unsafe { raw.create_image(&create, None) }
        .map_err(|error| VulkanInteropError::new(format!("create copy image: {error:?}")))?;
    let requirements = unsafe { raw.get_image_memory_requirements(image) };
    let memory_type = find_memory_type(
        hal_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .ok_or_else(|| VulkanInteropError::new("no device-local memory for CEF copy image"));
    let memory_type = match memory_type {
        Ok(value) => value,
        Err(error) => {
            unsafe { raw.destroy_image(image, None) };
            return Err(error);
        }
    };
    let allocate = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type);
    let memory = match unsafe { raw.allocate_memory(&allocate, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { raw.destroy_image(image, None) };
            return Err(VulkanInteropError::new(format!(
                "allocate copy image: {error:?}"
            )));
        }
    };
    if let Err(error) = unsafe { raw.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw.free_memory(memory, None);
            raw.destroy_image(image, None);
        }
        return Err(VulkanInteropError::new(format!(
            "bind copy image: {error:?}"
        )));
    }
    Ok(OwnedImage {
        image,
        memory,
        width: descriptor.width,
        height: descriptor.height,
        format: descriptor.format,
        initialized: false,
    })
}

fn create_imported_image(
    hal_device: &wgpu::hal::vulkan::Device,
    descriptor: &DmaBufDescriptor,
) -> Result<ImportedImage, VulkanInteropError> {
    let raw = hal_device.raw_device();
    let layout = vk::SubresourceLayout {
        offset: descriptor.offset,
        size: descriptor.allocation_size,
        row_pitch: u64::from(descriptor.stride),
        array_pitch: 0,
        depth_pitch: 0,
    };
    let layouts = [layout];
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(descriptor.modifier)
        .plane_layouts(&layouts);
    let create = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(descriptor.format)
        .extent(vk::Extent3D {
            width: descriptor.width,
            height: descriptor.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .push_next(&mut external)
        .push_next(&mut modifier);
    let image = unsafe { raw.create_image(&create, None) }
        .map_err(|error| VulkanInteropError::new(format!("import DMA-BUF image: {error:?}")))?;
    let requirements = unsafe { raw.get_image_memory_requirements(image) };
    let duplicated_fd = unsafe { libc::fcntl(descriptor.fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicated_fd < 0 {
        unsafe { raw.destroy_image(image, None) };
        return Err(VulkanInteropError::new(format!(
            "duplicate DMA-BUF descriptor: {}",
            std::io::Error::last_os_error()
        )));
    }
    let memory_type = find_memory_type(
        hal_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::empty(),
    );
    let Some(memory_type) = memory_type else {
        unsafe {
            libc::close(duplicated_fd);
            raw.destroy_image(image, None);
        }
        return Err(VulkanInteropError::new(
            "no compatible memory type for CEF DMA-BUF",
        ));
    };
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut import = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(duplicated_fd);
    let allocate = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type)
        .push_next(&mut dedicated)
        .push_next(&mut import);
    let memory = match unsafe { raw.allocate_memory(&allocate, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe {
                libc::close(duplicated_fd);
                raw.destroy_image(image, None);
            }
            return Err(VulkanInteropError::new(format!(
                "import DMA-BUF memory: {error:?}"
            )));
        }
    };
    if let Err(error) = unsafe { raw.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw.free_memory(memory, None);
            raw.destroy_image(image, None);
        }
        return Err(VulkanInteropError::new(format!(
            "bind DMA-BUF memory: {error:?}"
        )));
    }
    Ok(ImportedImage { image, memory })
}

fn find_memory_type(
    hal_device: &wgpu::hal::vulkan::Device,
    allowed: u32,
    required: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let memory = unsafe {
        hal_device
            .shared_instance()
            .raw_instance()
            .get_physical_device_memory_properties(hal_device.raw_physical_device())
    };
    (0..memory.memory_type_count).find(|index| {
        allowed & (1 << index) != 0
            && memory.memory_types[*index as usize]
                .property_flags
                .contains(required)
    })
}

fn submit_cef_copy(
    hal_device: &wgpu::hal::vulkan::Device,
    descriptor: &DmaBufDescriptor,
    imported: &ImportedImage,
    owned: &OwnedImage,
) -> Result<(), VulkanInteropError> {
    submit_one_time(hal_device, |raw, command| unsafe {
        let source_to_copy = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .image(imported.image)
            .subresource_range(color_range());
        let destination_to_copy = vk::ImageMemoryBarrier::default()
            .src_access_mask(if owned.initialized {
                vk::AccessFlags::TRANSFER_READ
            } else {
                vk::AccessFlags::empty()
            })
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(if owned.initialized {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            } else {
                vk::ImageLayout::UNDEFINED
            })
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(owned.image)
            .subresource_range(color_range());
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[source_to_copy, destination_to_copy],
        );
        let copy = vk::ImageCopy::default()
            .src_subresource(color_layers())
            .dst_subresource(color_layers())
            .extent(vk::Extent3D {
                width: descriptor.width,
                height: descriptor.height,
                depth: 1,
            });
        raw.cmd_copy_image(
            command,
            imported.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            owned.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy],
        );
        let source_restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(imported.image)
            .subresource_range(color_range());
        let destination_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .image(owned.image)
            .subresource_range(color_range());
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[source_restore, destination_ready],
        );
    })
}

fn submit_composition(
    hal_device: &wgpu::hal::vulkan::Device,
    source: &OwnedImage,
    target: vk::Image,
    target_rect: [u32; 4],
) -> Result<(), VulkanInteropError> {
    submit_one_time(hal_device, |raw, command| unsafe {
        let destination_to_copy = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(target)
            .subresource_range(color_range());
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[destination_to_copy],
        );
        let [x, y, width, height] = target_rect;
        let blit = vk::ImageBlit::default()
            .src_subresource(color_layers())
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: source.width as i32,
                    y: source.height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(color_layers())
            .dst_offsets([
                vk::Offset3D {
                    x: x as i32,
                    y: y as i32,
                    z: 0,
                },
                vk::Offset3D {
                    x: (x + width) as i32,
                    y: (y + height) as i32,
                    z: 1,
                },
            ]);
        raw.cmd_blit_image(
            command,
            source.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[blit],
            vk::Filter::LINEAR,
        );
        let destination_restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(target)
            .subresource_range(color_range());
        raw.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[destination_restore],
        );
    })
}

fn submit_one_time(
    hal_device: &wgpu::hal::vulkan::Device,
    record: impl FnOnce(&ash::Device, vk::CommandBuffer),
) -> Result<(), VulkanInteropError> {
    let raw = hal_device.raw_device();
    let pool_create = vk::CommandPoolCreateInfo::default()
        .queue_family_index(hal_device.queue_family_index())
        .flags(vk::CommandPoolCreateFlags::TRANSIENT);
    let pool = unsafe { raw.create_command_pool(&pool_create, None) }
        .map_err(|error| VulkanInteropError::new(format!("create command pool: {error:?}")))?;
    let allocate = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command = match unsafe { raw.allocate_command_buffers(&allocate) } {
        Ok(commands) => commands[0],
        Err(error) => {
            unsafe { raw.destroy_command_pool(pool, None) };
            return Err(VulkanInteropError::new(format!(
                "allocate command buffer: {error:?}"
            )));
        }
    };
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    if let Err(error) = unsafe { raw.begin_command_buffer(command, &begin) } {
        unsafe { raw.destroy_command_pool(pool, None) };
        return Err(VulkanInteropError::new(format!(
            "begin command buffer: {error:?}"
        )));
    }
    record(raw, command);
    if let Err(error) = unsafe { raw.end_command_buffer(command) } {
        unsafe { raw.destroy_command_pool(pool, None) };
        return Err(VulkanInteropError::new(format!(
            "finish command buffer: {error:?}"
        )));
    }
    let fence = match unsafe { raw.create_fence(&vk::FenceCreateInfo::default(), None) } {
        Ok(fence) => fence,
        Err(error) => {
            unsafe { raw.destroy_command_pool(pool, None) };
            return Err(VulkanInteropError::new(format!("create fence: {error:?}")));
        }
    };
    let commands = [command];
    let submit = vk::SubmitInfo::default().command_buffers(&commands);
    let result = unsafe {
        raw.queue_submit(hal_device.raw_queue(), &[submit], fence)
            .map_err(|error| VulkanInteropError::new(format!("submit GPU copy: {error:?}")))
            .and_then(|()| {
                raw.wait_for_fences(&[fence], true, 2_000_000_000)
                    .map_err(|error| {
                        VulkanInteropError::new(format!("wait for GPU copy: {error:?}"))
                    })
            })
    };
    unsafe {
        raw.destroy_fence(fence, None);
        raw.destroy_command_pool(pool, None);
    }
    result
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_rect_rejects_empty_and_out_of_bounds_regions() {
        assert!(validate_target_rect([100, 80], [0, 0, 100, 80]).is_ok());
        assert!(validate_target_rect([100, 80], [0, 0, 0, 80]).is_err());
        assert!(validate_target_rect([100, 80], [90, 0, 20, 80]).is_err());
        assert!(validate_target_rect([100, 80], [0, 70, 100, 20]).is_err());
    }
}
