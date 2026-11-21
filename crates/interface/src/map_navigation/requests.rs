use super::{
    Panel,
    panel::{Intent, button},
};
use crate::{
    controls::{clear, status, value},
    wake::WakeSignal,
};
use bevy::prelude::*;
use chrono::DateTime;
use lince_map::{
    navigation::{Geocoder, Place, Route, Router},
    pack::Manifest,
    region::{Definition, Progress, prepare},
};
use nucleus::execution::now;
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};
use tokio::{
    sync::oneshot::{Receiver, channel, error::TryRecvError},
    task::JoinHandle,
};

#[derive(Clone, Copy)]
pub(super) enum Lookup {
    Search,
    Reverse,
    Route,
}

enum Response {
    Places(Vec<Place>),
    Route(Route),
    Region(Manifest),
}

struct RegionWork {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Progress>,
}

pub(super) struct Job {
    task: JoinHandle<()>,
    response: Receiver<Result<Response, String>>,
    generation: u64,
    region: Option<RegionWork>,
}

impl Job {
    fn launch(
        world: &mut World,
        entity: Entity,
        wake: Option<WakeSignal>,
        region: Option<RegionWork>,
        request: impl Future<Output = Result<Response, String>> + Send + 'static,
    ) -> Result<(), String> {
        let panel = &mut *Panel::get_mut(world, entity)?;
        let lookup = region.is_none();
        let runtime = panel.runtime.as_ref().ok_or(if lookup {
            "Map provider runtime unavailable"
        } else {
            "Map runtime unavailable"
        })?;
        panel.job = None;
        let generation = panel.selection.invalidate();
        panel.changed |= lookup;
        let (sender, response) = channel();
        let task = runtime.spawn(async move {
            let _ = sender.send(request.await);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        panel.job = Some(Self {
            task,
            response,
            generation,
            region,
        });
        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(region) = &self.region {
            region.cancelled.store(true, Relaxed);
        }
        self.task.abort();
    }
}

pub(super) fn prepare_region(world: &mut World, entity: Entity) -> Result<(), String> {
    let panel = world.get::<Panel>(entity).ok_or("Map navigation closed")?;
    let (coverage, store) = panel
        .region
        .clone()
        .ok_or("Draw a region and configure a local or optional download source first")?;
    let [_, _, _, _, _, regional @ .., _] = panel.configuration_values(world)?;
    let [dataset, date, source, license, credits, output, min, max] = regional;
    let definition = Definition {
        dataset,
        date,
        source,
        license,
        credits,
        coverage,
        min_zoom: min.parse().map_err(|_| "Choose minimum zoom 0–14")?,
        max_zoom: max.parse().map_err(|_| "Choose maximum zoom 0–14")?,
    };
    definition.tiles()?;
    let output = PathBuf::from(output);
    let wake = world.get_resource::<WakeSignal>().cloned();
    let cancelled = Arc::<AtomicBool>::default();
    let progress = Arc::new(Progress {
        wake: wake.clone().map(|wake| Arc::new(move || wake.ring()) as _),
        ..default()
    });
    let (cancel, counts) = (cancelled.clone(), progress.clone());
    Job::launch(
        world,
        entity,
        wake,
        Some(RegionWork {
            cancelled,
            progress,
        }),
        async move {
            prepare(store, definition, output, cancel, counts)
                .await
                .map(Response::Region)
        },
    )
}

pub(super) fn lookup(world: &mut World, entity: Entity, lookup: Lookup) -> Result<(), String> {
    let panel = world.get::<Panel>(entity).ok_or("Map navigation closed")?;
    let provider = panel.provider.clone().ok_or("Configure providers first")?;
    let query = value(world, panel.query)?;
    let Panel {
        point,
        origin,
        destination,
        mode,
        ..
    } = *panel;
    let wake = world.get_resource::<WakeSignal>().cloned();
    Job::launch(world, entity, wake, None, async move {
        match lookup {
            Lookup::Search => provider.search(&query).await.map(Response::Places),
            Lookup::Reverse => provider
                .reverse(point)
                .await
                .map(|place| Response::Places(vec![place])),
            Lookup::Route => {
                let (Some(origin), Some(destination)) = (origin, destination) else {
                    return Err("Select start and destination from visible map points".into());
                };
                provider
                    .route(&[origin, destination], mode, now().timestamp())
                    .await
                    .map(Response::Route)
            }
        }
    })
}

pub(super) fn poll(world: &mut World) {
    let pending = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter_map(|(entity, panel)| panel.job.is_some().then_some(entity))
        .collect::<Vec<_>>();
    for entity in pending {
        let Some(mut panel) = world.get_mut::<Panel>(entity) else {
            continue;
        };
        let Some(mut job) = panel.job.take() else {
            continue;
        };
        let Panel {
            status: status_entity,
            results,
            ..
        } = *panel;
        if let Some(region) = &job.region {
            let progress = &region.progress;
            status(
                world,
                status_entity,
                format!(
                    "Region: {}/{} tiles · {} bytes",
                    progress.completed.load(Relaxed),
                    progress.total.load(Relaxed),
                    progress.bytes.load(Relaxed),
                ),
            );
        }
        let response = match job.response.try_recv() {
            Ok(response) => response,
            Err(TryRecvError::Empty) => {
                if let Some(mut panel) = world.get_mut::<Panel>(entity) {
                    panel.job = Some(job);
                }
                continue;
            }
            Err(TryRecvError::Closed) => Err("Provider request cancelled".into()),
        };
        clear(world, results);
        match response {
            Ok(Response::Places(places)) => {
                let Some(mut panel) = world.get_mut::<Panel>(entity) else {
                    continue;
                };
                if !panel.selection.accept_places(job.generation, places) {
                    continue;
                }
                let captions = panel
                    .selection
                    .places
                    .iter()
                    .map(|place| format!("Choose {} · {}", place.name, place.credits))
                    .collect::<Vec<_>>();
                let caption = if captions.is_empty() {
                    "No places in provider coverage"
                } else {
                    "Choose a result; nothing has been saved"
                };
                for (index, caption) in captions.iter().enumerate() {
                    button(world, results, entity, caption, Intent::Choose(index));
                }
                status(world, status_entity, caption);
            }
            Ok(Response::Route(route)) => {
                let caption = route_caption(&route);
                if let Some(mut panel) = world.get_mut::<Panel>(entity)
                    && panel.selection.accept_route(job.generation, route)
                {
                    panel.changed = true;
                    status(world, status_entity, caption);
                }
            }
            Ok(Response::Region(manifest)) => status(
                world,
                status_entity,
                format!(
                    "Regional pack ready: {} · {} files · {}",
                    manifest.dataset,
                    manifest.files.len(),
                    manifest.credits
                ),
            ),
            Err(error) => status(world, status_entity, error),
        }
    }
}

fn route_caption(route: &Route) -> String {
    let date = |time| DateTime::from_timestamp(time, 0).map(|date| date.to_rfc3339());
    let calculated = date(route.calculated_at).unwrap_or_else(|| "unknown time".into());
    let arrival = route
        .calculated_at
        .checked_add(route.duration_seconds.ceil() as i64)
        .and_then(date)
        .unwrap_or_else(|| "unavailable".into());
    format!(
        "{:?}: {:.1} km · {:.0} min estimate · calculated {calculated} · {} · estimated arrival {arrival}",
        route.mode,
        route.distance_metres / 1000.0,
        route.duration_seconds / 60.0,
        route.credits,
    )
}
