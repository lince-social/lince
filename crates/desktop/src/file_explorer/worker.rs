use bevy::prelude::*;
use std::sync::{Arc, Mutex, mpsc};

type Completion = Box<dyn FnOnce(&mut World) + Send>;
type Job = Box<dyn FnOnce() -> (Completion, Option<crate::wake::WakeSignal>) + Send>;

#[derive(Resource)]
pub(crate) struct Worker {
    sender: mpsc::SyncSender<Job>,
    replies: Mutex<mpsc::Receiver<Completion>>,
    output: mpsc::SyncSender<Completion>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Job>(16);
        let (output, replies) = mpsc::sync_channel::<Completion>(2);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..2 {
            let receiver = receiver.clone();
            let output = output.clone();
            std::thread::Builder::new()
                .name(format!("file-service-{index}"))
                .spawn(move || {
                    loop {
                        let job = receiver.lock().expect("file jobs").recv();
                        let Ok(job) = job else {
                            break;
                        };
                        let (reply, wake) = job();
                        if output.send(reply).is_err() {
                            break;
                        }
                        if let Some(wake) = wake {
                            wake.ring();
                        }
                    }
                })
                .expect("start file service");
        }
        Self {
            sender,
            replies: Mutex::new(replies),
            output,
        }
    }
}

pub(crate) fn stream<T: Send + 'static>(
    world: &World,
    work: impl FnOnce(&mut dyn FnMut(T) -> bool) + Send + 'static,
    apply: impl Fn(&mut World, T) + Send + Sync + 'static,
) -> Result<(), String> {
    let output = world
        .get_resource::<Worker>()
        .ok_or("The file service is unavailable")?
        .output
        .clone();
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let apply = Arc::new(apply);
    run(
        world,
        move || {
            work(&mut |result| {
                let apply = apply.clone();
                if output
                    .send(Box::new(move |world| apply(world, result)))
                    .is_err()
                {
                    return false;
                }
                if let Some(wake) = &wake {
                    wake.ring();
                }
                true
            });
        },
        |_, ()| {},
    )
}

pub(crate) fn run<T: Send + 'static>(
    world: &World,
    work: impl FnOnce() -> T + Send + 'static,
    apply: impl FnOnce(&mut World, T) + Send + 'static,
) -> Result<(), String> {
    let worker = world
        .get_resource::<Worker>()
        .ok_or("The file service is unavailable")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    worker
        .sender
        .try_send(Box::new(move || {
            let result = work();
            (Box::new(move |world| apply(world, result)), wake)
        }))
        .map_err(|_| "The file service is busy; try again".into())
}

pub(crate) fn poll(world: &mut World) {
    let replies: Vec<_> = world
        .resource::<Worker>()
        .replies
        .lock()
        .expect("file replies")
        .try_iter()
        .take(16)
        .collect();
    for reply in replies {
        reply(world);
    }
}
