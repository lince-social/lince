use bevy::prelude::*;
use lince_resources::Resource;
use std::sync::{Mutex, mpsc};

struct Job {
    owner: Entity,
    resource: Resource,
    wake: Option<crate::native::wake::WakeSignal>,
}

#[derive(bevy::prelude::Resource)]
struct Worker {
    sender: mpsc::SyncSender<Job>,
    receiver: Mutex<mpsc::Receiver<(Entity, Result<crate::native::media_sand::Pixels, String>)>>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Job>(16);
        let (replies, receiver) = mpsc::sync_channel(16);
        std::thread::Builder::new()
            .name("resource-preview".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let library =
                            engine::sources::library().map_err(|error| error.to_string())?;
                        if job.resource.media_type.starts_with("image/") {
                            crate::native::media_sand::decode_resource(&library, &job.resource)
                        } else if job.resource.media_type.starts_with("video/") {
                            Ok(crate::native::media_sand::Pixels {
                                width: 512,
                                height: 288,
                                rgba: lince_resources::video::preview(
                                    &library,
                                    &job.resource,
                                    512,
                                    288,
                                )?,
                            })
                        } else {
                            let mut reader =
                                lince_document::Reader::open_resource(library, &job.resource)?;
                            let tile = reader.excerpt(&job.resource.locator, 512)?;
                            Ok(crate::native::media_sand::Pixels {
                                width: tile.width,
                                height: tile.height,
                                rgba: tile.rgba,
                            })
                        }
                    }))
                    .unwrap_or_else(|_| Err("The source could not be previewed".into()));
                    if replies.send((job.owner, result)).is_err() {
                        break;
                    }
                    if let Some(wake) = job.wake {
                        wake.ring();
                    }
                }
            })
            .expect("start shared resource previews");
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }
}

pub(crate) fn spawn(world: &mut World, parent: Entity, resource: &Resource) {
    if !(resource.media_type.starts_with("image/")
        || resource.media_type.starts_with("video/")
        || matches!(
            resource.media_type.as_str(),
            "application/pdf" | "application/epub+zip"
        ))
    {
        return;
    }
    world.init_resource::<Worker>();
    let owner = crate::native::edit_mode::label(world, parent, "Loading source preview…", 12.0);
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    if world
        .resource::<Worker>()
        .sender
        .try_send(Job {
            owner,
            resource: resource.clone(),
            wake,
        })
        .is_err()
    {
        world.get_mut::<Text>(owner).unwrap().0 = "Open the source to read this reference.".into();
    }
}

pub(crate) fn update(world: &mut World) {
    let replies = world
        .get_resource::<Worker>()
        .map(|worker| {
            worker
                .receiver
                .lock()
                .unwrap()
                .try_iter()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (owner, result) in replies {
        let Some(parent) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            continue;
        };
        match result {
            Ok(pixels) => {
                let ratio = pixels.width as f32 / pixels.height as f32;
                let image = crate::native::media_sand::image(world, pixels);
                world.spawn((
                    ImageNode::new(image),
                    Node {
                        width: percent(100),
                        max_height: px(320),
                        aspect_ratio: Some(ratio),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(parent),
                ));
                world.despawn(owner);
            }
            Err(error) => {
                world.get_mut::<Text>(owner).unwrap().0 = format!("Preview unavailable: {error}")
            }
        }
    }
}
