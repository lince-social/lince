use super::*;
use bevy::{
    core_pipeline::prepass::ViewPrepassTextures,
    ecs::{component::Component, entity::EntityHashMap},
    pbr::{DirectionalLightViewEntities, ExtractedDirectionalLight, ExtractedPointLight, LightMeta, MeshViewBindGroup, PointAndSpotLightViewEntities, ShadowView, ViewLightEntities, ViewLightsUniformOffset, ViewShadowBindings, ViewTransmissionTexture},
    render::{Render, texture::TextureCache, view::{ViewDepthTexture, ViewTarget, ViewTargetAttachments}},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const MAX_TARGET_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TARGETS: usize = 1024;

struct Images(Vec<(wgpu::Texture, wgpu::Texture)>);

impl Images {
    fn save(renderer: &VulkanRenderer, textures: Vec<(wgpu::Texture, bool)>) -> Result<Self, String> {
        let mut bytes = 0u64;
        if textures.len() > MAX_TARGETS { return Err("The capture render-target limit was exceeded.".into()); }
        for (texture, _) in &textures {
            let required = wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT;
            if texture.dimension() != wgpu::TextureDimension::D2 || texture.depth_or_array_layers() != 1 || texture.sample_count() != 1 || texture.mip_level_count() != 1 || texture.format().is_depth_stencil_format() || !texture.usage().contains(required) {
                return Err("A render target is not ready for isolated capture.".into());
            }
            let block = texture.format().block_copy_size(None).ok_or("The render target's color format cannot be isolated for capture.")?;
            bytes = bytes.checked_add(u64::from(texture.width()) * u64::from(texture.height()) * u64::from(block)).ok_or("Capture render-target accounting overflowed.")?;
        }
        if bytes > MAX_TARGET_BYTES { return Err("The isolated capture render-target budget was exceeded.".into()); }
        let mut saved = Vec::new();
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("park.capture.save-and-clear") });
        for (texture, clear) in textures {
            let backup = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("park.capture.owner-backup"), size: texture.size(), mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: texture.format(), usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST, view_formats: &[],
            });
            encoder.copy_texture_to_texture(texture.as_image_copy(), backup.as_image_copy(), texture.size());
            if clear {
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("park.capture.clear-derived-pixels"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view, depth_slice: None, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                }));
            }
            saved.push((texture, backup));
        }
        renderer.queue.submit([encoder.finish()]);
        Ok(Self(saved))
    }

    fn restore(self, renderer: &VulkanRenderer) {
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("park.capture.restore-owner") });
        for (texture, backup) in &self.0 { encoder.copy_texture_to_texture(backup.as_image_copy(), texture.as_image_copy(), texture.size()); }
        renderer.queue.submit([encoder.finish()]);
        renderer.queue.on_submitted_work_done(move || drop(self));
    }
}

fn take<T: Component>(world: &mut World) -> Vec<(Entity, T)> {
    let entities: Vec<_> = world.query_filtered::<Entity, With<T>>().iter(world).collect();
    entities.into_iter().filter_map(|entity| world.entity_mut(entity).take::<T>().map(|value| (entity, value))).collect()
}

fn restore<T: Component>(world: &mut World, saved: Vec<(Entity, T)>) {
    for (entity, value) in saved { if let Ok(mut entity) = world.get_entity_mut(entity) { entity.insert(value); } }
}

struct Views {
    textures: TextureCache,
    attachments: ViewTargetAttachments,
    targets: Vec<(Entity, ViewTarget)>,
    depth: Vec<(Entity, ViewDepthTexture)>,
    prepass: Vec<(Entity, ViewPrepassTextures)>,
    transmission: Vec<(Entity, ViewTransmissionTexture)>,
    mesh_bindings: Vec<(Entity, MeshViewBindGroup)>,
    shadows: Vec<(Entity, ViewShadowBindings)>,
    shadow_views: Vec<(Entity, ShadowView)>,
    light_views: Vec<(Entity, ViewLightEntities)>,
    light_offsets: Vec<(Entity, ViewLightsUniformOffset)>,
    point_views: Vec<(Entity, Vec<Entity>)>,
    directional_views: Vec<(Entity, EntityHashMap<Vec<Entity>>)>,
    lights: Option<LightMeta>,
    cache: native::capture::render::Cache,
}

impl Views {
    fn isolate(world: &mut World) -> Self {
        let textures = world.remove_resource::<TextureCache>().unwrap_or_default();
        let attachments = world.remove_resource::<ViewTargetAttachments>().unwrap_or_default();
        world.insert_resource(TextureCache::default());
        world.insert_resource(ViewTargetAttachments::default());
        let point_views = world.query::<(Entity, &mut PointAndSpotLightViewEntities)>().iter_mut(world).map(|(entity, mut views)| (entity, std::mem::take(&mut **views))).collect();
        let directional_views = world.query::<(Entity, &mut DirectionalLightViewEntities)>().iter_mut(world).map(|(entity, mut views)| (entity, std::mem::take(&mut **views))).collect();
        let lights = world.remove_resource::<LightMeta>();
        if lights.is_some() { world.insert_resource(LightMeta::default()); }
        Self { textures, attachments, targets: take(world), depth: take(world), prepass: take(world), transmission: take(world), mesh_bindings: take(world), shadows: take(world), shadow_views: take(world), light_views: take(world), light_offsets: take(world), point_views, directional_views, lights, cache: native::capture::render::begin_cache(world) }
    }

    fn restore(self, world: &mut World) {
        let _ = take::<ViewTarget>(world);
        let _ = take::<ViewDepthTexture>(world);
        let _ = take::<ViewPrepassTextures>(world);
        let _ = take::<ViewTransmissionTexture>(world);
        let _ = take::<MeshViewBindGroup>(world);
        let _ = take::<ViewShadowBindings>(world);
        let _ = take::<ViewLightEntities>(world);
        let _ = take::<ViewLightsUniformOffset>(world);
        let mut fresh: BTreeSet<_> = world.query::<&PointAndSpotLightViewEntities>().iter(world).flat_map(|views| views.iter().copied()).collect();
        fresh.extend(world.query::<&DirectionalLightViewEntities>().iter(world).flat_map(|views| views.values().flatten().copied()));
        for entity in fresh { if world.get_entity(entity).is_ok() { world.despawn(entity); } }
        for (entity, views) in self.point_views {
            if world.get::<ExtractedPointLight>(entity).is_none() {
                for view in views { if world.get_entity(view).is_ok() { world.despawn(view); } }
            } else if let Some(mut current) = world.get_mut::<PointAndSpotLightViewEntities>(entity) { **current = views; }
        }
        for (entity, views) in self.directional_views {
            if world.get::<ExtractedDirectionalLight>(entity).is_none() {
                for view in views.values().flatten().copied() { if world.get_entity(view).is_ok() { world.despawn(view); } }
            } else if let Some(mut current) = world.get_mut::<DirectionalLightViewEntities>(entity) { **current = views; }
        }
        if let Some(lights) = self.lights { world.insert_resource(lights); }
        world.insert_resource(self.textures);
        world.insert_resource(self.attachments);
        restore(world, self.targets);
        restore(world, self.depth);
        restore(world, self.prepass);
        restore(world, self.transmission);
        restore(world, self.mesh_bindings);
        restore(world, self.shadows);
        restore(world, self.shadow_views);
        restore(world, self.light_views);
        restore(world, self.light_offsets);
        native::capture::render::end_cache(world, self.cache);
    }
}

impl Presentation {
    fn capture_targets(&mut self) -> Result<BTreeSet<bevy::asset::AssetId<Image>>, &'static str> {
        native::capture::render::target_ids(self.app.world_mut())
    }

    pub(in crate::presentation) fn prepare_capture_targets(&mut self) {
        if self.app.world().get_resource::<lince_interface::host::capture::Demand>().is_none_or(|demand| demand.outputs.is_empty()) { return; }
        let Ok(targets) = self.capture_targets() else {
            if let Some(gate) = self.app.world().get_resource::<PolicyGate>() { gate.invalidate(); }
            return;
        };
        let mut images = self.app.world_mut().resource_mut::<Assets<Image>>();
        let required = wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;
        for target in targets {
            if images.get(target).is_some_and(|image| !image.texture_descriptor.usage.contains(required)) {
                images.get_mut(target).unwrap().texture_descriptor.usage |= required;
            }
        }
    }

    pub(super) fn filtered_capture(&mut self, revision: u64, policy: &Policy, demand: &lince_interface::host::capture::Demand) -> Result<(BTreeMap<lince_interface::host::OutputId, ExportedImage>, bool), String> {
        if self.app.get_sub_app(RenderApp).is_none() { return Err("The native render app is unavailable.".into()); }
        let additional = self.capture_private_roots(policy);
        let targets = native::capture::render::Targets::collect(self.app.world_mut(), &additional).map_err(str::to_owned)?;
        let textures = {
            let images = self.app.sub_app(RenderApp).world().resource::<RenderAssets<GpuImage>>();
            let mut textures = HashMap::new();
            for target in &targets.images {
                if let Some(image) = images.get(*target) {
                    let clear = !targets.reusable.contains(target);
                    textures.entry(image.texture.id())
                        .and_modify(|(_, required): &mut (wgpu::Texture, bool)| *required |= clear)
                        .or_insert_with(|| ((*image.texture).clone(), clear));
                }
            }
            textures.into_values().collect()
        };
        let native_private_focus = self.app.world().get_resource::<bevy::input_focus::InputFocus>().and_then(bevy::input_focus::InputFocus::get)
            .map_or(!policy.excluded.is_empty(), |entity| native::capture::excluded_with(self.app.world(), entity, &additional));
        let mut filter = native::capture::render::Filter::apply(self.app.world_mut(), &additional).map_err(str::to_owned)?;
        targets.refresh(&mut filter, self.app.world_mut());
        let saved = match Images::save(&self.renderer, textures) {
            Ok(saved) => saved,
            Err(error) => { filter.restore(self.app.world_mut()); return Err(error); }
        };
        let mut render = self.app.remove_sub_app(RenderApp).expect("validated native render app");
        let views = Views::isolate(render.world_mut());
        targets.seed(render.world_mut());
        render.world_mut().insert_resource(native::capture::render::ExternalPass);
        render.extract(self.app.world_mut());
        render.world_mut().run_schedule(Render);
        let result = (|| {
            let gate = self.app.world().get_resource::<PolicyGate>().ok_or("Capture policy was retired.")?;
            if !gate.read().is_some_and(|policy| policy.revision() == revision) { return Err("Capture policy changed during rendering.".into()); }
            let images = render.world().resource::<RenderAssets<GpuImage>>();
            let mut exported = BTreeMap::new();
            let mut roots = Vec::new();
            for output in &demand.outputs {
                let target = if *output == self.output_state.assignments.focused().unwrap_or(lince_interface::host::OutputId(1)) { &self.target }
                    else { &self.output_state.targets.get(output).ok_or("The requested capture output is unavailable.")?.image };
                roots.push(target.id());
            }
            targets.validate(self.app.world(), render.world(), &roots, &additional).map_err(str::to_owned)?;
            for (output, target) in demand.outputs.iter().zip(roots) {
                let image = images.get(target).ok_or("The capture output image is not prepared.")?;
                exported.insert(*output, self.renderer.export_copy(&image.texture).map_err(|error| error.detail)?);
            }
            Ok(exported)
        })();
        views.restore(render.world_mut());
        let _ = render.world_mut().remove_resource::<native::capture::render::ExternalPass>();
        render.world_mut().clear_trackers();
        self.app.insert_sub_app(RenderApp, render);
        filter.restore(self.app.world_mut());
        saved.restore(&self.renderer);
        result.map(|images| (images, native_private_focus))
    }
}
