use super::*;
use std::{
    collections::VecDeque,
    path::Path,
    sync::{Mutex, mpsc},
    thread,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub path: String,
    pub section: usize,
    pub width: u32,
    pub epoch: u64,
}

pub(super) struct Request {
    pub owner: Entity,
    pub key: Key,
    pub tiles: Vec<u32>,
    pub wake: Option<crate::wake::WakeSignal>,
}

pub(super) enum Job {
    Render(Request),
    Close(Entity),
}

pub(super) struct Rendered {
    pub info: Info,
    pub layout: Layout,
    pub tiles: Vec<(u32, lince_document::Tile)>,
}

pub(super) enum Reply {
    Render {
        owner: Entity,
        key: Key,
        result: Result<Rendered, String>,
    },
    Pick {
        owner: Entity,
        path: Option<String>,
    },
}

#[derive(Resource)]
pub(super) struct Worker {
    pub sender: mpsc::SyncSender<Job>,
    pub replies: Mutex<mpsc::Receiver<Reply>>,
    pub reply_sender: mpsc::SyncSender<Reply>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Job>(8);
        let (reply_sender, replies) = mpsc::sync_channel(2);
        let output = reply_sender.clone();
        thread::Builder::new()
            .name("document-reader".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let mut readers = VecDeque::<(Entity, String, u64, lince_document::Reader)>::new();
                while let Ok(job) = receiver.recv() {
                    let request = match job {
                        Job::Render(request) => request,
                        Job::Close(owner) => {
                            readers.retain(|(candidate, _, _, _)| *candidate != owner);
                            continue;
                        }
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let index = readers.iter().position(|(owner, path, epoch, _)| {
                            *owner == request.owner
                                && *path == request.key.path
                                && *epoch == request.key.epoch
                        });
                        let mut entry = if let Some(index) = index {
                            readers.remove(index).unwrap()
                        } else {
                            readers.retain(|(owner, _, _, _)| *owner != request.owner);
                            while readers.len() >= 2 {
                                readers.pop_front();
                            }
                            (
                                request.owner,
                                request.key.path.clone(),
                                request.key.epoch,
                                lince_document::Reader::open(Path::new(&request.key.path))?,
                            )
                        };
                        let reader = &mut entry.3;
                        let section = request.key.section.min(reader.info.sections.len() - 1);
                        let layout = reader.layout(section, request.key.width)?;
                        let mut tiles = Vec::new();
                        for index in request.tiles.iter().copied().take(10) {
                            if index < layout.height.div_ceil(lince_document::TILE_HEIGHT) {
                                tiles
                                    .push((index, reader.tile(section, request.key.width, index)?));
                            }
                        }
                        let rendered = Rendered {
                            info: reader.info.clone(),
                            layout,
                            tiles,
                        };
                        readers.push_back(entry);
                        Ok(rendered)
                    }))
                    .unwrap_or_else(|_| {
                        readers.clear();
                        Err("The document renderer could not read this file".into())
                    });
                    if output
                        .send(Reply::Render {
                            owner: request.owner,
                            key: request.key,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    if let Some(wake) = request.wake {
                        wake.ring();
                    }
                }
            })
            .expect("start document reader");
        Self {
            sender,
            replies: Mutex::new(replies),
            reply_sender,
        }
    }
}
