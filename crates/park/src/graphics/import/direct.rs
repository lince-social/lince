use super::*;

#[derive(bevy::prelude::Resource)]
pub(crate) struct Prepared {
    images: Vec<SharedImage>,
}

impl Prepared {
    pub(crate) fn new(gpu: &GpuContext, sources: &[wgpu::Texture]) -> Result<Self> {
        if sources.is_empty() {
            return Ok(Self { images: Vec::new() });
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
        Ok(Self { images })
    }

    pub(crate) fn texture(&self, index: usize) -> &wgpu::Texture {
        &self.images[index].texture
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
        Ok(self
            .images
            .into_iter()
            .map(|image| ExportedImage {
                completion: completion.clone(),
                dmabuf: image.dmabuf,
                _allocation: image.allocation,
            })
            .collect())
    }
}
