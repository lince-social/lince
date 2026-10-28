use super::*;
use crate::native::topology::surface_render::{CaptureOrigins, CompletedCaptures, SurfaceCapture};
use bevy::asset::AssetId;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy)]
struct Writer {
    entity: Entity,
    source: Option<Entity>,
}

pub struct Targets {
    pub images: BTreeSet<AssetId<Image>>,
    pub reusable: BTreeSet<AssetId<Image>>,
    pub private_entities: BTreeSet<u64>,
    pub unresolved: bool,
    redacted: BTreeSet<AssetId<Image>>,
    seeds: BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>,
    refresh: BTreeSet<Entity>,
    aliases: BTreeMap<AssetId<Image>, BTreeSet<AssetId<Image>>>,
}

pub fn target_ids(world: &mut World) -> Result<BTreeSet<AssetId<Image>>, &'static str> {
    let mut targets: BTreeSet<_> = world.query_filtered::<&RenderTarget, (With<Camera>, Allow<Disabled>)>().iter(world)
        .filter_map(|target| match target { RenderTarget::Image(image) => Some(image.handle.id()), _ => None }).collect();
    targets.extend(crate::native::topology::presentation::capture_images(world));
    if let Some(completed) = world.get_resource::<CompletedCaptures>() {
        targets.extend(completed.snapshot().ok_or("The retained capture provenance is unavailable.")?.into_keys());
    }
    Ok(targets)
}

struct Audit<'a> {
    world: &'a World,
    additional: &'a BTreeSet<Entity>,
    history: &'a BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>,
    writers: &'a BTreeMap<AssetId<Image>, Vec<Writer>>,
    private: BTreeSet<u64>,
    unresolved: bool,
}

impl Audit<'_> {
    fn entity(&mut self, entity: Entity) -> bool {
        if super::super::excluded_with(self.world, entity, self.additional) {
            self.private.extend(super::super::semantic_sources(self.world, entity).into_iter().map(Entity::to_bits));
            false
        } else { true }
    }

    fn image(&mut self, image: AssetId<Image>) -> bool {
        let mut public = true;
        let mut known = false;
        if let Some(writers) = self.writers.get(&image).cloned() {
            for writer in writers {
                public &= self.entity(writer.entity);
                if let Some(source) = writer.source { public &= self.entity(source); known = true; }
                else if !self.history.contains_key(&image) { public = false; self.unresolved = true; }
            }
        }
        if let Some(origins) = self.history.get(&image).cloned() {
            known = true;
            if !origins.complete { public = false; self.unresolved = true; }
            for entity in &origins.contributors { public &= self.entity(*entity); }
        }
        if !known { public = false; self.unresolved = true; }
        public
    }
}

impl Targets {
    pub fn collect(world: &mut World, additional: &BTreeSet<Entity>) -> Result<Self, &'static str> {
        let images = target_ids(world)?;
        let history = world.get_resource::<CompletedCaptures>()
            .map_or_else(|| Some(BTreeMap::new()), CompletedCaptures::snapshot)
            .ok_or("The retained capture provenance is unavailable.")?;
        let mut writers: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (entity, target, source) in world.query_filtered::<(Entity, &RenderTarget, Option<&TargetSource>), (With<Camera>, Allow<Disabled>)>().iter(world) {
            if let RenderTarget::Image(image) = target {
                writers.entry(image.handle.id()).or_default().push(Writer { entity, source: source.map(|source| source.0) });
            }
        }
        let mut audit = Audit { world, additional, history: &history, writers: &writers, private: BTreeSet::new(), unresolved: false };
        let mut reusable = BTreeSet::new();
        let mut redacted = BTreeSet::new();
        let mut seeds = BTreeMap::new();
        let mut refresh = BTreeSet::new();
        for image in &images {
            if audit.image(*image) {
                reusable.insert(*image);
                let origins = history.get(image).cloned().unwrap_or_else(|| Arc::new(CaptureOrigins {
                    contributors: writers.get(image).into_iter().flatten().flat_map(|writer| std::iter::once(writer.entity).chain(writer.source)).collect(),
                    images: BTreeSet::new(), complete: true,
                }));
                seeds.insert(*image, origins);
            }
            if writers.get(image).is_some_and(|writers| !writers.is_empty()
                && writers.iter().all(|writer| super::super::excluded_with(world, writer.entity, additional)))
                || history.get(image).is_some_and(|origins| origins.complete && !origins.contributors.is_empty()
                    && origins.contributors.iter().all(|entity| super::super::excluded_with(world, *entity, additional)))
                    && writers.get(image).is_none_or(|writers| writers.iter().all(|writer| super::super::excluded_with(world, writer.entity, additional)))
            {
                reusable.remove(image);
                redacted.insert(*image);
                seeds.insert(*image, Arc::new(CaptureOrigins { complete: true, ..default() }));
            }
            if !reusable.contains(image) {
                for writer in writers.get(image).into_iter().flatten() {
                    if world.get::<SurfaceCapture>(writer.entity).is_some()
                        && !super::super::excluded_with(world, writer.entity, additional)
                        && world.get::<Camera>(writer.entity).is_some_and(|camera| !camera.is_active
                            && camera.physical_target_size().is_some_and(|size| size.min_element() > 0))
                    { refresh.insert(writer.entity); }
                }
            }
        }
        let private_entities = audit.private;
        let unresolved = audit.unresolved;
        let mut candidates: BTreeMap<_, Option<_>> = BTreeMap::new();
        for (camera, source, alias) in crate::native::topology::presentation::capture_aliases(world) {
            candidates.entry(alias).and_modify(|current| {
                if *current != Some((camera, source)) { *current = None; }
            }).or_insert(Some((camera, source)));
        }
        let mut aliases: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for (alias, candidate) in candidates {
            if let Some((camera, source)) = candidate {
                if super::super::excluded_with(world, camera, additional) {
                    if writers.get(&alias).is_none_or(|writers| writers.iter().all(|writer| super::super::excluded_with(world, writer.entity, additional))) {
                        reusable.remove(&alias);
                        redacted.insert(alias);
                        seeds.insert(alias, Arc::new(CaptureOrigins { complete: true, ..default() }));
                    }
                    continue;
                }
                if reusable.contains(&alias) || redacted.contains(&alias)
                    || world.get::<Camera>(camera).is_none_or(|camera| camera.physical_target_size().is_none_or(|size| size.min_element() == 0))
                { continue; }
                aliases.entry(source).or_default().insert(alias);
                if world.get::<Camera>(camera).is_some_and(|camera| !camera.is_active) { refresh.insert(camera); }
            }
        }
        Ok(Self { images, reusable, redacted, seeds, refresh, aliases, private_entities, unresolved })
    }

    pub fn seed(&self, render: &mut World) {
        if let Some(completed) = render.get_resource::<CompletedCaptures>() { completed.seed(self.seeds.clone()); }
        render.insert_resource(aliases::ExternalAliases(self.aliases.clone()));
    }

    pub fn refresh(&self, filter: &mut Filter, world: &mut World) {
        for entity in &self.refresh {
            if let Some(mut camera) = world.get_mut::<Camera>(*entity) {
                filter.cameras.push((*entity, camera.is_active));
                camera.is_active = true;
            }
        }
    }

    pub fn validate(&self, world: &World, render: &World, roots: &[AssetId<Image>], additional: &BTreeSet<Entity>) -> Result<(), &'static str> {
        let history = render.get_resource::<CompletedCaptures>().and_then(CompletedCaptures::snapshot)
            .ok_or("The filtered capture completion provenance is unavailable.")?;
        fn visit(
            image: AssetId<Image>, targets: &Targets, world: &World, additional: &BTreeSet<Entity>,
            history: &BTreeMap<AssetId<Image>, Arc<CaptureOrigins>>, path: &mut BTreeSet<AssetId<Image>>, budget: &mut usize,
        ) -> Result<(), &'static str> {
            if targets.reusable.contains(&image) || targets.redacted.contains(&image) { return Ok(()); }
            if *budget == 0 || path.len() >= crate::native::layout::MAX_DEPTH || !path.insert(image) {
                return Err("The filtered image dependency graph is unresolved.");
            }
            *budget -= 1;
            let origins = history.get(&image).filter(|origins| origins.complete)
                .ok_or("A retained public preview has no complete filtered image; a source is pending, unresolved or over its provenance budget.")?;
            if origins.contributors.iter().any(|entity| super::super::excluded_with(world, *entity, additional)) {
                return Err("A filtered image still contains an excluded or unresolved source.");
            }
            for input in &origins.images {
                visit(*input, targets, world, additional, history, path, budget)?;
            }
            path.remove(&image);
            Ok(())
        }
        for image in roots { visit(*image, self, world, additional, &history, &mut BTreeSet::new(), &mut 1024)?; }
        Ok(())
    }
}

pub fn reusable_targets(world: &mut World, additional: &BTreeSet<Entity>) -> BTreeSet<AssetId<Image>> {
    Targets::collect(world, additional).map_or_else(|_| BTreeSet::new(), |targets| targets.reusable)
}

