// non-lean-yet
// Scope: mod buffers, mod canvas, mod import, mod completion.
// BeautifulRust (baseline lines):
// - [review: §8 Unsafe] Check local SAFETY proof against the operation. Sites 163, 219, 259, 265,
//   271, 276, 293, 312, 401, 404, 406, 414, 430, 434, 448, 456, 465, 467, 504, 509, 532, 564, 617,
//   675, 741, 746.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 263, 308, 330,
//   355, 441, 442, 506, 708.

use std::{ffi::CStr, os::fd::FromRawFd, sync::Arc};

use ash::{ext, khr, vk};
use serde::Serialize;
use smithay::backend::{
    allocator::dmabuf::{Dmabuf, DmabufFlags},
    egl::{EGLContext, EGLDevice, EGLDisplay},
    renderer::{
        Bind, ExportMem, Frame, ImportDma, Offscreen, Renderer,
        gles::{GlesRenderbuffer, GlesRenderer},
    },
};
use wgpu::hal::api::Vulkan;

#[cfg(feature = "native-presentation")]
mod buffers;
#[cfg(feature = "native-presentation")]
pub(crate) use buffers::retire_capture;
#[cfg(feature = "nested")]
pub(crate) use buffers::{RetainedCapture, retained_capture};
#[cfg(feature = "graphics-gate")]
mod canvas;
mod import;
#[cfg(all(test, feature = "nested"))]
pub(crate) use import::direct;
mod completion;
mod synchronization;
pub use synchronization::probe as synchronization_probe;
#[cfg(all(test, feature = "nested", feature = "server-tests"))]
pub(crate) mod import_gate;
#[cfg(feature = "graphics-gate")]
mod live;
#[cfg(feature = "native-presentation")]
pub(crate) mod wait;

#[cfg(feature = "native-client-gate")]
pub(crate) use live::with_client;

const EXTENSIONS: [&CStr; 4] = [
    ext::image_drm_format_modifier::NAME,
    ext::external_memory_dma_buf::NAME,
    khr::external_memory_fd::NAME,
    khr::external_semaphore_fd::NAME,
];

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    adapter: Option<String>,
    pub egl_device: Option<String>,
    pub completed: Vec<String>,
    pub failure: Option<ProbeFailure>,
    pub full_graphics_gate_passed: bool,
}

#[derive(Debug, Serialize)]
pub struct ProbeFailure {
    pub stage: String,
    pub detail: String,
}

impl std::fmt::Display for ProbeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.detail)
    }
}

impl std::error::Error for ProbeFailure {}

impl ProbeFailure {
    fn at(stage: &str, detail: impl std::fmt::Display) -> Self {
        Self {
            stage: stage.into(),
            detail: detail.to_string(),
        }
    }
}

type Result<T> = std::result::Result<T, ProbeFailure>;

fn shared_format(
    format: wgpu::TextureFormat,
) -> Result<(
    wgpu::TextureFormat,
    vk::Format,
    smithay::backend::allocator::Fourcc,
)> {
    use smithay::backend::allocator::Fourcc;
    match format {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => Ok((
            wgpu::TextureFormat::Bgra8Unorm,
            vk::Format::B8G8R8A8_UNORM,
            Fourcc::Argb8888,
        )),
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => Ok((
            wgpu::TextureFormat::Rgba8Unorm,
            vk::Format::R8G8B8A8_UNORM,
            Fourcc::Abgr8888,
        )),
        wgpu::TextureFormat::Rgba16Float => Ok((
            wgpu::TextureFormat::Rgba16Float,
            vk::Format::R16G16B16A16_SFLOAT,
            Fourcc::Abgr16161616f,
        )),
        wgpu::TextureFormat::Rgb10a2Unorm => Ok((
            wgpu::TextureFormat::Rgb10a2Unorm,
            vk::Format::A2B10G10R10_UNORM_PACK32,
            Fourcc::Abgr2101010,
        )),
        _ => Err(ProbeFailure::at(
            "export_color_format",
            "This texture format has no supported shared color-buffer encoding",
        )),
    }
}

pub fn probe() -> ProbeReport {
    let mut report = ProbeReport {
        adapter: None,
        egl_device: None,
        completed: Vec::new(),
        failure: None,
        full_graphics_gate_passed: false,
    };
    if let Err(error) = run_probe(&mut report) {
        report.failure = Some(error);
    }
    report
}

#[cfg(feature = "graphics-gate")]
pub fn gate() -> ProbeReport {
    let mut report = probe();
    if report.failure.is_none() {
        if let Err(error) = canvas::run(&mut report) {
            report.failure = Some(error);
        } else {
            report.full_graphics_gate_passed = true;
        }
    }
    report
}

pub(crate) struct VulkanRenderer {
    #[cfg(feature = "native-presentation")]
    capture_bytes: Arc<std::sync::atomic::AtomicU64>,
    #[cfg(feature = "native-presentation")]
    waits: std::cell::RefCell<
        std::collections::HashMap<
            (
                smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
                u64,
            ),
            wait::Watch,
        >,
    >,
    #[cfg(feature = "native-presentation")]
    reactor: wait::Reactor,
    pub(crate) gpu: Arc<GpuContext>,
    _thread: std::marker::PhantomData<std::cell::Cell<()>>,
}

pub(crate) struct GpuContext {
    #[cfg(all(test, feature = "native-presentation"))]
    export_timestamps: Option<Arc<import::tests::timestamps::ExportTimestamps>>,
    completion: completion::Driver,
    pub(crate) _instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    raw_instance: ash::Instance,
    raw_device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue_family: u32,
}

impl std::ops::Deref for VulkanRenderer {
    type Target = GpuContext;

    fn deref(&self) -> &Self::Target {
        &self.gpu
    }
}

struct Allocation {
    image: vk::Image,
    memory: vk::DeviceMemory,
    raw_device: ash::Device,
    _device: wgpu::Device,
    #[cfg(all(test, feature = "nested"))]
    bytes: u64,
}

impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: This is the last owner. HAL textures and queued work retain their
        // own references, and `_device` keeps the allocating device live here.
        unsafe {
            self.raw_device.destroy_image(self.image, None);
            self.raw_device.free_memory(self.memory, None);
        }
    }
}

pub(crate) struct ExportedImage {
    completion: completion::Completion,
    pub(crate) dmabuf: Dmabuf,
    _allocation: Arc<Allocation>,
    #[cfg(all(test, feature = "nested"))]
    _direct_lease: Option<Arc<()>>,
}

impl ExportedImage {
    pub(crate) fn ready(&self) -> Result<bool> {
        self.completion.ready()
    }
    #[cfg(feature = "native-presentation")]
    pub(crate) fn pending_until(&self) -> Result<Option<std::time::Instant>> {
        self.completion.pending_until()
    }
    pub(crate) fn wait_for_probe(&self) -> Result<()> {
        self.completion.wait_for_probe()
    }
}

struct SharedImage {
    texture: wgpu::Texture,
    dmabuf: Dmabuf,
    allocation: Arc<Allocation>,
}

struct SharedAllocation {
    dmabuf: Dmabuf,
    allocation: Arc<Allocation>,
}

impl VulkanRenderer {
    #[cfg(feature = "native-presentation")]
    pub(crate) fn client_import_formats(&self) -> Vec<smithay::backend::allocator::Fourcc> {
        let mut formats = Vec::new();
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgb10a2Unorm,
            wgpu::TextureFormat::Rgba16Float,
        ] {
            let features = self.adapter.get_texture_format_features(format);
            if !features
                .allowed_usages
                .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
                || !features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
            {
                continue;
            }
            let Ok((_, vk_format, fourcc)) = shared_format(format) else {
                continue;
            };
            let mut modifier = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
                .drm_format_modifier(0)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
            let information = vk::PhysicalDeviceImageFormatInfo2::default()
                .format(vk_format)
                .ty(vk::ImageType::TYPE_2D)
                .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
                .usage(vk::ImageUsageFlags::TRANSFER_SRC)
                .push_next(&mut modifier)
                .push_next(&mut external);
            let mut external_properties = vk::ExternalImageFormatProperties::default();
            let supported = {
                let mut properties =
                    vk::ImageFormatProperties2::default().push_next(&mut external_properties);
                // SAFETY: The physical device belongs to this instance and both chains are live.
                unsafe {
                    self.raw_instance
                        .get_physical_device_image_format_properties2(
                            self.physical_device,
                            &information,
                            &mut properties,
                        )
                }
                .is_ok()
            };
            if supported
                && external_properties
                    .external_memory_properties
                    .external_memory_features
                    .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
                && external_properties
                    .external_memory_properties
                    .compatible_handle_types
                    .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            {
                formats.push(fourcc);
            }
        }
        formats
    }

    #[cfg(feature = "native-presentation")]
    pub(crate) fn linear_scene_support(&self) -> Result<()> {
        let format = self
            .adapter
            .get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let usages = wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING;
        if !format.allowed_usages.contains(usages)
            || !format.flags.contains(
                wgpu::TextureFormatFeatureFlags::FILTERABLE
                    | wgpu::TextureFormatFeatureFlags::BLENDABLE,
            )
        {
            return Err(ProbeFailure::at(
                "native_linear_color",
                "The Vulkan adapter does not support filtered, blendable RGBA16F scene targets",
            ));
        }
        let _probe = self.create_shared_image_format(1, 1, wgpu::TextureFormat::Rgba16Float)?;
        Ok(())
    }

    pub(crate) fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|error| ProbeFailure::at("vulkan_adapter", error))?;
        if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            return Err(ProbeFailure::at(
                "vulkan_adapter",
                "a hardware Vulkan adapter is required; software rendering is not a graphics proof",
            ));
        }
        // SAFETY: Only borrow the HAL instance. GpuContext retains its wgpu owner.
        let raw_instance = unsafe { instance.as_hal::<Vulkan>() }
            .ok_or_else(|| ProbeFailure::at("vulkan_instance", "Vulkan HAL unavailable"))?
            .shared_instance()
            .raw_instance()
            .clone();
        // SAFETY: Borrow the adapter for device creation without destroying its handle.
        let hal_adapter = unsafe { adapter.as_hal::<Vulkan>() }
            .ok_or_else(|| ProbeFailure::at("vulkan_adapter", "Vulkan HAL unavailable"))?;
        let descriptor = wgpu::DeviceDescriptor {
            required_features: adapter.features(),
            required_limits: adapter.limits(),
            // SAFETY: This native path opts into the risks of wgpu's advertised experimental features.
            experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        };
        // SAFETY: The live HAL adapter was selected from this instance.
        let available = unsafe {
            raw_instance.enumerate_device_extension_properties(hal_adapter.raw_physical_device())
        }
        .map_err(|error| ProbeFailure::at("vulkan_extensions", error))?;
        for required in EXTENSIONS {
            if !available
                .iter()
                .any(|extension| extension.extension_name_as_c_str().ok() == Some(required))
            {
                return Err(ProbeFailure::at(
                    "vulkan_extensions",
                    format!("missing {}", required.to_string_lossy()),
                ));
            }
        }
        // SAFETY: Use this adapter's features and limits. The callback only adds
        // extensions confirmed by enumeration and preserves the selected features.
        let open = unsafe {
            hal_adapter.open_with_callback(
                descriptor.required_features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(Box::new(|arguments| {
                    for extension in EXTENSIONS {
                        if !arguments.extensions.contains(&extension) {
                            arguments.extensions.push(extension);
                        }
                    }
                })),
            )
        }
        .map_err(|error| ProbeFailure::at("vulkan_device", error))?;
        let raw_device = open.device.raw_device().clone();
        let physical_device = open.device.raw_physical_device();
        let queue_family = open.device.queue_family_index();
        // SAFETY: `open` came from this adapter with the same feature set as `descriptor`.
        let (device, queue) =
            unsafe { adapter.create_device_from_hal::<Vulkan>(open, &descriptor) }
                .map_err(|error| ProbeFailure::at("wgpu_device", error))?;
        drop(hal_adapter);
        #[cfg(feature = "native-presentation")]
        let reactor = {
            let thread = std::thread::current();
            wait::Reactor::new(move || thread.unpark())
                .map_err(|error| ProbeFailure::at("client_acquire_events", error))?
        };
        Ok(Self {
            #[cfg(feature = "native-presentation")]
            capture_bytes: Default::default(),
            #[cfg(feature = "native-presentation")]
            waits: Default::default(),
            #[cfg(feature = "native-presentation")]
            reactor,
            gpu: Arc::new(GpuContext {
                #[cfg(all(test, feature = "native-presentation"))]
                export_timestamps: None,
                completion: completion::Driver::new(device.clone())?,
                _instance: instance,
                adapter,
                device,
                queue,
                raw_instance,
                raw_device,
                physical_device,
                queue_family,
            }),
            _thread: Default::default(),
        })
    }
}

impl GpuContext {
    fn create_shared_image(&self, width: u32, height: u32) -> Result<SharedImage> {
        self.create_shared_image_format(width, height, wgpu::TextureFormat::Bgra8Unorm)
    }

    fn create_shared_image_format(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<SharedImage> {
        let SharedAllocation { dmabuf, allocation } =
            self.allocate_shared_image(width, height, format)?;
        let (format, _, _) = shared_format(format)?;
        #[cfg(feature = "native-presentation")]
        let started = std::time::Instant::now();
        self.initialize(allocation.clone())?;
        #[cfg(feature = "native-presentation")]
        crate::runtime::trace_frame("shared-image-initialize", started);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        self.register_shared_image(SharedAllocation { dmabuf, allocation }, size, format)
    }

    fn allocate_shared_image(
        &self,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<SharedAllocation> {
        #[cfg(feature = "native-presentation")]
        let started = std::time::Instant::now();
        let (_, vk_format, fourcc) = shared_format(format)?;
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return Err(ProbeFailure::at(
                "image_dimensions",
                "invalid shared image dimensions",
            ));
        }
        let modifiers = [0_u64];
        let mut modifier_info =
            vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(&modifiers);
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier_info)
            .push_next(&mut external_info);
        // SAFETY: The device enables modifier and DMA-BUF support. The format,
        // dimensions, and live descriptor chain specify a single linear image.
        let image = unsafe { self.raw_device.create_image(&image_info, None) }
            .map_err(|error| ProbeFailure::at("exportable_image", error))?;
        // SAFETY: The image was just created on this device and is still owned here.
        let requirements = unsafe { self.raw_device.get_image_memory_requirements(image) };
        // SAFETY: This physical device belongs to the retained Vulkan instance.
        let properties = unsafe {
            self.raw_instance
                .get_physical_device_memory_properties(self.physical_device)
        };
        let memory_type = (0..properties.memory_type_count)
            .find(|index| requirements.memory_type_bits & (1 << index) != 0);
        let Some(memory_type) = memory_type else {
            // SAFETY: This fresh image has never been submitted or registered elsewhere.
            unsafe { self.raw_device.destroy_image(image, None) };
            return Err(ProbeFailure::at(
                "image_memory",
                "no compatible memory type",
            ));
        };
        let mut export_info = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type)
            .push_next(&mut export_info)
            .push_next(&mut dedicated);
        // SAFETY: The size and memory type come from the image's requirements,
        // and the allocation chain names that same image.
        let memory = match unsafe { self.raw_device.allocate_memory(&allocation_info, None) } {
            Ok(memory) => memory,
            Err(error) => {
                // SAFETY: Failed allocation leaves the fresh image unused and unbound.
                unsafe { self.raw_device.destroy_image(image, None) };
                return Err(ProbeFailure::at("image_memory", error));
            }
        };
        let allocation = Arc::new(Allocation {
            image,
            memory,
            raw_device: self.raw_device.clone(),
            _device: self.device.clone(),
            #[cfg(all(test, feature = "nested"))]
            bytes: requirements.size,
        });
        // SAFETY: Both handles belong to this device and the memory was dedicated
        // to this unbound image. Offset zero satisfies its alignment.
        unsafe { self.raw_device.bind_image_memory(image, memory, 0) }
            .map_err(|error| ProbeFailure::at("bind_image_memory", error))?;
        #[cfg(feature = "native-presentation")]
        crate::runtime::trace_frame("shared-image-memory", started);
        #[cfg(feature = "native-presentation")]
        let started = std::time::Instant::now();
        let memory_fd = khr::external_memory_fd::Device::new(&self.raw_instance, &self.raw_device);
        // SAFETY: The retained allocation was created with the same DMA-BUF handle type.
        let fd = unsafe {
            memory_fd.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
            )
        }
        .map_err(|error| ProbeFailure::at("export_dmabuf", error))?;
        // SAFETY: Successful export transfers a new, owned descriptor to this caller.
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
        // SAFETY: Query the only memory plane of the live single-plane linear image.
        let layout = unsafe {
            self.raw_device.get_image_subresource_layout(
                image,
                vk::ImageSubresource::default()
                    .aspect_mask(vk::ImageAspectFlags::MEMORY_PLANE_0_EXT),
            )
        };
        let offset = u32::try_from(layout.offset)
            .map_err(|error| ProbeFailure::at("dmabuf_offset", error))?;
        let stride = u32::try_from(layout.row_pitch)
            .map_err(|error| ProbeFailure::at("dmabuf_stride", error))?;
        let mut builder = Dmabuf::builder(
            (width as i32, height as i32),
            fourcc,
            smithay::backend::allocator::Modifier::Linear,
            DmabufFlags::empty(),
        );
        if !builder.add_plane(fd, 0, offset, stride) {
            return Err(ProbeFailure::at(
                "dmabuf_plane",
                "could not attach shared image plane",
            ));
        }
        let dmabuf = builder
            .build()
            .ok_or_else(|| ProbeFailure::at("dmabuf_plane", "could not build shared image"))?;
        #[cfg(feature = "native-presentation")]
        crate::runtime::trace_frame("shared-image-export-fd", started);
        Ok(SharedAllocation { dmabuf, allocation })
    }

    fn register_shared_image(
        &self,
        shared: SharedAllocation,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) -> Result<SharedImage> {
        let SharedAllocation { dmabuf, allocation } = shared;
        let image = allocation.image;
        #[cfg(feature = "native-presentation")]
        let started = std::time::Instant::now();
        // SAFETY: Borrow the HAL handle without destroying the device.
        let hal_device = unsafe { self.device.as_hal::<Vulkan>() }
            .ok_or_else(|| ProbeFailure::at("wgpu_texture", "Vulkan HAL unavailable"))?;
        let retained = allocation.clone();
        // SAFETY: The descriptor matches allocate_shared_image and the drop
        // callback retains both the external image and its dedicated memory.
        let hal_texture = unsafe {
            hal_device.texture_from_raw(
                image,
                &wgpu::hal::TextureDescriptor {
                    label: Some("park_shared_image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUses::COLOR_TARGET
                        | wgpu::TextureUses::RESOURCE
                        | wgpu::TextureUses::COPY_SRC
                        | wgpu::TextureUses::COPY_DST,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                Some(Box::new(move || drop(retained))),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };
        // SAFETY: The texture belongs to this device and matches the descriptor.
        // Callers submit initialization in TRANSFER_DST_OPTIMAL before registration.
        let texture = unsafe {
            self.device.create_texture_from_hal::<Vulkan>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("park_shared_image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::TextureUses::COPY_DST,
            )
        };
        #[cfg(feature = "native-presentation")]
        crate::runtime::trace_frame("shared-image-register", started);
        Ok(SharedImage {
            texture,
            dmabuf,
            allocation,
        })
    }

    fn submit_raw(
        &self,
        stage: &str,
        record: impl FnOnce(vk::CommandBuffer),
    ) -> Result<wgpu::SubmissionIndex> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(stage) });
        // SAFETY: Callers only record commands inside the HAL borrow. wgpu finishes
        // and owns the command buffer, and callers retain submitted resources.
        unsafe {
            encoder.as_hal_mut::<Vulkan, _, _>(|encoder| {
                let encoder =
                    encoder.ok_or_else(|| ProbeFailure::at(stage, "Vulkan HAL unavailable"))?;
                record(encoder.raw_handle());
                Ok::<_, ProbeFailure>(())
            })?;
        }
        Ok(self.queue.submit([encoder.finish()]))
    }

    fn retain(&self, resources: impl Send + 'static) {
        self.queue.on_submitted_work_done(move || drop(resources));
    }

    fn checked_submission(&self, submit: impl FnOnce() -> Result<()>) -> Result<()> {
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let result = submit();
        let errors = [
            pollster::block_on(internal.pop()),
            pollster::block_on(memory.pop()),
            pollster::block_on(validation.pop()),
        ];
        result?;
        if let Some(error) = errors.into_iter().flatten().next() {
            return Err(ProbeFailure::at("gpu_submission", error));
        }
        Ok(())
    }

    fn initialize(&self, allocation: Arc<Allocation>) -> Result<()> {
        self.submit_raw("initialize_image", |command| {
            self.encode_initialize(&allocation, command)
        })?;
        self.retain(allocation);
        Ok(())
    }

    fn encode_initialize(&self, allocation: &Allocation, command: vk::CommandBuffer) {
        let image = allocation.image;
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let barrier = vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        // SAFETY: Record into this device's active command buffer. The fresh image
        // supports transfer writes and the barrier establishes its layout before clearing it.
        unsafe {
            self.raw_device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            self.raw_device.cmd_clear_color_image(
                command,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [0.0; 4] },
                &[range],
            );
        }
    }

    fn release(
        &self,
        allocation: Arc<Allocation>,
        layout: vk::ImageLayout,
        access: vk::AccessFlags,
        stage: vk::PipelineStageFlags,
    ) -> Result<()> {
        self.checked_submission(|| {
            self.submit_raw("release_to_egl", |command| {
                self.encode_release(&allocation, layout, access, stage, command)
            })?;
            Ok(())
        })?;
        self.retain(allocation);
        Ok(())
    }

    fn encode_release(
        &self,
        allocation: &Allocation,
        layout: vk::ImageLayout,
        access: vk::AccessFlags,
        stage: vk::PipelineStageFlags,
        command: vk::CommandBuffer,
    ) {
        let image = allocation.image;
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let barrier = vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range)
            .old_layout(layout)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(self.queue_family)
            .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .src_access_mask(access);
        // SAFETY: The active command buffer is from the same queue as the image's
        // preceding use. The caller supplies that use's layout, access, and stage.
        unsafe {
            self.raw_device.cmd_pipeline_barrier(
                command,
                stage,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }
    }

    fn clear(&self, image: SharedImage, color: wgpu::Color) -> Result<ExportedImage> {
        let view = image.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("park_probe_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit([encoder.finish()]);
        self.release(
            image.allocation.clone(),
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        )?;
        Ok(ExportedImage {
            completion: self.completion.track(&self.queue)?,
            dmabuf: image.dmabuf,
            _allocation: image.allocation,
            #[cfg(all(test, feature = "nested"))]
            _direct_lease: None,
        })
    }
}

fn run_probe(report: &mut ProbeReport) -> Result<()> {
    let vulkan = VulkanRenderer::new()?;
    report.adapter = Some(vulkan.adapter.get_info().name);
    report
        .completed
        .push("hardware Vulkan/wgpu device with DMA-BUF extensions".into());
    let egl_device = EGLDevice::enumerate()
        .map_err(|error| ProbeFailure::at("egl_device", error))?
        .find(|device| !device.is_software())
        .ok_or_else(|| ProbeFailure::at("egl_device", "no hardware EGL device"))?;
    report.egl_device = Some(
        egl_device
            .render_device_path()
            .map_err(|error| ProbeFailure::at("egl_device", error))?
            .display()
            .to_string(),
    );
    // SAFETY: Smithay owns this display lifetime and no raw caller terminates it.
    let display = unsafe { EGLDisplay::new(egl_device) }
        .map_err(|error| ProbeFailure::at("egl_display", error))?;
    let context =
        EGLContext::new(&display).map_err(|error| ProbeFailure::at("egl_context", error))?;
    // SAFETY: The context was created here and has never been bound to another thread.
    let mut renderer = unsafe { GlesRenderer::new(context) }
        .map_err(|error| ProbeFailure::at("gles_renderer", error))?;
    report.completed.push("hardware EGL/GLES renderer".into());
    for (width, height) in [(64, 32), (96, 48)] {
        let image = vulkan.create_shared_image(width, height)?;
        let image = vulkan.clear(
            image,
            wgpu::Color {
                r: 1.0,
                g: 0.25,
                b: 0.0,
                a: 1.0,
            },
        )?;
        image.wait_for_probe()?;
        let client_texture = vulkan.import_dmabuf(&image.dmabuf)?;
        let image = vulkan.export_copy(&client_texture)?;
        image.wait_for_probe()?;
        let imported = renderer
            .import_dmabuf(&image.dmabuf, None)
            .map_err(|error| ProbeFailure::at("gles_import", error))?;
        let size = smithay::utils::Size::<i32, smithay::utils::Physical>::from((
            width as i32,
            height as i32,
        ));
        let buffer_size = (width as i32, height as i32).into();
        let mut target: GlesRenderbuffer = renderer
            .create_buffer(smithay::backend::allocator::Fourcc::Abgr8888, buffer_size)
            .map_err(|error| ProbeFailure::at("gles_target", error))?;
        {
            let mut framebuffer = renderer
                .bind(&mut target)
                .map_err(|error| ProbeFailure::at("gles_framebuffer", error))?;
            let mut frame = renderer
                .render(&mut framebuffer, size, smithay::utils::Transform::Normal)
                .map_err(|error| ProbeFailure::at("gles_frame", error))?;
            let rectangle = smithay::utils::Rectangle::from_size(size);
            frame
                .render_texture_at(
                    &imported,
                    (0, 0).into(),
                    1,
                    1.0,
                    smithay::utils::Transform::Normal,
                    &[rectangle],
                    &[],
                    1.0,
                )
                .map_err(|error| ProbeFailure::at("gles_draw", error))?;
            frame
                .finish()
                .map_err(|error| ProbeFailure::at("gles_finish", error))?
                .wait()
                .map_err(|error| ProbeFailure::at("gles_sync", error))?;
        }
        let mut framebuffer = renderer
            .bind(&mut target)
            .map_err(|error| ProbeFailure::at("gles_readback", error))?;
        let mapping = renderer
            .copy_framebuffer(
                &mut framebuffer,
                smithay::utils::Rectangle::from_size(buffer_size),
                smithay::backend::allocator::Fourcc::Abgr8888,
            )
            .map_err(|error| ProbeFailure::at("gles_readback", error))?;
        let bytes = renderer
            .map_texture(&mapping)
            .map_err(|error| ProbeFailure::at("gles_readback", error))?;
        let pixel = bytes
            .get(..4)
            .ok_or_else(|| ProbeFailure::at("pixel_validation", "empty readback"))?;
        if pixel[0] < 250 || !(60..=68).contains(&pixel[1]) || pixel[2] > 5 || pixel[3] < 250 {
            return Err(ProbeFailure::at(
                "pixel_validation",
                format!("expected RGBA [255,64,0,255], got {pixel:?}"),
            ));
        }
        report.completed.push(format!(
            "Vulkan -> DMA-BUF -> GLES pixels at {width}x{height}"
        ));
    }
    Ok(())
}
