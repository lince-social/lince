use super::{Clock, Reading};
use bevy::log::{
    tracing::{self, Subscriber, field::Visit, span::Attributes},
    tracing_subscriber::{self, layer::Context, registry::LookupSpan},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const SELECTED: &[&str] = &[
    "bevy_ui::layout::ui_layout_system",
    "bevy_ui::stack::ui_stack_system",
    "lince_interface::native::topology::presentation::synchronize",
    "lince_interface::native::layout::engine::resolve",
    "lince_interface::native::physics::simulate",
    "lince_interface::native::canvas_host::update",
    "lince_interface::native::topology::compositor::synchronize",
    "lince_interface::native::token_metrics::layout",
    "lince_interface::native::castle::remember_layout",
    "lince_interface::native::layout::desktop::publish",
    "lince_interface::native::topology::surface_render::origins::extract",
    "bevy_render::run_render_schedule",
    "bevy_render::renderer::render_system",
    "bevy_core_pipeline::schedule::camera_driver",
    "bevy_render::render_resource::pipeline_cache::PipelineCache::process_pipeline_queue_system",
    "bevy_pbr::cluster::gpu::prepare_clusters_for_gpu_clustering",
    "bevy_render::extract_plugin::apply_extract_commands",
    "bevy_core_pipeline::schedule::submit_pending_command_buffers",
    "bevy_pbr::render::mesh::extract_meshes_for_gpu_building",
    "bevy_pbr::light_probe::upload_light_probes",
    "bevy_pbr::render::light::prepare_lights",
    "bevy_pbr::render::mesh_view_bindings::prepare_mesh_view_bind_groups",
    "bevy_pbr::cluster::gpu::prepare_clustering_bind_groups",
    "bevy_pbr::cluster::gpu::upload_view_gpu_clustering_buffers",
    "bevy_render::view::prepare_view_uniforms",
    "bevy_ui_render::prepare_uinodes",
];

#[derive(Default)]
pub(super) struct Probe {
    active: AtomicBool,
    readings: Mutex<Vec<(String, Reading)>>,
}

impl Probe {
    pub(super) fn start(&self) {
        self.readings.lock().unwrap().clear();
        self.active.store(true, Ordering::Release);
    }

    pub(super) fn stop(&self) -> Vec<(String, Reading)> {
        self.active.store(false, Ordering::Release);
        std::mem::take(&mut *self.readings.lock().unwrap())
    }
}

struct Body {
    name: String,
    clock: Mutex<Option<Clock>>,
}

#[derive(Default)]
struct Name(String);

impl Visit for Name {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.0 = value.to_owned();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.0 = format!("{value:?}").trim_matches('"').to_owned();
        }
    }
}

pub(super) struct Layer(pub(super) Arc<Probe>);

impl<S: Subscriber + for<'lookup> LookupSpan<'lookup>> tracing_subscriber::Layer<S> for Layer {
    fn on_new_span(
        &self,
        attributes: &Attributes<'_>,
        id: &tracing::span::Id,
        context: Context<'_, S>,
    ) {
        if attributes.metadata().name() != "system" {
            return;
        }
        let mut name = Name::default();
        attributes.record(&mut name);
        if SELECTED.contains(&name.0.as_str()) {
            context.span(id).unwrap().extensions_mut().insert(Body {
                name: name.0,
                clock: Mutex::new(None),
            });
        }
    }

    fn on_enter(&self, id: &tracing::span::Id, context: Context<'_, S>) {
        if !self.0.active.load(Ordering::Acquire) {
            return;
        }
        let span = context.span(id).unwrap();
        let extensions = span.extensions();
        if let Some(body) = extensions.get::<Body>() {
            assert!(body.clock.lock().unwrap().replace(Clock::start()).is_none());
        }
    }

    fn on_exit(&self, id: &tracing::span::Id, context: Context<'_, S>) {
        if !self.0.active.load(Ordering::Acquire) {
            return;
        }
        let span = context.span(id).unwrap();
        let extensions = span.extensions();
        if let Some(body) = extensions.get::<Body>()
            && let Some(clock) = body.clock.lock().unwrap().take()
        {
            let reading = clock.finish();
            let mut readings = self.0.readings.lock().unwrap();
            assert!(readings.len() < 8192, "Warmed system observation overflow");
            readings.push((body.name.clone(), reading));
        }
    }
}
