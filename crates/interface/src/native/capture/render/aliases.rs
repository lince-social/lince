use super::*;
use crate::native::topology::surface_render::{CaptureOrigins, CompletedCaptures};
use bevy::{
    asset::AssetId,
    core_pipeline::blit::{BlitPipeline, BlitPipelineKey},
    render::{
        render_asset::RenderAssets,
        render_resource::{CachedRenderPipelineId, LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, SpecializedRenderPipelines, StoreOp},
        renderer::RenderContext,
        texture::GpuImage,
    },
};
use std::collections::BTreeMap;

#[derive(Resource, Default)]
pub(crate) struct ExternalAliases(pub BTreeMap<AssetId<Image>, BTreeSet<AssetId<Image>>>);

#[derive(Resource, Default)]
pub(crate) struct PreparedAliases(BTreeMap<AssetId<Image>, Vec<(AssetId<Image>, CachedRenderPipelineId)>>);

pub(crate) fn prepare(
    aliases: Option<Res<ExternalAliases>>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    blit: Res<BlitPipeline>,
    mut prepared: ResMut<PreparedAliases>,
) {
    prepared.0.clear();
    let Some(aliases) = aliases else { return; };
    for (source, targets) in &aliases.0 {
        if images.get(*source).is_none() { continue; }
        for target in targets {
            let Some(image) = images.get(*target) else { continue; };
            let pipeline = pipelines.specialize(&cache, &blit, BlitPipelineKey {
                target_format: image.texture_view_descriptor.as_ref().and_then(|view| view.format).unwrap_or(image.texture.format()),
                blend_state: None, samples: 1, source_space: None,
            });
            prepared.0.entry(*source).or_default().push((*target, pipeline));
        }
    }
}

pub(crate) fn copy(world: &World, context: &mut RenderContext, source: AssetId<Image>, origins: &CaptureOrigins, order: (isize, usize)) {
    let Some(prepared) = world.get_resource::<PreparedAliases>().and_then(|prepared| prepared.0.get(&source)) else { return; };
    let images = world.resource::<RenderAssets<GpuImage>>();
    let Some(image) = images.get(source) else { return; };
    let cache = world.resource::<PipelineCache>();
    let blit = world.resource::<BlitPipeline>();
    let group = blit.create_bind_group(context.render_device(), &image.texture_view, cache);
    for (target, pipeline) in prepared {
        let Some(pipeline) = cache.get_render_pipeline(*pipeline) else { continue; };
        let Some(target_image) = images.get(*target) else { continue; };
        if image.texture.id() == target_image.texture.id() { continue; }
        let attachments = [Some(RenderPassColorAttachment {
            view: &target_image.texture_view, depth_slice: None, resolve_target: None,
            ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store },
        })];
        let mut pass = context.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("park.capture.displayed-alias"), color_attachments: &attachments,
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        world.resource::<CompletedCaptures>().queue(*target, order, true, origins.clone());
    }
}
