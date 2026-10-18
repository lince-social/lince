use super::*;
use bevy::{
    asset::AssetId,
    render::{
        Render, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{AsBindGroup, ShaderType},
    },
    shader::ShaderRef,
};
use lince_interface::host::SurfaceId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use std::collections::{HashMap, HashSet};

mod tree;

#[derive(Clone, Copy, ShaderType)]
struct Mapping {
    u: Vec4,
    v: Vec4,
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            u: Vec4::new(0.0, 1.0, 0.0, 0.0),
            v: Vec4::new(0.0, 0.0, 1.0, 0.0),
        }
    }
}

const SHADER: Handle<Shader> = bevy::asset::uuid_handle!("a44649f6-fb98-4a8b-8a7f-20cf024e04ab");

#[derive(AsBindGroup, Asset, TypePath, Clone)]
pub(super) struct ClientMaterial {
    #[texture(0)]
    #[sampler(1)]
    content: Handle<Image>,
    #[uniform(2)]
    mapping: Mapping,
}

impl UiMaterial for ClientMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

#[derive(Resource, ExtractResource, Clone, Default)]
struct Textures(HashMap<AssetId<Image>, wgpu::Texture>);

pub(super) struct Window {
    pub(super) entity: Entity,
    pub(super) parent: Option<SurfaceId>,
    image: Handle<Image>,
    material: Handle<ClientMaterial>,
    texture: Option<wgpu::Texture>,
    nodes: HashMap<WlSurface, tree::SurfaceView>,
}

#[cfg(test)]
impl Window {
    pub(super) fn fixture(entity: Entity) -> Self {
        Self {
            entity,
            parent: None,
            image: Default::default(),
            material: Default::default(),
            texture: None,
            nodes: Default::default(),
        }
    }
}

pub(super) fn install(app: &mut App) -> Result<(), ProbeFailure> {
    app.init_resource::<Textures>().add_plugins((
        UiMaterialPlugin::<ClientMaterial>::default(),
        ExtractResourcePlugin::<Textures>::default(),
    ));
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wgsl(include_str!("clients/tree.wgsl"), "park/application.wgsl"),
        )
        .map_err(|error| input_failure(&error.to_string()))?;
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        prepare
            .in_set(RenderSystems::PrepareAssets)
            .after(bevy::render::render_asset::prepare_assets::<GpuImage>)
            .before(
                bevy::render::render_asset::prepare_assets::<
                    bevy::ui_render::PreparedUiMaterial<ClientMaterial>,
                >,
            ),
    );
    Ok(())
}

fn prepare(
    textures: Res<Textures>,
    device: Res<RenderDevice>,
    mut images: ResMut<RenderAssets<GpuImage>>,
    mut retained: Local<HashSet<AssetId<Image>>>,
) {
    if !textures.is_changed() {
        return;
    }
    let current: HashSet<_> = textures.0.keys().copied().collect();
    for removed in retained.difference(&current) {
        images.remove(*removed);
    }
    for (id, texture) in &textures.0 {
        images.insert(
            *id,
            GpuImage {
                texture: texture.clone().into(),
                texture_view: texture.create_view(&Default::default()).into(),
                sampler: device.create_sampler(&Default::default()),
                texture_descriptor: wgpu::TextureDescriptor {
                    label: Some("park_application_content"),
                    size: texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: texture.format(),
                    usage: texture.usage(),
                    view_formats: &[],
                },
                texture_view_descriptor: None,
                had_data: false,
            },
        );
    }
    *retained = current;
}

impl Presentation {
    pub fn open_window(
        &mut self,
        surface: SurfaceId,
        title: &str,
        app_id: &str,
    ) -> Result<lince_interface::layout::desktop::PlacementId, ProbeFailure> {
        if self.windows.contains_key(&surface) {
            return Err(input_failure("the application surface already exists"));
        }
        if title.len() > 4096 || app_id.len() > 4096 {
            return Err(input_failure("application metadata exceeds its limit"));
        }
        let root = self.canvas_root()?;
        let world = self.app.world_mut();
        let workspace = native::laboratory::scope(world)
            .filter(|(owner, _, temporary, _)| *owner == root && world.get::<native::workspace::Workspaces>(root).is_some_and(|spaces| spaces.active == *temporary))
            .map(|(_, _, _, previous)| previous)
            .unwrap_or(
                world
                    .get::<native::workspace::Workspaces>(root)
                    .ok_or_else(|| input_failure("the workspace is unavailable"))?
                    .active,
            );
        let image = world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1,
                1,
                wgpu::TextureFormat::Bgra8Unorm,
                None,
            ));
        let material = world
            .resource_mut::<Assets<ClientMaterial>>()
            .add(ClientMaterial {
                content: image.clone(),
                mapping: Default::default(),
            });
        let mut layout = native::layout::LayoutBox::new(Vec2::new(640.0, 480.0));
        let identity = *self
            .identities
            .entry(surface)
            .or_insert(lince_interface::layout::desktop::PlacementId(layout.id));
        layout.id = identity.0;
        let entity = world
            .spawn((
                native::castle::Castle,
                native::castle::Pending,
                native::sand_store::SandCredits(native::credits::ATTRIBUTIONS),
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::new(640.0, 480.0),
                },
                layout,
                native::workspace::WorkspaceMember(workspace),
                native::layout::desktop::ApplicationWindow {
                    surface,
                    content_size: [1, 1],
                },
                native::area::RecordProperties(serde_json::json!({"head":title,"app_id":app_id})),
                MaterialNode(material.clone()),
                ChildOf(root),
            ))
            .id();
        self.windows.insert(
            surface,
            Window {
                entity,
                parent: None,
                image,
                material,
                texture: None,
                nodes: Default::default(),
            },
        );
        if let Some(saved) = self
            .records
            .as_ref()
            .and_then(|records| records.profile(identity))
            .cloned()
        {
            let world = self.app.world_mut();
            let workspace = world
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .any(|workspace| workspace.id == saved.workspace)
                .then_some(saved.workspace)
                .unwrap_or(workspace);
            world.entity_mut(entity).insert((
                saved.layout,
                saved.tokens,
                native::workspace::WorkspaceMember(workspace),
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::from_array(saved.position),
                    size: Vec2::from_array(saved.size),
                },
            ));
        }
        Ok(identity)
    }

    pub fn window_metadata(
        &mut self,
        surface: SurfaceId,
        title: &str,
        app_id: &str,
    ) -> Result<(), ProbeFailure> {
        if title.len() > 4096 || app_id.len() > 4096 {
            return Err(input_failure("application metadata exceeds its limit"));
        }
        let window = self
            .windows
            .get(&surface)
            .ok_or_else(|| input_failure("the application surface is unavailable"))?;
        let bound = self
            .app
            .world()
            .get::<native::protein_area::RecordBinding>(window.entity)
            .is_some();
        let title_field = if bound { "window_title" } else { "head" };
        if self
            .app
            .world()
            .get::<native::area::RecordProperties>(window.entity)
            .is_some_and(|properties| {
                properties.0[title_field] == title && properties.0["app_id"] == app_id
            })
        {
            return Ok(());
        }
        let mut properties = self
            .app
            .world_mut()
            .get_mut::<native::area::RecordProperties>(window.entity)
            .ok_or_else(|| input_failure("the application Castle is unavailable"))?;
        properties.0[title_field] = title.into();
        properties.0["app_id"] = app_id.into();
        Ok(())
    }

    pub fn close_window(&mut self, surface: SurfaceId) {
        self.window_requests.remove(&surface);
        self.rejected.remove(&surface);
        self.hide_window(surface);
        self.identities.remove(&surface);
    }

    pub fn rejected_windows(&self) -> Vec<SurfaceId> {
        self.rejected.iter().copied().collect()
    }

    fn hide_window(&mut self, surface: SurfaceId) {
        self.painted_windows.remove(&surface);
        let Some(window) = self.windows.remove(&surface) else {
            return;
        };
        if let Some(records) = &mut self.records
            && let Some(identity) = self.identities.get(&surface)
        {
            records.capture(self.app.world(), &window, *identity);
        }
        let world = self.app.world_mut();
        world.despawn(window.entity);
        for node in window.nodes.into_values() {
            tree::remove(world, node);
        }
        world
            .resource_mut::<Textures>()
            .0
            .remove(&window.image.id());
        world
            .resource_mut::<Assets<Image>>()
            .remove(window.image.id());
        world
            .resource_mut::<Assets<ClientMaterial>>()
            .remove(window.material.id());
    }

    pub fn surface_texture(
        &mut self,
        surface: SurfaceId,
        source: &wgpu::Texture,
    ) -> Result<(), ProbeFailure> {
        validate_size([source.width(), source.height()])?;
        if source.format() != wgpu::TextureFormat::Bgra8Unorm
            || source.dimension() != wgpu::TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.sample_count() != 1
            || !source.usage().contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(input_failure(
                "application textures must be copyable BGRA8 content",
            ));
        }
        let window = self
            .windows
            .get_mut(&surface)
            .ok_or_else(|| input_failure("the application surface is unavailable"))?;
        let resized = window
            .texture
            .as_ref()
            .is_none_or(|texture| texture.size() != source.size());
        if resized {
            let texture = self
                .renderer
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("park_application_content"),
                    size: source.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
            let world = self.app.world_mut();
            *world
                .resource_mut::<Assets<Image>>()
                .get_mut(&window.image)
                .unwrap() =
                Image::new_target_texture(source.width(), source.height(), source.format(), None);
            world
                .resource_mut::<Assets<ClientMaterial>>()
                .get_mut(&window.material)
                .unwrap()
                .content = window.image.clone();
            world
                .resource_mut::<Textures>()
                .0
                .insert(window.image.id(), texture.clone());
            window.texture = Some(texture);
        }
        let mut commands = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        commands.copy_texture_to_texture(
            source.as_image_copy(),
            window.texture.as_ref().unwrap().as_image_copy(),
            source.size(),
        );
        self.renderer.queue.submit([commands.finish()]);
        let world = self.app.world_mut();
        world
            .get_mut::<native::layout::desktop::ApplicationWindow>(window.entity)
            .unwrap()
            .content_size = [source.width(), source.height()];
        world
            .entity_mut(window.entity)
            .remove::<native::castle::Pending>();
        Ok(())
    }

    pub fn geometry(&self) -> Option<&lince_interface::host::GeometrySnapshot> {
        self.app
            .world()
            .get_resource::<native::layout::desktop::PublishedGeometry>()?
            .0
            .as_ref()
    }
}
