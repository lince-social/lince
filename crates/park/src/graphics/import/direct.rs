use super::*;
use std::sync::{Mutex, Weak};

mod pool_tests;

#[derive(Clone)]
struct Retirement {
    completion: completion::Completion,
    reusable: bool,
}

#[derive(bevy::prelude::Resource)]
pub(crate) struct Prepared {
    images: Vec<SharedImage>,
    views: Vec<wgpu::TextureView>,
    lease: Option<Arc<()>>,
    retirement: Option<Arc<Mutex<Option<Retirement>>>>,
}

impl Prepared {
    pub(crate) fn new(gpu: &GpuContext, sources: &[wgpu::Texture]) -> Result<Self> {
        if sources.is_empty() {
            return Ok(Self {
                images: Vec::new(),
                views: Vec::new(),
                lease: None,
                retirement: None,
            });
        }
        let mut allocations = Vec::new();
        for source in sources {
            if source.format().is_srgb() {
                return Err(ProbeFailure::at(
                    "direct_target",
                    "The prototype requires a linear attachment format",
                ));
            }
            allocations.push(gpu.allocate_export_target(source)?);
        }
        gpu.submit_raw("direct_target_initialize", |command| {
            for allocation in &allocations {
                gpu.encode_initialize(&allocation.allocation, command);
            }
        })?;
        gpu.retain(
            allocations
                .iter()
                .map(|allocation| allocation.allocation.clone())
                .collect::<Vec<_>>(),
        );
        let images = allocations
            .into_iter()
            .zip(sources)
            .map(|(allocation, source)| {
                gpu.register_shared_image(allocation, source.size(), source.format())
            })
            .collect::<Result<Vec<_>>>()?;
        let views = images
            .iter()
            .map(|image| image.texture.create_view(&Default::default()))
            .collect();
        Ok(Self {
            images,
            views,
            lease: None,
            retirement: None,
        })
    }

    pub(crate) fn texture(&self, index: usize) -> &wgpu::Texture {
        &self.images[index].texture
    }

    pub(crate) fn view(&self, index: usize) -> &wgpu::TextureView {
        &self.views[index]
    }

    pub(crate) fn dmabuf(&self, index: usize) -> &Dmabuf {
        &self.images[index].dmabuf
    }

    pub(crate) fn export(
        self,
        gpu: &GpuContext,
        sources: &[wgpu::Texture],
    ) -> Result<Vec<ExportedImage>> {
        if self.images.len() != sources.len()
            || self
                .images
                .iter()
                .zip(sources)
                .any(|(image, source)| &image.texture != source)
        {
            return Err(ProbeFailure::at(
                "direct_target",
                "The prepared target epoch does not match the exported images",
            ));
        }
        if self.images.is_empty() {
            return Ok(Vec::new());
        }
        let mut prepare = gpu.device.create_command_encoder(&Default::default());
        for image in &self.images {
            gpu.encode_export_source(&image.texture, &mut prepare);
        }
        let prepare = prepare.finish();
        let mut release = gpu.device.create_command_encoder(&Default::default());
        unsafe {
            release.as_hal_mut::<Vulkan, _, _>(|encoder| {
                let encoder = encoder
                    .ok_or_else(|| ProbeFailure::at("direct_target", "Vulkan HAL unavailable"))?;
                for image in &self.images {
                    gpu.encode_release(
                        &image.allocation,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::AccessFlags::TRANSFER_READ,
                        vk::PipelineStageFlags::TRANSFER,
                        encoder.raw_handle(),
                    );
                }
                Ok::<_, ProbeFailure>(())
            })?;
        }
        let completion = gpu.submit_exports(
            [prepare, release.finish()],
            self.images
                .iter()
                .map(|image| (image.texture.clone(), image.allocation.clone()))
                .collect::<Vec<_>>(),
        )?;
        if let Some(retirement) = &self.retirement {
            *retirement.lock().unwrap() = Some(Retirement {
                completion: completion.clone(),
                reusable: true,
            });
        }
        let lease = self.lease;
        Ok(self
            .images
            .into_iter()
            .map(|image| ExportedImage {
                completion: completion.clone(),
                dmabuf: image.dmabuf,
                _allocation: image.allocation,
                _direct_lease: lease.clone(),
            })
            .collect())
    }
}

struct Entry {
    images: Vec<SharedImage>,
    views: Vec<wgpu::TextureView>,
    key: Vec<(u32, u32, wgpu::TextureFormat)>,
    lease: Weak<()>,
    retirement: Arc<Mutex<Option<Retirement>>>,
    bytes: u64,
}

pub(crate) struct Pool {
    entries: Vec<Entry>,
    capacity: usize,
    budget: u64,
    allocations: u64,
    reuses: u64,
    misses: u64,
    discarded: u64,
}

fn clone_images(images: &[SharedImage]) -> Vec<SharedImage> {
    images
        .iter()
        .map(|image| SharedImage {
            texture: image.texture.clone(),
            dmabuf: image.dmabuf.clone(),
            allocation: image.allocation.clone(),
        })
        .collect()
}

impl Pool {
    pub(crate) fn new(capacity: usize, budget: u64) -> Self {
        assert!(capacity > 0 && budget > 0);
        Self {
            entries: Vec::new(),
            capacity,
            budget,
            allocations: 0,
            reuses: 0,
            misses: 0,
            discarded: 0,
        }
    }

    pub(crate) fn contains(&self, buffer: &Dmabuf) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.images.iter().any(|image| &image.dmabuf == buffer))
    }

    pub(crate) fn stats(&self) -> [u64; 6] {
        [
            self.entries.len() as u64,
            self.entries.iter().map(|entry| entry.bytes).sum(),
            self.allocations,
            self.reuses,
            self.misses,
            self.discarded,
        ]
    }

    pub(crate) fn acquire(
        &mut self,
        gpu: &GpuContext,
        sources: &[wgpu::Texture],
    ) -> Result<Option<Prepared>> {
        if sources.is_empty() {
            return Prepared::new(gpu, sources).map(Some);
        }
        if sources
            .iter()
            .any(|source| !render_target_source(source) || source.format().is_srgb())
        {
            return Err(ProbeFailure::at(
                "direct_pool",
                "The pool requires copyable linear single-layer render targets",
            ));
        }
        let key = sources
            .iter()
            .map(|source| (source.width(), source.height(), source.format()))
            .collect::<Vec<_>>();
        let mut retained = Vec::new();
        for entry in self.entries.drain(..) {
            let mut state = entry.retirement.lock().unwrap().clone();
            let unleased = entry.lease.upgrade().is_none();
            if unleased && state.is_none() {
                let completion = gpu.completion.track(gpu.queue.submit([]))?;
                state = Some(Retirement {
                    completion,
                    reusable: false,
                });
                *entry.retirement.lock().unwrap() = state.clone();
            }
            if unleased
                && state
                    .as_ref()
                    .is_some_and(|state| !state.reusable || entry.key != key)
                && state.as_ref().unwrap().completion.ready()?
            {
                self.discarded += 1;
            } else {
                retained.push(entry);
            }
        }
        self.entries = retained;
        let mut available = None;
        for (index, entry) in self.entries.iter().enumerate() {
            let completion = entry.retirement.lock().unwrap().clone();
            if entry.key == key
                && entry.lease.upgrade().is_none()
                && completion.is_some()
                && completion.as_ref().unwrap().reusable
                && completion.as_ref().unwrap().completion.ready()?
            {
                available = Some(index);
                break;
            }
        }
        if let Some(index) = available {
            let entry = &mut self.entries[index];
            let lease = Arc::new(());
            entry.lease = Arc::downgrade(&lease);
            *entry.retirement.lock().unwrap() = None;
            gpu.submit_raw("direct_target_reacquire", |command| {
                for image in &entry.images {
                    let range = vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1);
                    let acquire = vk::ImageMemoryBarrier::default()
                        .image(image.allocation.image)
                        .subresource_range(range)
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                        .dst_queue_family_index(gpu.queue_family)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
                    let restore = vk::ImageMemoryBarrier::default()
                        .image(image.allocation.image)
                        .subresource_range(range)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
                    unsafe {
                        gpu.raw_device.cmd_pipeline_barrier(
                            command,
                            vk::PipelineStageFlags::TOP_OF_PIPE,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::DependencyFlags::empty(),
                            &[],
                            &[],
                            &[acquire],
                        );
                        gpu.raw_device.cmd_clear_color_image(
                            command,
                            image.allocation.image,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            &vk::ClearColorValue { float32: [0.0; 4] },
                            &[range],
                        );
                        gpu.raw_device.cmd_pipeline_barrier(
                            command,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::DependencyFlags::empty(),
                            &[],
                            &[],
                            &[restore],
                        );
                    }
                }
            })?;
            gpu.retain(
                entry
                    .images
                    .iter()
                    .map(|image| image.allocation.clone())
                    .collect::<Vec<_>>(),
            );
            self.reuses += 1;
            return Ok(Some(Prepared {
                images: clone_images(&entry.images),
                views: entry.views.clone(),
                lease: Some(lease),
                retirement: Some(entry.retirement.clone()),
            }));
        }
        let used = self.entries.iter().map(|entry| entry.bytes).sum::<u64>();
        let logical = sources
            .iter()
            .map(|source| {
                u64::from(source.width())
                    * u64::from(source.height())
                    * u64::from(source.format().block_copy_size(None).unwrap())
            })
            .sum::<u64>();
        if self.entries.len() >= self.capacity
            || used
                .checked_add(logical)
                .is_none_or(|bytes| bytes > self.budget)
        {
            self.misses += 1;
            return Ok(None);
        }
        let mut prepared = Prepared::new(gpu, sources)?;
        let bytes = prepared
            .images
            .iter()
            .map(|image| image.allocation.bytes)
            .sum::<u64>();
        if used
            .checked_add(bytes)
            .is_none_or(|bytes| bytes > self.budget)
        {
            self.misses += 1;
            return Ok(None);
        }
        let lease = Arc::new(());
        let retirement = Arc::new(Mutex::new(None));
        self.entries.push(Entry {
            images: clone_images(&prepared.images),
            views: prepared.views.clone(),
            key,
            lease: Arc::downgrade(&lease),
            retirement: retirement.clone(),
            bytes,
        });
        prepared.lease = Some(lease);
        prepared.retirement = Some(retirement);
        self.allocations += 1;
        Ok(Some(prepared))
    }
}
