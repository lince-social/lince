use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        mesh::{RenderMesh, allocator::allocate_and_free_meshes},
        render_asset::ExtractedAssets,
    },
};

pub(super) fn install(app: &mut App) {
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.add_systems(
            Render,
            retire_empty
                .in_set(RenderSystems::PrepareAssets)
                .before(allocate_and_free_meshes),
        );
    }
}

fn retire_empty(mut meshes: ResMut<ExtractedAssets<RenderMesh>>) {
    let meshes = &mut *meshes;
    meshes.extracted.retain(|(id, mesh)| {
        if mesh.get_vertex_buffer_size() != 0 {
            return true;
        }
        meshes.removed.insert(*id);
        meshes.added.remove(id);
        meshes.modified.remove(id);
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        mesh::{Indices, PrimitiveTopology},
        render::{mesh::allocator::MeshAllocator, render_asset::RenderAssets},
    };

    fn mesh(visible: bool) -> Mesh {
        let positions = if visible {
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        } else {
            Vec::new()
        };
        let indices = if visible { vec![0, 1, 2] } else { Vec::new() };
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(Indices::U32(indices))
    }

    #[test]
    #[ignore = "requires a native render adapter"]
    fn empty_geometry_retires_gpu_data_and_restores_the_same_asset_when_visible() {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::log::LogPlugin>()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                }),
        );
        install(&mut app);
        app.finish();
        app.cleanup();
        let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh(false));
        for visible in [false, true, false, false, true, false, true] {
            app.world_mut()
                .resource_mut::<Assets<Mesh>>()
                .insert(handle.id(), mesh(visible))
                .unwrap();
            for _ in 0..3 {
                app.update();
            }
            let render = app.sub_app(RenderApp).world();
            assert_eq!(
                render.resource::<RenderAssets<RenderMesh>>().get(handle.id()).is_some(),
                visible,
            );
            let allocator = render.resource::<MeshAllocator>();
            assert_eq!(allocator.mesh_vertex_slice(&handle.id()).is_some(), visible);
            assert_eq!(allocator.mesh_index_slice(&handle.id()).is_some(), visible);
            assert!(app.world().resource::<Assets<Mesh>>().contains(handle.id()));
        }
    }
}
