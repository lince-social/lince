use std::{collections::HashMap, hash::Hash};

use bevy::{
    asset::AssetId,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_phase::{DrawFunctionId, DrawFunctions},
        render_resource::BindGroupId,
    },
    ui_render::{
        DrawUiMaterial, ExtractedUiMaterialNodes, PreparedUiMaterial, TransparentUi,
        ui_material::UiMaterial,
    },
};

/// Versions for materials whose pixels depend only on their geometry, prepared
/// bindings and explicitly versioned texture contents. Update the version on
/// every pixel write, including updates that keep the same texture and binding.
#[derive(Resource)]
pub struct UiMaterialVersions<M: UiMaterial>(HashMap<AssetId<M>, u64>);

impl<M: UiMaterial> Default for UiMaterialVersions<M> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<M: UiMaterial> ExtractResource<RenderApp> for UiMaterialVersions<M> {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        Self(source.0.clone())
    }
}

impl<M: UiMaterial> UiMaterialVersions<M> {
    pub fn set(&mut self, material: AssetId<M>, version: u64) {
        self.0.insert(material, version);
    }

    pub fn remove(&mut self, material: AssetId<M>) {
        self.0.remove(&material);
    }
}

pub fn retain_ui_material<M: UiMaterial>(app: &mut App) {
    app.init_resource::<UiMaterialVersions<M>>()
        .add_plugins(ExtractResourcePlugin::<UiMaterialVersions<M>>::default());
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        collect::<M>
            .in_set(RenderSystems::PrepareViews)
            .after(clear),
    );
}

#[derive(Resource, Default)]
pub(super) struct RetainedDraws(HashMap<(DrawFunctionId, Entity), (BindGroupId, u64)>);

impl RetainedDraws {
    pub(super) fn hash(
        &self,
        item: &TransparentUi,
        fingerprint: &mut impl std::hash::Hasher,
    ) -> Option<()> {
        let key = (item.draw_function, item.entity.0);
        let bindings = self.0.get(&key)?;
        key.hash(fingerprint);
        bindings.hash(fingerprint);
        item.sort_key.0.to_bits().hash(fingerprint);
        Some(())
    }
}

pub(super) fn clear(mut retained: ResMut<RetainedDraws>) {
    retained.0.clear();
}

fn collect<M: UiMaterial>(
    versions: Res<UiMaterialVersions<M>>,
    nodes: Res<ExtractedUiMaterialNodes<M>>,
    materials: Res<RenderAssets<PreparedUiMaterial<M>>>,
    draws: Res<DrawFunctions<TransparentUi>>,
    mut retained: ResMut<RetainedDraws>,
) {
    let Some(draw) = draws.read().get_id::<DrawUiMaterial<M>>() else {
        return;
    };
    for (_, (_, group)) in &nodes.uinodes {
        for (entity, node) in group.iter() {
            if let Some(&version) = versions.0.get(&node.material)
                && let Some(material) = materials.get(node.material)
            {
                retained
                    .0
                    .insert((draw, *entity), (material.bind_group.id(), version));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{DefaultHasher, Hasher};

    use bevy::{
        math::FloatOrd,
        render::{render_phase::PhaseItemExtraIndex, render_resource::CachedRenderPipelineId},
    };

    use super::*;

    #[test]
    fn a_reused_binding_requires_the_same_content_version() {
        let mut retained = RetainedDraws::default();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let draw = DrawFunctionId(1);
        let item = TransparentUi {
            sort_key: FloatOrd(1.0),
            entity: (entity, entity.into()),
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: draw,
            batch_range: 0..0,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        };
        let fingerprint = |retained: &RetainedDraws| {
            let mut hash = DefaultHasher::new();
            retained.hash(&item, &mut hash).map(|()| hash.finish())
        };
        assert!(fingerprint(&retained).is_none());
        let binding = BindGroupId::new();
        retained.0.insert((draw, entity), (binding, 1));
        let first = fingerprint(&retained).unwrap();
        assert_eq!(fingerprint(&retained), Some(first));
        retained.0.insert((draw, entity), (binding, 2));
        let second = fingerprint(&retained).unwrap();
        assert_ne!(first, second);
        retained.0.insert((draw, entity), (BindGroupId::new(), 2));
        assert_ne!(fingerprint(&retained), Some(second));
    }
}
