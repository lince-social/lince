use bevy::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{Mutex, mpsc},
};

use crate::app::{Intent, Mobile, button, label};

enum Status {
    Loading,
    Ready(Handle<Image>, u32),
    Failed(String),
}

struct Job {
    scope: String,
    source: String,
    receiver: Mutex<mpsc::Receiver<Result<lince_interface::images::Pixels, String>>>,
}

#[derive(Resource, Default)]
struct Images {
    scope: String,
    entries: BTreeMap<String, Status>,
    jobs: Vec<Job>,
}

fn synchronize(world: &mut World) {
    world.init_resource::<Images>();
    let state = world.resource::<Mobile>();
    let scope = format!("{}:{:?}", state.scope_key(), state.navigation.current);
    let mut images = world.resource_mut::<Images>();
    if images.scope != scope {
        images.scope = scope;
        images.entries.clear();
    }
}

pub fn request(world: &mut World, source: String) -> Result<(), String> {
    synchronize(world);
    let images = world.resource::<Images>();
    if images.jobs.len() >= 2 {
        return Err("Two images are loading. Try this image again shortly.".into());
    }
    if images
        .entries
        .get(&source)
        .is_some_and(|status| !matches!(status, Status::Failed(_)))
    {
        return Ok(());
    }
    let scope = images.scope.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let wake = world
        .get_resource::<lince_interface::wake::WakeSignal>()
        .cloned();
    let requested = source.clone();
    std::thread::Builder::new()
        .name("lince-image".into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| runtime.block_on(lince_interface::images::load(&requested)));
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        })
        .map_err(|error| error.to_string())?;
    let mut images = world.resource_mut::<Images>();
    if images.entries.len() >= 4 {
        if let Some(key) = images
            .entries
            .iter()
            .find(|(_, status)| !matches!(status, Status::Loading))
            .map(|(key, _)| key.clone())
        {
            images.entries.remove(&key);
        }
    }
    images.entries.insert(source.clone(), Status::Loading);
    images.jobs.push(Job {
        scope,
        source,
        receiver: Mutex::new(receiver),
    });
    Ok(())
}

pub fn poll(world: &mut World) {
    synchronize(world);
    let mut completed = Vec::new();
    let scope = world.resource::<Images>().scope.clone();
    world.resource_mut::<Images>().jobs.retain(|job| {
        let result = match job.receiver.lock() {
            Ok(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("Image loader stopped".into())),
            },
            Err(_) => Some(Err("Image loader stopped".into())),
        };
        if let Some(result) = result {
            if job.scope == scope {
                completed.push((job.source.clone(), result));
            }
            false
        } else {
            true
        }
    });
    for (source, result) in completed {
        let status = match result {
            Ok(pixels) => {
                world.init_resource::<Assets<Image>>();
                let image = Image::new(
                    bevy::render::render_resource::Extent3d {
                        width: pixels.width,
                        height: pixels.height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    pixels.rgba,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                Status::Ready(
                    world.resource_mut::<Assets<Image>>().add(image),
                    pixels.width,
                )
            }
            Err(error) => Status::Failed(error),
        };
        world
            .resource_mut::<Images>()
            .entries
            .insert(source, status);
        world.resource_mut::<Mobile>().dirty = true;
    }
}

pub fn render(world: &mut World, parent: Entity, source: &str) {
    synchronize(world);
    let status = world.resource::<Images>().entries.get(source);
    match status {
        Some(Status::Ready(image, width)) => {
            let image = image.clone();
            let width = *width;
            world.spawn((
                ChildOf(parent),
                ImageNode::new(image),
                Node {
                    width: px(width as f32),
                    max_width: percent(100),
                    height: Val::Auto,
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
        }
        Some(Status::Loading) => {
            label(world, parent, "Loading image…", 14.0);
        }
        _ => {
            if let Some(Status::Failed(error)) = status {
                let error = error.clone();
                label(world, parent, &error, 14.0);
            }
            button(
                world,
                parent,
                if source.starts_with("data:") {
                    "Show image"
                } else {
                    "Load image from website"
                },
                Intent::LoadImage(source.into()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_changes_drop_cached_images_and_ignore_old_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::new();
        world.insert_resource(Mobile::new(directory.path().into()));
        synchronize(&mut world);
        let (sender, receiver) = mpsc::sync_channel(1);
        {
            let mut images = world.resource_mut::<Images>();
            let scope = images.scope.clone();
            images.entries.insert("private".into(), Status::Loading);
            images.jobs.push(Job {
                scope,
                source: "private".into(),
                receiver: Mutex::new(receiver),
            });
        }
        world.resource_mut::<Mobile>().navigation.current = crate::navigation::Page::Organ;
        poll(&mut world);
        assert!(world.resource::<Images>().entries.is_empty());
        assert_eq!(world.resource::<Images>().jobs.len(), 1);
        sender.send(Err("Private download failure".into())).unwrap();
        poll(&mut world);
        assert!(world.resource::<Images>().entries.is_empty());
        assert!(world.resource::<Images>().jobs.is_empty());
    }

    #[test]
    fn session_lock_drops_images_and_downloads_stay_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::new();
        world.insert_resource(Mobile::new(directory.path().into()));
        world.resource_mut::<Mobile>().identity = crate::session::Identity::Person("one".into());
        synchronize(&mut world);
        let mut senders = Vec::new();
        for index in 0..2 {
            let (sender, receiver) = mpsc::sync_channel(1);
            senders.push(sender);
            let mut images = world.resource_mut::<Images>();
            let scope = images.scope.clone();
            images.entries.insert(index.to_string(), Status::Loading);
            images.jobs.push(Job {
                scope,
                source: index.to_string(),
                receiver: Mutex::new(receiver),
            });
        }
        world.resource_mut::<Mobile>().identity = crate::session::Identity::Locked;
        poll(&mut world);
        assert!(world.resource::<Images>().entries.is_empty());
        assert_eq!(world.resource::<Images>().jobs.len(), 2);
        assert!(request(&mut world, "https://example.com/third.png".into()).is_err());
        assert_eq!(world.resource::<Images>().jobs.len(), 2);
        drop(senders);
        poll(&mut world);
        assert!(world.resource::<Images>().jobs.is_empty());
        assert!(world.resource::<Images>().entries.is_empty());
    }
}
