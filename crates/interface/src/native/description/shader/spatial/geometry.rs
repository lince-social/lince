// non-lean-yet
// Scope: struct Job, struct Jobs, struct ComputeRequests, struct ComputeStatus.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 49, 126, 127,
//   292.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   257, 269.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (152, 189, 197, 258, 259, 270, 271, 311, 336).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 202.

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
    buffer: BufferId,
    vertices: u32,
    pipeline: CachedComputePipelineId,
    admitted: bool,
    submitted: Option<(ComputePipelineId, ShaderInputs)>,
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
    let _extraction = nucleus::performance::Span::new("shader-geometry-requests");
    requests.0.clear();
    requests.0.extend(jobs.0.values().take(128).cloned());
    if requests.0.is_empty() {
        requests.0 = Vec::new();
    }
}

fn prepare(
    requests: Res<ComputeRequests>,
    mut compute: ResMut<Compute>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    status: Res<ComputeStatus>,
    mut alive: Local<HashSet<AssetId<ShaderBuffer>>>,
) {
    let _preparation = nucleus::performance::Span::new("shader-geometry-preparation");
    alive.clear();
    alive.extend(requests.0.iter().map(|job| job.buffer.id()));
    compute.prepared.retain(|id, prepared| {
        prepared.admitted = false;
        alive.contains(id)
    });
    if alive.is_empty() {
        *alive = HashSet::new();
        compute.prepared = HashMap::new();
        return;
    }
    for job in &requests.0 {
        let Some(buffer) = buffers.get(&job.buffer) else {
            compute.prepared.remove(&job.buffer.id());
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
        if compute
            .prepared
            .get(&job.buffer.id())
            .is_some_and(|prepared| {
                prepared.buffer != buffer.buffer.id()
                    || prepared.pipeline != id
                    || prepared.vertices != job.vertices
            })
        {
            compute.prepared.remove(&job.buffer.id());
        }
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
            nucleus::performance::add("shader-geometry-uniform-writes", 1);
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
                    buffer: buffer.buffer.id(),
                    vertices: job.vertices,
                    pipeline: id,
                    admitted: false,
                    submitted: None,
                },
            );
            nucleus::performance::add("shader-geometry-bindings-prepared", 1);
        }
        if job.run {
            let prepared = compute.prepared.get_mut(&job.buffer.id()).unwrap();
            if !prepared.inputs.get().same_uniform(&job.inputs) {
                prepared.inputs.set(job.inputs);
                prepared.inputs.write_buffer(&device, &queue);
                nucleus::performance::add("shader-geometry-uniform-writes", 1);
            }
            prepared.admitted = true;
        }
        status.0.lock().unwrap().insert(job.buffer.id(), Ok(()));
    }
}

fn dispatch(
    mut context: RenderContext,
    requests: Res<ComputeRequests>,
    mut compute: ResMut<Compute>,
    cache: Res<PipelineCache>,
    external: Option<Res<crate::native::capture::render::ExternalPass>>,
    mut work: Local<Vec<usize>>,
) {
    let _dispatch = nucleus::performance::Span::new("shader-geometry-dispatch");
    work.clear();
    if requests.0.is_empty() {
        *work = Vec::new();
        return;
    }
    if external.is_some() {
        return;
    }
    for (index, job) in requests
        .0
        .iter()
        .enumerate()
        .filter(|(_, job)| job.run)
        .take(8)
    {
        let Some(prepared) = compute.prepared.get(&job.buffer.id()) else {
            continue;
        };
        if !prepared.admitted {
            continue;
        }
        let Some(pipeline) = cache.get_compute_pipeline(prepared.pipeline) else {
            continue;
        };
        if prepared
            .submitted
            .as_ref()
            .is_some_and(|(previous, inputs)| {
                *previous == pipeline.id() && inputs.same_uniform(&job.inputs)
            })
        {
            nucleus::performance::add("shader-geometry-dispatches-retained", 1);
        } else {
            work.push(index);
        }
    }
    if work.is_empty() {
        return;
    }
    let diagnostics = context.diagnostic_recorder();
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());
    let span = diagnostics
        .as_ref()
        .map(|recorder| recorder.pass_span(&mut pass, "record_geometry"));
    for &index in work.iter() {
        let job = &requests.0[index];
        let prepared = compute.prepared.get(&job.buffer.id()).unwrap();
        let pipeline = cache.get_compute_pipeline(prepared.pipeline).unwrap();
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &prepared.bindings, &[]);
        pass.dispatch_workgroups(job.vertices.div_ceil(64), 1, 1);
    }
    if let Some(span) = span {
        span.end(&mut pass);
    }
    drop(pass);
    for &index in work.iter() {
        let job = &requests.0[index];
        let prepared = compute.prepared.get_mut(&job.buffer.id()).unwrap();
        let pipeline = cache.get_compute_pipeline(prepared.pipeline).unwrap();
        prepared.submitted = Some((pipeline.id(), job.inputs));
    }
    nucleus::performance::add("shader-geometry-dispatches", work.len() as u64);
}

pub(super) fn allocate(
    world: &mut World,
    stage: Option<(u32, Handle<Shader>)>,
) -> Handle<ShaderBuffer> {
    let vertices = stage.as_ref().map_or(1, |(vertices, _)| *vertices);
    let buffer = world
        .resource_mut::<Assets<ShaderBuffer>>()
        .add(ShaderBuffer::with_size(
            u64::from(vertices) * 48,
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
