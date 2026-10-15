use super::*;
use bevy::render::{
    RenderStartup,
    diagnostic::RecordDiagnostics,
    renderer::{RenderContext, RenderGraph, RenderGraphSystems, RenderQueue},
    storage::{GpuShaderBuffer, ShaderBuffer},
};
use bevy::{core_pipeline::schedule::camera_driver, shader::ShaderCacheError};

#[derive(Clone)]
struct Job {
    buffer: Handle<ShaderBuffer>,
    shader: Handle<Shader>,
    vertices: u32,
    inputs: ShaderInputs,
    run: bool,
}

#[derive(Resource, Default)]
struct Jobs(HashMap<AssetId<ShaderBuffer>, Job>);

#[derive(Resource, Default)]
struct ComputeRequests(Vec<Job>);

#[derive(Clone, Resource, Default)]
struct ComputeStatus(Arc<Mutex<HashMap<AssetId<ShaderBuffer>, Result<(), String>>>>);

struct Prepared {
    inputs: UniformBuffer<ShaderInputs>,
    _counts: UniformBuffer<UVec4>,
    bindings: BindGroup,
    pipeline: CachedComputePipelineId,
}

#[derive(Resource)]
struct Compute {
    layout: BindGroupLayoutDescriptor,
    pipelines: HashMap<AssetId<Shader>, CachedComputePipelineId>,
    prepared: HashMap<AssetId<ShaderBuffer>, Prepared>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Jobs>();
    let status = ComputeStatus::default();
    app.insert_resource(status.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(status)
            .init_resource::<ComputeRequests>()
            .add_systems(RenderStartup, setup)
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareBindGroups))
            .add_systems(
                RenderGraph,
                dispatch
                    .in_set(RenderGraphSystems::Render)
                    .before(camera_driver),
            );
    }
}

fn setup(mut commands: Commands) {
    use bevy::render::render_resource::binding_types::{storage_buffer_sized, uniform_buffer};
    commands.insert_resource(Compute {
        layout: BindGroupLayoutDescriptor::new(
            "Record geometry",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<ShaderInputs>(false),
                    uniform_buffer::<UVec4>(false),
                    storage_buffer_sized(false, std::num::NonZeroU64::new(48)),
                ),
            ),
        ),
        pipelines: default(),
        prepared: default(),
    });
}

fn extract(mut requests: ResMut<ComputeRequests>, jobs: Extract<Res<Jobs>>) {
    requests.0 = jobs.0.values().take(128).cloned().collect();
}

fn prepare(
    requests: Res<ComputeRequests>,
    mut compute: ResMut<Compute>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    status: Res<ComputeStatus>,
) {
    let alive: HashSet<_> = requests.0.iter().map(|job| job.buffer.id()).collect();
    compute.prepared.retain(|id, _| alive.contains(id));
    for job in &requests.0 {
        let Some(buffer) = buffers.get(&job.buffer) else {
            continue;
        };
        let id = if let Some(id) = compute.pipelines.get(&job.shader.id()) {
            *id
        } else {
            let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("Record geometry".into()),
                layout: vec![compute.layout.clone()],
                shader: job.shader.clone(),
                entry_point: Some("lince_generate".into()),
                ..default()
            });
            compute.pipelines.insert(job.shader.id(), id);
            id
        };
        match cache.get_compute_pipeline_state(id) {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_)) => continue,
            CachedPipelineState::Err(error) => {
                status
                    .0
                    .lock()
                    .unwrap()
                    .insert(job.buffer.id(), Err(error.to_string()));
                continue;
            }
            _ => continue,
        }
        if !compute.prepared.contains_key(&job.buffer.id()) {
            let mut inputs = UniformBuffer::from(job.inputs);
            inputs.write_buffer(&device, &queue);
            let mut counts = UniformBuffer::from(UVec4::new(job.vertices, 0, 0, 0));
            counts.write_buffer(&device, &queue);
            let bindings = device.create_bind_group(
                "Record geometry",
                &cache.get_bind_group_layout(&compute.layout),
                &BindGroupEntries::sequential((
                    &inputs,
                    &counts,
                    buffer.buffer.as_entire_buffer_binding(),
                )),
            );
            compute.prepared.insert(
                job.buffer.id(),
                Prepared {
                    inputs,
                    _counts: counts,
                    bindings,
                    pipeline: id,
                },
            );
        }
        if job.run {
            let prepared = compute.prepared.get_mut(&job.buffer.id()).unwrap();
            prepared.inputs.set(job.inputs);
            prepared.inputs.write_buffer(&device, &queue);
        }
        status.0.lock().unwrap().insert(job.buffer.id(), Ok(()));
    }
}

fn dispatch(
    mut context: RenderContext,
    requests: Res<ComputeRequests>,
    compute: Res<Compute>,
    cache: Res<PipelineCache>,
) {
    if !requests.0.iter().any(|job| job.run) {
        return;
    }
    let diagnostics = context.diagnostic_recorder();
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());
    let span = diagnostics
        .as_ref()
        .map(|recorder| recorder.pass_span(&mut pass, "record_geometry"));
    for job in requests.0.iter().filter(|job| job.run).take(8) {
        let Some(prepared) = compute.prepared.get(&job.buffer.id()) else {
            continue;
        };
        let Some(pipeline) = cache.get_compute_pipeline(prepared.pipeline) else {
            continue;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &prepared.bindings, &[]);
        pass.dispatch_workgroups(job.vertices.div_ceil(64), 1, 1);
    }
    if let Some(span) = span {
        span.end(&mut pass);
    }
}

pub(super) fn allocate(
    world: &mut World,
    stage: Option<(u32, Handle<Shader>)>,
) -> Handle<ShaderBuffer> {
    let vertices = stage.as_ref().map_or(1, |(vertices, _)| *vertices);
    let buffer = world
        .resource_mut::<Assets<ShaderBuffer>>()
        .add(ShaderBuffer::with_size(
            vertices as usize * 48,
            RenderAssetUsages::default(),
        ));
    if let Some((vertices, shader)) = stage {
        world.resource_mut::<Jobs>().0.insert(
            buffer.id(),
            Job {
                buffer: buffer.clone(),
                shader,
                vertices,
                inputs: default(),
                run: false,
            },
        );
    }
    buffer
}

pub(super) fn ready(world: &World, buffer: &Handle<ShaderBuffer>) -> Result<bool, String> {
    if !world.resource::<Jobs>().0.contains_key(&buffer.id()) {
        return Ok(true);
    }
    world
        .resource::<ComputeStatus>()
        .0
        .lock()
        .unwrap()
        .get(&buffer.id())
        .cloned()
        .map_or(Ok(false), |result| result.map(|()| true))
}

pub(super) fn begin(world: &mut World) {
    for job in world.resource_mut::<Jobs>().0.values_mut() {
        job.run = false;
    }
}

pub(super) fn update(world: &mut World, buffer: &Handle<ShaderBuffer>, inputs: ShaderInputs) {
    if let Some(job) = world.resource_mut::<Jobs>().0.get_mut(&buffer.id()) {
        job.inputs = inputs;
        job.run = true;
    }
}

pub(super) fn remove(world: &mut World, buffer: &Handle<ShaderBuffer>) {
    world.resource_mut::<Jobs>().0.remove(&buffer.id());
    world
        .resource::<ComputeStatus>()
        .0
        .lock()
        .unwrap()
        .remove(&buffer.id());
    world
        .resource_mut::<Assets<ShaderBuffer>>()
        .remove(buffer.id());
}

pub(super) fn source(stage: &nucleus::shader_effect::spatial::GeneratedGeometry) -> String {
    format!(
        "{}{}\n{}\n{}",
        nucleus::shader_effect::HEADER.replace("@group(1)", "@group(0)"),
        MESH_TYPES,
        stage.source,
        include_str!("generate.wgsl")
    )
}
