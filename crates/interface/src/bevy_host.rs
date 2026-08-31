use bevy::{
    app::{App, TaskPoolPlugin},
    asset::AssetPlugin,
    camera::CameraPlugin,
    diagnostic::FrameCountPlugin,
    ecs::{
        resource::Resource,
        schedule::{IntoScheduleConfigs, ScheduleCleanupPolicy},
        system::{Local, Res},
        world::World,
    },
    image::ImagePlugin,
    mesh::MeshPlugin,
    render::{
        Render, RenderApp, RenderPlugin, RenderSystems,
        renderer::{
            RenderAdapter, RenderAdapterInfo, RenderDevice, RenderGraph, RenderGraphSystems,
            RenderInstance, RenderQueue, WgpuWrapper, render_system,
        },
        settings::RenderCreation,
    },
    time::TimePlugin,
    transform::TransformPlugin,
    window::{ExitCondition, WindowPlugin},
};
use std::sync::{Arc, Mutex};

pub struct BevyHost {
    app: App,
    _output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    command_handoff: HostCommandHandoff,
    updates: u64,
    handed_off_command_buffers: u64,
}

#[derive(Resource, Clone)]
struct BevyOutputView(wgpu::TextureView);

#[derive(Resource, Clone)]
struct HostCommandHandoff(Arc<Mutex<Vec<wgpu::CommandBuffer>>>);

impl HostCommandHandoff {
    fn is_empty(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }

    fn take(&self) -> Vec<wgpu::CommandBuffer> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

fn render_output(
    output: Res<BevyOutputView>,
    device: Res<RenderDevice>,
    handoff: Res<HostCommandHandoff>,
    mut frame: Local<u64>,
) {
    let phase = (*frame % 240) as f64 / 239.0;
    *frame += 1;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("lince-bevy-output"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lince-bevy-output-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output.0,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.025 + phase * 0.035,
                        g: 0.055 + phase * 0.055,
                        b: 0.09 + phase * 0.09,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    handoff
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(encoder.finish());
}

fn render_for_host(world: &mut World) {
    world.run_schedule(RenderGraph);
}

impl BevyHost {
    pub fn new(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        adapter_info: wgpu::AdapterInfo,
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Result<Self, String> {
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lince-bevy-output-texture"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let command_handoff = HostCommandHandoff(Arc::new(Mutex::new(Vec::new())));
        let render_creation = RenderCreation::manual(
            RenderDevice::from(device),
            RenderQueue(Arc::new(WgpuWrapper::new(queue))),
            RenderAdapterInfo(WgpuWrapper::new(adapter_info)),
            RenderAdapter(Arc::new(WgpuWrapper::new(adapter))),
            RenderInstance(Arc::new(WgpuWrapper::new(instance))),
        );
        let mut app = App::new();
        app.add_plugins(TaskPoolPlugin::default());
        app.add_plugins(FrameCountPlugin);
        app.add_plugins(TimePlugin);
        app.add_plugins(TransformPlugin);
        app.add_plugins(WindowPlugin {
            primary_window: None,
            primary_cursor_options: None,
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
        });
        app.add_plugins(AssetPlugin::default());
        app.add_plugins(RenderPlugin {
            render_creation,
            synchronous_pipeline_compilation: true,
            ..RenderPlugin::default()
        });
        app.add_plugins(ImagePlugin::default());
        app.add_plugins(MeshPlugin);
        app.add_plugins(CameraPlugin);
        app.finish();
        app.cleanup();
        if app.get_sub_app(RenderApp).is_none() {
            return Err("Bevy created no render sub-application".into());
        }
        let render_app = app
            .get_sub_app_mut(RenderApp)
            .ok_or_else(|| "Bevy created no mutable render sub-application".to_owned())?;
        render_app.insert_resource(BevyOutputView(output_view.clone()));
        render_app.insert_resource(command_handoff.clone());
        render_app.add_systems(
            RenderGraph,
            render_output.in_set(RenderGraphSystems::Render),
        );
        let removed = render_app
            .world_mut()
            .schedule_scope(Render, |world, schedule| {
                schedule.remove_systems_in_set(
                    render_system,
                    world,
                    ScheduleCleanupPolicy::RemoveSystemsOnly,
                )
            });
        let removed = removed.map_err(|error| error.to_string())?;
        if removed != 1 {
            return Err(format!(
                "expected one stock Bevy render system, removed {removed}"
            ));
        }
        render_app.add_systems(
            Render,
            render_for_host
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
        );
        app.update();
        Ok(Self {
            app,
            _output_texture: output_texture,
            output_view,
            command_handoff,
            updates: 1,
            handed_off_command_buffers: 0,
        })
    }

    pub fn prepare_frame(&mut self) {
        if self.command_handoff.is_empty() {
            self.app.update();
            self.updates += 1;
        }
    }

    pub fn take_output_commands(&mut self) -> Vec<wgpu::CommandBuffer> {
        let commands = self.command_handoff.take();
        self.handed_off_command_buffers += commands.len() as u64;
        commands
    }

    pub fn updates(&self) -> u64 {
        self.updates
    }

    pub fn handed_off_command_buffers(&self) -> u64 {
        self.handed_off_command_buffers
    }

    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }
}
