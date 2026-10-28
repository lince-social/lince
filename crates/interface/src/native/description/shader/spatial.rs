use super::*;
use bevy::{
    camera::primitives::Aabb,
    core_pipeline::schedule::{Core2d, Core3d},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey, MeshPipeline, MeshPipelineKey, PreparedMaterial},
    render::{
        camera::CameraRenderGraph, erased_render_asset::ErasedRenderAssets, mesh::RenderMesh,
        render_asset::RenderAssets, render_resource::*, renderer::RenderDevice,
        storage::ShaderBuffer,
    },
    shader::{ShaderDefVal, ShaderRef},
};
use nucleus::shader_effect::spatial::{MESH_TYPES, MeshEntry, MeshSettings, Mode, Shape};

mod geometry;
mod lighting;
mod volume;

const DEPTH_VERTEX: Handle<Shader> =
    bevy::asset::uuid_handle!("de1e71d0-63c5-4d2e-980d-25677e2188cd");

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MeshShaderKey {
    vertex: Handle<Shader>,
    fragment: Handle<Shader>,
    volume: bool,
    offscreen: bool,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(MeshShaderKey)]
pub(crate) struct MeshMaterial {
    #[uniform(0)]
    inputs: ShaderInputs,
    #[uniform(1)]
    parameters: Vec4,
    #[storage(2, read_only)]
    geometry: Handle<ShaderBuffer>,
    #[uniform(3)]
    lighting: lighting::Parameters,
    #[texture(4)]
    #[sampler(5)]
    #[dependency]
    texture: Option<Handle<Image>>,
    key: MeshShaderKey,
}

impl From<&MeshMaterial> for MeshShaderKey {
    fn from(material: &MeshMaterial) -> Self {
        material.key.clone()
    }
}

fn specialize(descriptor: &mut RenderPipelineDescriptor, key: &MeshShaderKey) {
    descriptor.vertex.shader = key.vertex.clone();
    descriptor.vertex.entry_point = Some("lince_vertex".into());
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader = key.fragment.clone();
        fragment.entry_point = Some("lince_fragment".into());
        if key.volume {
            descriptor.primitive.cull_mode = None;
            if let Some(depth) = descriptor.depth_stencil.as_mut() {
                depth.depth_compare = Some(CompareFunction::Always);
                depth.depth_write_enabled = Some(false);
            }
            if key.offscreen {
                fragment.shader_defs.push("LINCE_VOLUME_PREVIEW".into());
                for target in fragment.targets.iter_mut().flatten() {
                    target.blend = None;
                }
            }
        }
    }
}

fn specialize_depth(
    descriptor: &mut RenderPipelineDescriptor,
    key: &MeshShaderKey,
    layout: &MeshVertexBufferLayoutRef,
) -> Result<(), SpecializedMeshPipelineError> {
    descriptor.vertex.shader = key.vertex.clone();
    descriptor.vertex.entry_point = Some("lince_vertex".into());
    descriptor.vertex.buffers = vec![layout.0.get_layout(&[
        Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
        Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
        Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
    ])?];
    if descriptor
        .fragment
        .as_ref()
        .is_some_and(|fragment| fragment.targets.iter().all(Option::is_none))
    {
        descriptor.fragment = None;
    }
    Ok(())
}

impl Material for MeshMaterial {
    fn alpha_mode(&self) -> AlphaMode {
        if self.key.volume {
            AlphaMode::Premultiplied
        } else if self.lighting.repeat_opacity.z < 1.0 && self.lighting.parameters.w > 0.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        }
    }

    fn vertex_shader() -> ShaderRef {
        ShaderRef::Default
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Handle(DEPTH_VERTEX)
    }

    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if descriptor.vertex.shader_defs.iter().any(|definition| {
            matches!(definition, ShaderDefVal::Bool(name, true) if name == "PREPASS_PIPELINE")
        }) {
            specialize_depth(descriptor, &key.bind_group_data, layout)?;
        } else {
            specialize(descriptor, &key.bind_group_data);
        }
        Ok(())
    }
}

struct Bundle {
    effect: Effect,
    mesh: Handle<Mesh>,
    materials: BTreeMap<String, Handle<MeshMaterial>>,
    world_materials: BTreeMap<String, Handle<MeshMaterial>>,
    keys: BTreeMap<String, MeshShaderKey>,
    geometry: Handle<ShaderBuffer>,
    compute: Option<Handle<Shader>>,
}

struct State {
    slot: usize,
    candidate: Option<Bundle>,
    working: Option<Bundle>,
    preview: Option<Entity>,
    selected: Option<String>,
}

#[derive(Component)]
struct WorldInstance {
    preview: Entity,
    anchor: Entity,
    extent: f32,
    requested_visible: bool,
}

#[derive(Resource, Default)]
struct MeshEffects(HashMap<Entity, State>);

#[derive(Resource, Default)]
struct ShaderPools {
    available: [Vec<Handle<Shader>>; 3],
    allocated: [usize; 3],
}

#[derive(Clone, Copy)]
enum Stage {
    Vertex,
    Fragment,
    Compute,
}

impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Fragment => "fragment",
            Self::Compute => "compute",
        }
    }
}

#[derive(Resource, Clone, Default)]
struct Status(Arc<Mutex<HashMap<AssetId<MeshMaterial>, Result<(), String>>>>);

#[derive(Clone)]
struct Request {
    material: AssetId<MeshMaterial>,
    mesh: AssetId<Mesh>,
    key: MeshShaderKey,
}

#[derive(Resource, Default)]
struct Requests(Vec<Request>);

struct Preflight {
    mesh: MeshPipeline,
    layout: BindGroupLayoutDescriptor,
}

impl SpecializedMeshPipeline for Preflight {
    type Key = (MeshPipelineKey, MeshShaderKey, bool);

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh.specialize(key.0, layout)?;
        descriptor.layout.insert(3, self.layout.clone());
        descriptor
            .vertex
            .shader_defs
            .push(ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3));
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment
                .shader_defs
                .push(ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3));
        }
        if key.2 {
            descriptor.fragment = None;
            specialize_depth(&mut descriptor, &key.1, layout)?;
        } else {
            specialize(&mut descriptor, &key.1);
        }
        Ok(descriptor)
    }
}

pub(super) fn install(app: &mut App) {
    lighting::install(app);
    app.init_resource::<MeshEffects>();
    app.init_resource::<ShaderPools>();
    geometry::install(app);
    if !app.world().contains_resource::<AssetServer>() {
        return;
    }
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            DEPTH_VERTEX.id(),
            Shader::from_wgsl(
                vertex_source(&Effect {
                    mode: Mode::Mesh(default()),
                    ..default()
                }),
                "record-mesh-depth.wgsl",
            ),
        )
        .unwrap();
    app.add_plugins(MaterialPlugin::<MeshMaterial>::default());
    let status = Status::default();
    app.insert_resource(status.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(status)
            .init_resource::<Requests>()
            .init_resource::<SpecializedMeshPipelines<Preflight>>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, prepare.in_set(RenderSystems::Queue));
    }
}

fn extract(mut requests: ResMut<Requests>, effects: Extract<Res<MeshEffects>>) {
    requests.0 = effects
        .0
        .values()
        .filter_map(|state| state.candidate.as_ref())
        .flat_map(|bundle| {
            bundle
                .materials
                .iter()
                .map(|(name, material)| Request {
                    material: material.id(),
                    mesh: bundle.mesh.id(),
                    key: bundle.keys[name].clone(),
                })
                .chain(bundle.world_materials.iter().map(|(name, material)| {
                    let mut key = bundle.keys[name].clone();
                    key.offscreen = false;
                    Request {
                        material: material.id(),
                        mesh: bundle.mesh.id(),
                        key,
                    }
                }))
        })
        .collect();
}

fn prepare(
    requests: Res<Requests>,
    status: Res<Status>,
    pipeline: Option<Res<MaterialPipeline>>,
    device: Res<RenderDevice>,
    meshes: Res<RenderAssets<RenderMesh>>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    mut pipelines: ResMut<SpecializedMeshPipelines<Preflight>>,
    cache: Res<PipelineCache>,
    views: Query<&ExtractedView, With<bevy::render::camera::ExtractedCamera>>,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    let formats: HashSet<_> = views
        .iter()
        .map(|view| view.target_format)
        .filter(|format| !format.is_depth_stencil_format())
        .collect();
    if formats.is_empty() {
        return;
    }
    let preflight = Preflight {
        mesh: pipeline.mesh_pipeline.clone(),
        layout: MeshMaterial::bind_group_layout_descriptor(&device),
    };
    for request in &requests.0 {
        let Some(mesh) = meshes.get(request.mesh) else {
            continue;
        };
        if materials.get(request.material).is_none() {
            continue;
        }
        let mut ready = true;
        let mut error = None;
        for format in &formats {
            for projection in [
                MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE,
                MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC,
            ] {
                let mut key = MeshPipelineKey::from_msaa_samples(1)
                    | MeshPipelineKey::from_target_format(*format)
                    | MeshPipelineKey::from_primitive_topology_and_strip_index(
                        PrimitiveTopology::TriangleList,
                        None,
                    )
                    | projection;
                if request.key.volume {
                    key |=
                        MeshPipelineKey::DEPTH_PREPASS | MeshPipelineKey::BLEND_PREMULTIPLIED_ALPHA;
                }
                for depth in [false, true] {
                    if depth && request.key.volume {
                        continue;
                    }
                    match pipelines.specialize(
                        &cache,
                        &preflight,
                        (key, request.key.clone(), depth),
                        &mesh.layout,
                    ) {
                        Ok(id) => match cache.get_render_pipeline_state(id) {
                            CachedPipelineState::Ok(_) => {}
                            CachedPipelineState::Err(cause) => error = Some(cause.to_string()),
                            _ => ready = false,
                        },
                        Err(cause) => error = Some(cause.to_string()),
                    }
                }
            }
        }
        if let Some(error) = error {
            status
                .0
                .lock()
                .unwrap()
                .insert(request.material, Err(error));
        } else if ready {
            status.0.lock().unwrap().insert(request.material, Ok(()));
        }
    }
}

fn header() -> String {
    format!(
        "{}{}\n@group(#{{MATERIAL_BIND_GROUP}}) @binding(1) var<uniform> lince_parameters: vec4<f32>;\n{}",
        nucleus::shader_effect::HEADER.replace("@group(1)", "@group(#{MATERIAL_BIND_GROUP})"),
        MESH_TYPES,
        include_str!("varyings.wgsl")
    )
}

fn vertex_source(effect: &Effect) -> String {
    let settings = match &effect.mode {
        Mode::Mesh(settings) => Some(settings),
        _ => None,
    };
    let deformation = settings.and_then(|settings| settings.deformation.as_ref()).map(|stage| stage.source.as_str())
        .unwrap_or("fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex { return vertex; }");
    format!(
        "{}\nconst LINCE_GENERATED_GEOMETRY: bool = {};\n{}\n{}",
        header(),
        settings.is_some_and(|settings| settings.geometry.is_some()),
        deformation,
        include_str!("mesh_vertex.wgsl")
    )
}

fn fragment_source(effect: &Effect, source: &str) -> Result<String, String> {
    if let Mode::Volume(settings) = &effect.mode {
        return Ok(volume::source(source, settings));
    }
    let call = match nucleus::shader_effect::spatial::mesh_entry(source)? {
        MeshEntry::Uv => "shade(input.uv, shader_inputs)",
        MeshEntry::Point => {
            "shade_mesh(MeshPoint(input.local_position, input.world_position, normalize(input.normal), input.uv), shader_inputs)"
        }
    };
    Ok(format!(
        "{}\n{}\n{}\n@fragment fn lince_fragment(input: LinceVaryings) -> @location(0) vec4<f32> {{ return lince_lit_color(input, {}); }}",
        header(),
        source,
        include_str!("spatial/lighting.wgsl"),
        call
    ))
}

fn shader(
    world: &mut World,
    source: String,
    priority: bool,
    stage: Stage,
) -> Result<Handle<Shader>, String> {
    let key = format!("mesh:{}:{source}", stage.label());
    if let Some(handle) = world.resource::<EffectAssets>().shaders.get(&key) {
        return Ok(handle.clone());
    }
    if world.resource::<EffectAssets>().shaders.len() >= if priority { 72 } else { 64 } {
        return Err("Shader cache is full. Close unused effect views before editing.".into());
    }
    let mut shader = Shader::from_wgsl(source, "record-mesh.wgsl");
    shader.validate_shader = ValidateShader::Enabled;
    let available = {
        let mut pools = world.resource_mut::<ShaderPools>();
        pools.available[stage as usize].pop()
    };
    let handle = if let Some(handle) = available {
        world
            .resource_mut::<Assets<Shader>>()
            .insert(handle.id(), shader)
            .map_err(|error| error.to_string())?;
        if let Some(status) = world.get_resource::<PipelineStatus>() {
            status.0.lock().unwrap().remove(&handle.id());
        }
        handle
    } else {
        let mut pools = world.resource_mut::<ShaderPools>();
        let count = &mut pools.allocated[stage as usize];
        if *count >= 72 {
            return Err("Mesh stage shader budget reached. Close unused effects.".into());
        }
        *count += 1;
        world.resource_mut::<Assets<Shader>>().add(shader)
    };
    world
        .resource_mut::<EffectAssets>()
        .shaders
        .insert(key, handle.clone());
    Ok(handle)
}

fn release(world: &mut World, bundle: Bundle) {
    geometry::remove(world, &bundle.geometry);
    for material in bundle
        .materials
        .values()
        .chain(bundle.world_materials.values())
    {
        world
            .resource_mut::<Assets<MeshMaterial>>()
            .remove(material.id());
        if let Some(status) = world.get_resource::<Status>() {
            status.0.lock().unwrap().remove(&material.id());
        }
    }
    world
        .resource_mut::<Assets<Mesh>>()
        .remove(bundle.mesh.id());
}

pub(super) fn candidate(
    world: &mut World,
    owner: Entity,
    effect: &Effect,
) -> Result<BTreeMap<String, Handle<Shader>>, String> {
    let settings = match &effect.mode {
        Mode::Mesh(settings) => settings.clone(),
        Mode::Volume(_) => MeshSettings {
            shape: Shape::Cuboid,
            ..default()
        },
        Mode::Surface => return Err("Choose mesh or volume mode.".into()),
    };
    let is_volume = matches!(effect.mode, Mode::Volume(_));
    if is_volume
        && world
            .query::<(&WorldInstance, &Visibility)>()
            .iter(world)
            .any(|(instance, visibility)| {
                instance.preview == owner && *visibility == Visibility::Visible
            })
    {
        volume::world_views(world);
    }
    if !world.contains_resource::<Assets<MeshMaterial>>() {
        return Err("Mesh effects need the 3D renderer.".into());
    }
    if let Some(bundle) = world
        .resource::<MeshEffects>()
        .0
        .get(&owner)
        .and_then(|state| state.candidate.as_ref())
        && bundle.effect == *effect
    {
        return Ok(bundle
            .keys
            .iter()
            .map(|(name, key)| (name.clone(), key.fragment.clone()))
            .collect());
    }
    let priority = world.get::<EffectPreview>(owner).unwrap().priority;
    let vertex = shader(world, vertex_source(effect), priority, Stage::Vertex)?;
    let compute = settings
        .geometry
        .as_ref()
        .map(|stage| shader(world, geometry::source(stage), priority, Stage::Compute))
        .transpose()?;
    let mut keys = BTreeMap::new();
    for (name, source) in &effect.programs {
        keys.insert(
            name.clone(),
            MeshShaderKey {
                vertex: vertex.clone(),
                fragment: shader(
                    world,
                    fragment_source(effect, source)?,
                    priority,
                    Stage::Fragment,
                )?,
                volume: is_volume,
                offscreen: is_volume,
            },
        );
    }
    let mesh = if let Some(stage) = &settings.geometry {
        let count = stage.vertices as usize;
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; count])
    } else {
        match settings.shape {
            Shape::Sphere => Sphere::new(0.5).mesh().uv(32, 20),
            Shape::Plane => Plane3d::default().mesh().size(1.0, 1.0).build(),
            Shape::Cuboid => Mesh::from(Cuboid::from_size(Vec3::ONE)),
        }
    };
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let geometry = geometry::allocate(
        world,
        settings
            .geometry
            .as_ref()
            .zip(compute.clone())
            .map(|(stage, shader)| (stage.vertices, shader)),
    );
    let parameters = match &effect.mode {
        Mode::Volume(settings) => Vec4::new(0.0, settings.density, 0.0, 0.0),
        _ => Vec4::new(
            settings
                .deformation
                .as_ref()
                .map_or(0.0, |stage| stage.max_displacement),
            0.0,
            0.0,
            0.0,
        ),
    };
    let materials: BTreeMap<_, _> = keys
        .iter()
        .map(|(name, key)| {
            let material = world
                .resource_mut::<Assets<MeshMaterial>>()
                .add(MeshMaterial {
                    lighting: default(),
                    texture: None,
                    key: key.clone(),
                    inputs: default(),
                    geometry: geometry.clone(),
                    parameters,
                });
            (name.clone(), material)
        })
        .collect();
    let world_materials = if is_volume {
        keys.iter()
            .map(|(name, key)| {
                let mut key = key.clone();
                key.offscreen = false;
                let material = world
                    .resource_mut::<Assets<MeshMaterial>>()
                    .add(MeshMaterial {
                        lighting: default(),
                        texture: None,
                        key,
                        inputs: default(),
                        geometry: geometry.clone(),
                        parameters,
                    });
                (name.clone(), material)
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    let slot = world
        .resource::<MeshEffects>()
        .0
        .get(&owner)
        .map(|state| state.slot)
        .or_else(|| {
            (0..crate::native::render_layers::SHADERS.len).find(|slot| {
                !world
                    .resource::<MeshEffects>()
                    .0
                    .values()
                    .any(|state| state.slot == *slot)
            })
        })
        .ok_or("Mesh preview budget reached")?;
    let old = {
        let mut effects = world.resource_mut::<MeshEffects>();
        let state = effects.0.entry(owner).or_insert(State {
            slot,
            candidate: None,
            working: None,
            preview: None,
            selected: None,
        });
        state.candidate.replace(Bundle {
            effect: effect.clone(),
            mesh,
            materials,
            world_materials,
            keys: keys.clone(),
            geometry,
            compute,
        })
    };
    if let Some(old) = old {
        release(world, old);
    }
    Ok(keys
        .into_iter()
        .map(|(name, key)| (name, key.fragment))
        .collect())
}

pub(super) fn ready(world: &World, owner: Entity) -> Result<bool, String> {
    let Some(bundle) = world
        .resource::<MeshEffects>()
        .0
        .get(&owner)
        .and_then(|state| state.candidate.as_ref())
    else {
        return Ok(false);
    };
    let Some(status) = world.get_resource::<Status>() else {
        return Ok(false);
    };
    let status = status.0.lock().unwrap();
    if !geometry::ready(world, &bundle.geometry)? {
        return Ok(false);
    }
    for material in bundle
        .materials
        .values()
        .chain(bundle.world_materials.values())
    {
        if let Some(Err(error)) = status.get(&material.id()) {
            return Err(error.clone());
        }
    }
    Ok(bundle
        .materials
        .values()
        .chain(bundle.world_materials.values())
        .all(|material| status.get(&material.id()).is_some_and(Result::is_ok)))
}

pub(super) fn commit(world: &mut World, owner: Entity) {
    let old = {
        let mut effects = world.resource_mut::<MeshEffects>();
        let state = effects.0.get_mut(&owner).unwrap();
        state.working.replace(state.candidate.take().unwrap())
    };
    if let Some(old) = old {
        release(world, old);
    }
}

pub(super) fn recycle(
    world: &mut World,
    key: &str,
    handle: Handle<Shader>,
) -> Option<Handle<Shader>> {
    if let Some(mut pools) = world.get_resource_mut::<ShaderPools>() {
        for stage in [Stage::Vertex, Stage::Fragment, Stage::Compute] {
            if key.starts_with(&format!("mesh:{}:", stage.label())) {
                pools.available[stage as usize].push(handle);
                return None;
            }
        }
    }
    Some(handle)
}

pub(super) fn shaders(world: &World) -> Vec<AssetId<Shader>> {
    world
        .get_resource::<MeshEffects>()
        .into_iter()
        .flat_map(|effects| effects.0.values())
        .flat_map(|state| state.working.iter().chain(state.candidate.iter()))
        .flat_map(|bundle| {
            bundle
                .keys
                .values()
                .flat_map(|key| [key.vertex.id(), key.fragment.id()])
                .chain(bundle.compute.iter().map(Handle::id))
        })
        .collect()
}

fn extent(effect: &Effect) -> f32 {
    match &effect.mode {
        Mode::Mesh(settings) => {
            0.5 + settings
                .deformation
                .as_ref()
                .map_or(0.0, |stage| stage.max_displacement)
        }
        _ => 0.5,
    }
}

pub(super) fn draw(world: &mut World, owner: Entity, program: &str, inputs: ShaderInputs) {
    let (mesh, material, world_material, geometry, slot, entity, extent, is_volume) = {
        let effects = world.resource::<MeshEffects>();
        let state = &effects.0[&owner];
        let bundle = state.working.as_ref().unwrap();
        (
            bundle.mesh.clone(),
            bundle.materials[program].clone(),
            bundle.world_materials.get(program).cloned(),
            bundle.geometry.clone(),
            state.slot,
            state.preview,
            extent(&bundle.effect),
            matches!(bundle.effect.mode, Mode::Volume(_)),
        )
    };
    if !world.get::<EffectPreview>(owner).unwrap().snapshot.frozen() {
        geometry::update(world, &geometry, inputs);
    }
    for handle in std::iter::once(&material).chain(world_material.as_ref()) {
        world
            .resource_mut::<Assets<MeshMaterial>>()
            .get_mut(handle)
            .unwrap()
            .inputs = inputs;
    }
    let data = world.get::<EffectPreview>(owner).unwrap();
    let camera = data.camera;
    let render_surface = data.render_surface;
    let layer = RenderLayers::none().with(crate::native::render_layers::SHADERS.get(slot).unwrap());
    if camera != Entity::PLACEHOLDER && world.get::<Camera3d>(camera).is_none() {
        world.entity_mut(camera).remove::<Camera2d>().insert((
            Camera3d::default(),
            CameraRenderGraph::new(Core3d),
            Projection::Perspective(PerspectiveProjection::default()),
            Transform::from_translation(Vec3::new(1.4, 0.9, 1.7).normalize() * extent * 5.0)
                .looking_at(Vec3::ZERO, Vec3::Y),
            layer.clone(),
        ));
    }
    if camera != Entity::PLACEHOLDER
        && let Some(mut transform) = world.get_mut::<Transform>(camera)
    {
        let direction = transform
            .translation
            .normalize_or(Vec3::new(1.4, 0.9, 1.7).normalize());
        *transform =
            Transform::from_translation(direction * extent * 5.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    volume::preview(world, camera, is_volume);
    world.get_mut::<Node>(render_surface).unwrap().display = Display::None;
    let entity = entity.unwrap_or_else(|| {
        world
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Visible,
                layer,
                bevy::picking::Pickable::IGNORE,
            ))
            .id()
    });
    world.entity_mut(entity).insert((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        crate::native::capture::render::TargetSource(owner),
        Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(extent).into(),
        },
    ));
    if is_volume {
        world
            .entity_mut(entity)
            .insert(bevy::light::NotShadowCaster);
    } else {
        world
            .entity_mut(entity)
            .remove::<bevy::light::NotShadowCaster>();
    }
    let instances: Vec<_> = world
        .query::<(Entity, &WorldInstance, &Transform, &Visibility)>()
        .iter(world)
        .filter(|(_, instance, _, _)| instance.preview == owner)
        .map(|(entity, instance, transform, visible)| {
            (
                entity,
                instance.extent,
                *transform,
                *visible == Visibility::Visible,
            )
        })
        .collect();
    if is_volume && instances.iter().any(|(_, _, _, visible)| *visible) {
        volume::world_views(world);
    }
    let world_material = world_material.as_ref().unwrap_or(&material);
    for (entity, previous_extent, mut transform, _) in instances {
        transform.scale *= previous_extent / extent;
        world.entity_mut(entity).insert((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(world_material.clone()),
            transform,
            Aabb {
                center: Vec3::ZERO.into(),
                half_extents: Vec3::splat(extent).into(),
            },
        ));
        world.get_mut::<WorldInstance>(entity).unwrap().extent = extent;
        if is_volume {
            world
                .entity_mut(entity)
                .insert(bevy::light::NotShadowCaster);
        } else {
            world
                .entity_mut(entity)
                .remove::<bevy::light::NotShadowCaster>();
        }
    }
    world
        .resource_mut::<MeshEffects>()
        .0
        .get_mut(&owner)
        .unwrap()
        .preview = Some(entity);
    world
        .resource_mut::<MeshEffects>()
        .0
        .get_mut(&owner)
        .unwrap()
        .selected = Some(program.to_owned());
}

pub(crate) fn place(
    world: &mut World,
    preview: Entity,
    anchor: Entity,
    record: Entity,
    mut transform: Transform,
    visible: bool,
) -> bool {
    let requested_visible = visible;
    let visible = visible && enabled(world, preview);
    let existing = world
        .query::<(Entity, &WorldInstance)>()
        .iter(world)
        .find(|(_, instance)| instance.anchor == anchor)
        .map(|(entity, _)| entity);
    let assets = world.get_resource::<MeshEffects>().and_then(|effects| {
        let state = effects.0.get(&preview)?;
        let bundle = state.working.as_ref()?;
        let selected = state.selected.as_ref()?;
        let material = bundle
            .world_materials
            .get(selected)
            .or_else(|| bundle.materials.get(selected))?
            .clone();
        Some((
            bundle.mesh.clone(),
            material,
            extent(&bundle.effect),
            matches!(bundle.effect.mode, Mode::Volume(_)),
        ))
    });
    let Some((mesh, material, extent, is_volume)) = assets else {
        if let Some(entity) = existing {
            world.despawn(entity);
        }
        return false;
    };
    if is_volume && visible {
        volume::world_views(world);
    }
    transform.scale /= 2.0 * extent;
    let mut layers = RenderLayers::layer(0);
    if let Some((root, workspace)) = world
        .get::<ChildOf>(anchor)
        .map(ChildOf::parent)
        .filter(|root| {
            world
                .get::<crate::native::workspace::Workspaces>(*root)
                .is_some()
        })
        .zip(
            world
                .get::<crate::native::workspace::WorkspaceMember>(anchor)
                .map(|member| member.0),
        )
    {
        let origin = crate::native::topology::presentation::origin(world, root);
        let canonical = crate::native::output_views::canvas(world, root, workspace).center;
        transform.translation +=
            (origin - bevy::math::DVec3::new(canonical.x, 0.0, canonical.y)).as_vec3();
        layers = crate::native::topology::output_views::asset_layers(world, root, workspace);
    }
    let entity = existing.unwrap_or_else(|| {
        world
            .spawn((
                crate::native::shader_effects::RecordMesh(record),
                WorldInstance {
                    preview,
                    anchor,
                    extent,
                    requested_visible,
                },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Hidden,
                bevy::picking::Pickable::IGNORE,
                crate::native::inspection::InspectionExcluded,
            ))
            .id()
    });
    world.entity_mut(entity).insert((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        crate::native::capture::render::TargetSource(preview),
        layers,
        transform,
        if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
        Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(extent).into(),
        },
    ));
    world.get_mut::<WorldInstance>(entity).unwrap().extent = extent;
    world
        .get_mut::<WorldInstance>(entity)
        .unwrap()
        .requested_visible = requested_visible;
    if is_volume {
        world
            .entity_mut(entity)
            .insert(bevy::light::NotShadowCaster);
    } else {
        world
            .entity_mut(entity)
            .remove::<bevy::light::NotShadowCaster>();
    }
    true
}

pub(crate) fn rotate(world: &mut World, owner: Entity) {
    let Some(preview) = world.get::<EffectPreview>(owner) else {
        return;
    };
    let (camera, frozen) = (preview.camera, preview.frozen);
    if world.get::<Camera3d>(camera).is_none() {
        return;
    }
    if let Some(mut transform) = world.get_mut::<Transform>(camera) {
        let position = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4) * transform.translation;
        *transform = Transform::from_translation(position).looking_at(Vec3::ZERO, Vec3::Y);
    }
    if !frozen {
        world.get_mut::<EffectPreview>(owner).unwrap().painted = 0;
    }
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(super) fn surface(world: &mut World, owner: Entity) {
    let Some(state) = world
        .get_resource_mut::<MeshEffects>()
        .and_then(|mut effects| effects.0.remove(&owner))
    else {
        return;
    };
    if let Some(entity) = state.preview {
        world.despawn(entity);
    }
    let instances: Vec<_> = world
        .query::<(Entity, &WorldInstance)>()
        .iter(world)
        .filter(|(_, instance)| instance.preview == owner)
        .map(|(entity, _)| entity)
        .collect();
    for entity in instances {
        world.despawn(entity);
    }
    for bundle in state.working.into_iter().chain(state.candidate) {
        release(world, bundle);
    }
    if let Some(preview) = world.get::<EffectPreview>(owner) {
        let (camera, surface) = (preview.camera, preview.render_surface);
        if camera != Entity::PLACEHOLDER {
            world
                .entity_mut(camera)
                .remove::<(
                    Camera3d,
                    bevy::core_pipeline::prepass::DepthPrepass,
                    volume::VolumeDepth,
                )>()
                .insert((
                    Camera2d,
                    CameraRenderGraph::new(Core2d),
                    Projection::Orthographic(OrthographicProjection::default_2d()),
                    Transform::default(),
                    RenderLayers::layer(30),
                ));
        }
        world.get_mut::<Node>(surface).unwrap().display = Display::Flex;
    }
}

fn enabled(world: &World, preview: Entity) -> bool {
    !crate::native::shader_effects::disabled(world)
        && world
            .get::<EffectPreview>(preview)
            .is_some_and(|preview| !preview.disabled && !preview.budget_blocked)
}

pub(super) fn prune(world: &mut World, alive: &HashSet<Entity>) {
    if !world.contains_resource::<MeshEffects>() {
        return;
    }
    geometry::begin(world);
    let dead: Vec<_> = world
        .resource::<MeshEffects>()
        .0
        .keys()
        .filter(|owner| !alive.contains(owner))
        .copied()
        .collect();
    for owner in dead {
        surface(world, owner);
    }
    let stale: Vec<_> = world
        .query::<(Entity, &WorldInstance)>()
        .iter(world)
        .filter(|(_, instance)| !world.entities().contains(instance.anchor))
        .map(|(entity, _)| entity)
        .collect();
    for entity in stale {
        world.despawn(entity);
    }
    let visibility: Vec<_> = world
        .query::<(Entity, &WorldInstance, &Visibility)>()
        .iter(world)
        .filter_map(|(entity, instance, current)| {
            let next = if instance.requested_visible && enabled(world, instance.preview) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            (*current != next).then_some((entity, next))
        })
        .collect();
    for (entity, visibility) in visibility {
        world.entity_mut(entity).insert(visibility);
    }
    volume::prune(world);
}
