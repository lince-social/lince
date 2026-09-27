use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Write, sync::Arc};

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
struct Saved {
    body: String,
    parts: Vec<MessagePart>,
}

#[derive(Component)]
struct Persistent {
    path: PathBuf,
    last: Saved,
    revision: u64,
}

#[derive(Resource)]
struct Writer {
    send: mpsc::SyncSender<(PathBuf, Saved)>,
    errors: Arc<Mutex<Vec<(PathBuf, String)>>>,
    task: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Writer {
    fn drop(&mut self) {
        let (sender, _) = mpsc::sync_channel(0);
        drop(std::mem::replace(&mut self.send, sender));
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

pub(super) fn attach(world: &mut World, owner: Entity, thread: &str, binding: &RecordBinding) {
    let Some(directory) = world
        .get_resource::<crate::workspace::WorkspaceFile>()
        .map(|file| file.directory().join("message-drafts"))
    else {
        return;
    };
    let key = nucleus::message::digest(format!("{:?}:{thread}", binding.source).as_bytes());
    let path = directory.join(format!("{key}.json"));
    let saved = (|| {
        if !path.exists() {
            return Ok(Saved::default());
        }
        let file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("Saved draft exceeds its size limit.".into());
        }
        let saved: Saved = serde_json::from_reader(file).map_err(|error| error.to_string())?;
        nucleus::message::validate(&saved.parts)?;
        if saved.body.len() > 65_536 {
            return Err("Saved draft text exceeds its size limit.".into());
        }
        Ok(saved)
    })();
    let saved = match saved {
        Ok(saved) => saved,
        Err(error) => {
            if let Some(status) = world.get::<Draft>(owner).map(|draft| draft.status) {
                world.get_mut::<Text>(status).unwrap().0 = error;
            }
            return;
        }
    };
    let Some(mut draft) = world.get_mut::<Draft>(owner) else {
        return;
    };
    draft.parts = saved.parts.clone();
    let input = draft.input;
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&saved.body);
    render_draft(world, owner);
    let revision = world.get::<Draft>(owner).unwrap().revision;
    world.entity_mut(owner).insert(Persistent {
        path,
        last: saved,
        revision,
    });
    if !world.contains_resource::<Writer>() {
        let (send, received) = mpsc::sync_channel::<(PathBuf, Saved)>(16);
        let errors = Arc::new(Mutex::new(Vec::new()));
        let results = errors.clone();
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let task = std::thread::spawn(move || {
            while let Ok((path, saved)) = received.recv() {
                let mut pending = HashMap::from([(path, saved)]);
                for (path, saved) in received.try_iter().take(15) {
                    pending.insert(path, saved);
                }
                for (path, saved) in pending {
                    if let Err(error) = write(&path, &saved) {
                        let mut errors = results.lock().unwrap();
                        if errors.len() < 16 {
                            errors.push((path, error));
                        }
                    }
                }
                if let Some(wake) = &wake {
                    wake.ring();
                }
            }
        });
        world.insert_resource(Writer {
            send,
            errors,
            task: Some(task),
        });
    }
}

fn write(path: &std::path::Path, saved: &Saved) -> Result<(), String> {
    nucleus::message::validate(&saved.parts)?;
    if saved.body.len() > 65_536 {
        return Err("Draft text exceeds 64 KiB.".into());
    }
    if saved.body.is_empty() && saved.parts.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    let parent = path.parent().ok_or("Draft directory is unavailable.")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    if !path.exists()
        && std::fs::read_dir(parent)
            .map_err(|error| error.to_string())?
            .take(129)
            .count()
            >= 128
    {
        return Err(
            "128 message drafts are already saved. Send or clear a draft before saving more."
                .into(),
        );
    }
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut file, saved).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn clear(world: &mut World, owner: Entity) {
    if let Some(path) = world
        .get::<Persistent>(owner)
        .map(|draft| draft.path.clone())
    {
        if let Some(writer) = world.get_resource::<Writer>() {
            let _ = writer.send.try_send((path, Saved::default()));
        }
        if let Some(mut saved) = world.get_mut::<Persistent>(owner) {
            saved.revision = u64::MAX;
        }
    }
}

pub(super) fn update(world: &mut World) {
    let Some(writer) = world.get_resource::<Writer>() else {
        return;
    };
    let errors = std::mem::take(&mut *writer.errors.lock().unwrap());
    let sender = writer.send.clone();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Persistent>>()
        .iter(world)
        .collect();
    for owner in owners {
        let saved = world.get::<Persistent>(owner).unwrap();
        let Some(draft) = world.get::<Draft>(owner) else {
            continue;
        };
        if let Some((_, error)) = errors.iter().find(|(path, _)| *path == saved.path) {
            let status = draft.status;
            world.get_mut::<Text>(status).unwrap().0 = format!("Draft could not be saved: {error}");
            continue;
        }
        let body = world
            .get::<EditableText>(draft.input)
            .map(|input| input.value().to_string())
            .unwrap_or_default();
        if body == saved.last.body && draft.revision == saved.revision {
            continue;
        }
        let revision = draft.revision;
        let next = Saved {
            body,
            parts: draft.parts.clone(),
        };
        if next != saved.last && sender.try_send((saved.path.clone(), next.clone())).is_err() {
            continue;
        }
        let mut saved = world.get_mut::<Persistent>(owner).unwrap();
        saved.last = next;
        saved.revision = revision;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_draft_reopens_and_clears_without_a_message_record() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("draft.json");
        let saved = Saved {
            body: "Unsent".into(),
            parts: vec![MessagePart::Attachment {
                name: "image.png".into(),
                mime_type: "image/png".into(),
                data: "YWJj".into(),
            }],
        };
        write(&path, &saved).unwrap();
        let restored: Saved = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(restored == saved);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
                0
            );
        }
        write(&path, &Saved::default()).unwrap();
        assert!(!path.exists());
    }
}
