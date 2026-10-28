use super::*;
use bevy::render::extract_resource::ExtractResource;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    time::Instant,
};

#[derive(Default, Serialize)]
pub(super) struct ViewMetrics {
    pub query_micros: VecDeque<u64>,
    pub render_submission_micros: VecDeque<u64>,
    pub input_to_submission_micros: VecDeque<u64>,
    pub tracked_output_texture_bytes: u64,
    pub tracked_ui_buffer_bytes: u64,
    #[serde(skip)]
    input_seen: Option<Instant>,
}

#[derive(Default, Serialize)]
pub(super) struct State {
    pub views: BTreeMap<String, ViewMetrics>,
    pub device: lince_map::diagnostics::DeviceSnapshot,
    pub frame_interval_micros: VecDeque<u64>,
}

#[derive(Resource, Default, Clone, ExtractResource)]
pub(super) struct Diagnostics(pub Arc<Mutex<State>>);

fn sample(values: &mut VecDeque<u64>, value: u64) {
    if values.len() >= 16_384 {
        values.pop_front();
    }
    values.push_back(value);
}

pub(super) fn query(world: &mut World, owner: Entity, micros: u64) {
    world.init_resource::<Diagnostics>();
    let mut state = world
        .resource::<Diagnostics>()
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    sample(
        &mut state
            .views
            .entry(format!("{owner:?}"))
            .or_default()
            .query_micros,
        micros,
    );
}

pub(super) fn rendered(metrics: &Diagnostics, owner: Entity, rendered: crate::map_view::Rendered) {
    let now = rendered.submitted;
    let mut state = metrics.0.lock().unwrap_or_else(|error| error.into_inner());
    state.device = rendered.device;
    let view = state.views.entry(format!("{owner:?}")).or_default();
    sample(
        &mut view.render_submission_micros,
        now.duration_since(rendered.started)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64,
    );
    if let Some(input) = rendered.input
        && view.input_seen != Some(input)
    {
        sample(
            &mut view.input_to_submission_micros,
            now.saturating_duration_since(input)
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
        );
        view.input_seen = Some(input);
    }
}

pub(super) fn callback(
    world: &World,
    owner: Entity,
) -> Arc<dyn Fn(crate::map_view::Rendered) + Send + Sync> {
    let metrics = world
        .get_resource::<Diagnostics>()
        .cloned()
        .unwrap_or_default();
    Arc::new(move |sample| rendered(&metrics, owner, sample))
}

pub(super) fn snapshot(world: &World) -> serde_json::Value {
    let metrics = world
        .get_resource::<Diagnostics>()
        .map(|resource| {
            serde_json::to_value(&*resource.0.lock().unwrap_or_else(|error| error.into_inner()))
                .unwrap_or_default()
        })
        .unwrap_or_default();
    serde_json::json!({"metrics":metrics,"scene_tasks":lince_map::diagnostics::snapshot(),"gpu_scope":"Buffer/texture device counters include the whole Bevy device. Per-view output textures and UI buffers are tracked separately. Unavailable counters are unknown; driver memory is unmeasured.","timing_scope":"Render submission is CPU time, frame interval is host redraw interval, and input latency ends at submission. GPU/display timing and RSS require the named manual procedure."})
}

pub(super) fn toggle(world: &mut World, owner: Entity) {
    if let Some(panel) = world
        .get::<MapSand>(owner)
        .and_then(|sand| sand.diagnostics)
    {
        world.despawn(panel);
        world.get_mut::<MapSand>(owner).unwrap().diagnostics = None;
    } else {
        let panel =
            crate::native::edit_mode::label(world, owner, "Collecting map diagnostics", 12.0);
        world.get_mut::<MapSand>(owner).unwrap().diagnostics = Some(panel);
    }
}

pub(super) fn reset(world: &mut World) {
    world.init_resource::<Diagnostics>();
    let mut state = world
        .resource::<Diagnostics>()
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.frame_interval_micros.clear();
    for view in state.views.values_mut() {
        view.query_micros.clear();
        view.render_submission_micros.clear();
        view.input_to_submission_micros.clear();
    }
}

#[derive(Resource, Default)]
struct ImagesOwned(BTreeMap<Entity, bevy::asset::AssetId<Image>>);

#[derive(Resource, Default)]
struct LastReport(Option<Instant>);

fn p95(values: &VecDeque<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.iter().copied().collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted
        .get((sorted.len() * 95).div_ceil(100).saturating_sub(1))
        .copied()
}

fn report(world: &World) -> String {
    let state = world
        .resource::<Diagnostics>()
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let views: Vec<_> = state.views.iter().map(|(uid, view)| serde_json::json!({"view":uid,"query_p95_us":p95(&view.query_micros),"query_samples":view.query_micros.len(),"render_submission_p95_us":p95(&view.render_submission_micros),"input_to_submission_p95_us":p95(&view.input_to_submission_micros),"owned_output_texture_bytes":view.tracked_output_texture_bytes,"tracked_ui_buffer_bytes":view.tracked_ui_buffer_bytes})).collect();
    serde_json::to_string_pretty(&serde_json::json!({"views":views,"host_frame_interval_p95_us":p95(&state.frame_interval_micros),"device_gpu":state.device,"scene_tasks":lince_map::diagnostics::snapshot(),"measurement":"GPU counters cover the whole Bevy device; unavailable counters are unknown. Driver memory and display/GPU time are unmeasured. Timings retain at most 16384 samples; reset before a measured run."})).unwrap_or_default()
}

pub(super) fn maintain(world: &mut World) {
    world.init_resource::<Diagnostics>();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<MapSand>>()
        .iter(world)
        .filter(|owner| visible(world, *owner))
        .collect();
    world.init_resource::<ImagesOwned>();
    let images: BTreeMap<_, _> = owners
        .iter()
        .map(|owner| (*owner, world.get::<MapSand>(*owner).unwrap().image.id()))
        .collect();
    let retired: Vec<_> = world
        .resource::<ImagesOwned>()
        .0
        .iter()
        .filter(|(owner, image)| images.get(owner) != Some(image))
        .map(|(_, image)| *image)
        .collect();
    for image in retired {
        if let Some(mut assets) = world.get_resource_mut::<Assets<Image>>() {
            assets.remove(image);
        }
    }
    world.resource_mut::<ImagesOwned>().0 = images;
    let rows: Vec<_> = owners
        .iter()
        .map(|owner| {
            let sand = world.get::<MapSand>(*owner).unwrap();
            let bytes = sand.markers.capacity() * std::mem::size_of::<feed::Marker>()
                + sand.records.capacity() * std::mem::size_of::<String>()
                + sand.records.iter().map(String::capacity).sum::<usize>()
                + paging::retained_bytes(world, *owner);
            (
                format!("{owner:?}"),
                u64::from(sand.size[0]) * u64::from(sand.size[1]) * 4,
                bytes as u64,
            )
        })
        .collect();
    {
        let mut state = world
            .resource::<Diagnostics>()
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(readback) = world.get_resource::<crate::map_view::Readback>() {
            state.device = readback
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone();
        }
        state
            .views
            .retain(|owner, _| rows.iter().any(|(key, _, _)| key == owner));
        for (owner, textures, buffers) in rows {
            let entry = state.views.entry(owner).or_default();
            entry.tracked_output_texture_bytes = textures;
            entry.tracked_ui_buffer_bytes = buffers;
        }
        if !owners.is_empty()
            && let Some(time) = world.get_resource::<Time<Real>>()
        {
            sample(
                &mut state.frame_interval_micros,
                time.delta().as_micros().min(u128::from(u64::MAX)) as u64,
            );
        }
        if owners.is_empty() {
            state.frame_interval_micros = VecDeque::new();
        }
    }
    let panels: Vec<_> = owners
        .iter()
        .filter_map(|owner| {
            world
                .get::<MapSand>(*owner)
                .and_then(|sand| sand.diagnostics)
        })
        .collect();
    world.init_resource::<LastReport>();
    if !panels.is_empty()
        && world
            .resource::<LastReport>()
            .0
            .is_none_or(|last| last.elapsed() >= std::time::Duration::from_secs(1))
    {
        world.resource_mut::<LastReport>().0 = Some(Instant::now());
        let report = report(world);
        for panel in panels {
            if let Some(mut text) = world.get_mut::<Text>(panel)
                && text.0 != report
            {
                text.0 = report.clone();
            }
        }
    }
}
