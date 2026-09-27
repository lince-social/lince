use super::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

#[derive(Component)]
struct Attachment {
    binding: RecordBinding,
    message: String,
    index: usize,
    descriptor: StoredPart,
    status: Entity,
    preview: Entity,
    pending: Option<String>,
    next: usize,
    data: String,
    loaded: Option<Arc<Vec<u8>>>,
    saving: bool,
}

#[derive(Resource, Default)]
struct Subscriptions(HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>);

enum Preview {
    Image(u32, u32, Vec<u8>),
    Text(String),
    Saved,
}

#[derive(Component)]
struct Decoding(Mutex<mpsc::Receiver<Result<Preview, String>>>);

#[derive(Component)]
struct Playing(mpsc::Sender<()>);
#[derive(Component)]
struct Snapshot(Arc<Vec<u8>>);
impl Drop for Playing {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

pub(super) fn create(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    message: &str,
    index: usize,
    descriptor: StoredPart,
) {
    let StoredPart::Attachment {
        name,
        mime_type,
        bytes,
        chunks,
        ..
    } = &descriptor
    else {
        return;
    };
    if *bytes > nucleus::message::MAX_CONTENT_BYTES || *chunks > 24 {
        return;
    }
    let owner = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
            crate::sand_store::SandCredits(CREDITS),
        ))
        .id();
    crate::edit_mode::label(
        world,
        owner,
        &format!("{name} · {mime_type} · {} bytes", bytes),
        13.0,
    );
    crate::description::button(world, owner, owner, "Preview / play", Load(false));
    crate::description::button(world, owner, owner, "Save attachment", Load(true));
    crate::description::button(world, owner, owner, "Close preview / stop", Close);
    let status = crate::edit_mode::label(world, owner, "", 12.0);
    let preview = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                max_width: px(480),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.entity_mut(owner).insert(Attachment {
        binding: binding.clone(),
        message: message.into(),
        index,
        descriptor,
        status,
        preview,
        pending: None,
        next: 0,
        data: String::new(),
        loaded: None,
        saving: false,
    });
}

pub(super) fn draft_preview(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    part: &MessagePart,
) {
    let MessagePart::Attachment { data, .. } = part else {
        return;
    };
    let Ok(mut descriptors) = nucleus::message::describe(std::slice::from_ref(part)) else {
        return;
    };
    let Ok(bytes) = BASE64_STANDARD.decode(data) else {
        return;
    };
    create(world, parent, binding, "", 0, descriptors.remove(0));
    let Some(owner) = world
        .get::<Children>(parent)
        .and_then(|children| children.last().copied())
    else {
        return;
    };
    let bytes = Arc::new(bytes);
    if let Some(mut card) = world.get_mut::<Attachment>(owner) {
        card.loaded = Some(bytes.clone());
    }
    world.entity_mut(owner).insert(Snapshot(bytes));
}

fn request(world: &mut World, owner: Entity) -> Result<(), String> {
    let card = world.get::<Attachment>(owner).ok_or("Attachment closed.")?;
    let sender = crate::protein_area::editor_sender(world, &card.binding)
        .ok_or("The message source is disconnected.")?;
    let id = nucleus::new_uid("attachment");
    let protein = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":card.message}],"fields":["uid"],"include":{"extension":{"namespace":nucleus::message::chunk_namespace(card.index, card.next)}},"limit":1})).map_err(|error| error.to_string())?;
    sender
        .try_send(ClientMessage::Subscribe {
            id: id.clone(),
            protein,
        })
        .map_err(|error| error.to_string())?;
    world.init_resource::<Subscriptions>();
    world
        .resource_mut::<Subscriptions>()
        .0
        .insert(id.clone(), sender);
    world.get_mut::<Attachment>(owner).unwrap().pending = Some(id);
    Ok(())
}

fn error(world: &mut World, owner: Entity, error: String) {
    let Some(mut card) = world.get_mut::<Attachment>(owner) else {
        return;
    };
    card.pending = None;
    card.data.clear();
    card.next = 0;
    let status = card.status;
    world.get_mut::<Text>(status).unwrap().0 = error;
}

#[derive(Clone)]
struct Load(bool);
impl Action for Load {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<Decoding>(owner).is_some() {
            return;
        }
        if let Some(bytes) = world
            .get::<Snapshot>(owner)
            .map(|snapshot| snapshot.0.clone())
        {
            if let Some(mut card) = world.get_mut::<Attachment>(owner) {
                card.loaded = Some(bytes);
            }
        }
        let Some(mut card) = world.get_mut::<Attachment>(owner) else {
            return;
        };
        if card.pending.is_some() {
            return;
        }
        card.saving = self.0;
        let status = card.status;
        if card.loaded.is_some() {
            render(world, owner);
            return;
        }
        card.next = 0;
        card.data.clear();
        world.get_mut::<Text>(status).unwrap().0 = "Loading attachment…".into();
        if let Err(message) = request(world, owner) {
            error(world, owner, message);
        }
    }
}

#[derive(Clone)]
struct Close;
impl Action for Close {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut card) = world.get_mut::<Attachment>(owner) else {
            return;
        };
        card.loaded = None;
        card.pending = None;
        card.data.clear();
        let preview = card.preview;
        world.entity_mut(preview).despawn_children();
        world
            .entity_mut(owner)
            .remove::<Playing>()
            .remove::<Decoding>();
    }
}

fn decode_image(bytes: &[u8]) -> Result<Preview, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| error.to_string())?
        .into_rgba8();
    if image.width() == 0 || image.height() == 0 {
        return Err("This image has no pixels.".into());
    }
    Ok(Preview::Image(
        image.width(),
        image.height(),
        image.into_raw(),
    ))
}

fn audio(bytes: &[u8]) -> Result<crate::sound::library::Clip, String> {
    let mut reader =
        hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels == 0
        || spec.channels > 8
        || !(8000..=192000).contains(&spec.sample_rate)
        || reader.duration() > spec.sample_rate * 120
    {
        return Err("Audio playback supports WAV files up to two minutes.".into());
    }
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?
    } else {
        let scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
        reader
            .samples::<i32>()
            .map(|sample| sample.map(|sample| sample as f32 / scale))
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?
    };
    let samples = samples
        .chunks(usize::from(spec.channels))
        .map(|frame| {
            frame
                .iter()
                .copied()
                .filter(|sample| sample.is_finite())
                .sum::<f32>()
                / f32::from(spec.channels)
        })
        .collect();
    Ok(crate::sound::library::Clip {
        samples,
        rate: spec.sample_rate,
    })
}

fn render(world: &mut World, owner: Entity) {
    let Some(card) = world.get::<Attachment>(owner) else {
        return;
    };
    let Some(bytes) = card.loaded.clone() else {
        return;
    };
    let StoredPart::Attachment {
        name, mime_type, ..
    } = &card.descriptor
    else {
        return;
    };
    let name = name.clone();
    let mime = mime_type.clone();
    let saving = card.saving;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    world
        .entity_mut(owner)
        .insert(Decoding(Mutex::new(receiver)));
    if mime == "audio/wav" && !saving {
        world.entity_mut(owner).insert(Playing(stop));
    }
    std::thread::spawn(move || {
        let result = (|| {
            if saving {
                if let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                    std::fs::write(path, bytes.as_slice()).map_err(|error| error.to_string())?;
                }
                Ok(Preview::Saved)
            } else if mime.starts_with("image/") {
                decode_image(&bytes)
            } else if mime == "audio/wav" {
                let clip = audio(&bytes)?;
                let duration = std::time::Duration::from_secs_f64(
                    clip.samples.len() as f64 / f64::from(clip.rate) + 0.1,
                );
                let output = crate::sound::device::Output::open()?;
                output.play(owner, Arc::new(clip), 1.0, true);
                let _ = stopped.recv_timeout(duration);
                Ok(Preview::Text("Playback ended.".into()))
            } else if mime.starts_with("text/") || mime == "application/json" {
                Ok(Preview::Text(
                    String::from_utf8_lossy(&bytes)
                        .chars()
                        .take(16_384)
                        .collect(),
                ))
            } else {
                Ok(Preview::Text(
                    "Save this attachment to open it in an application that supports its format."
                        .into(),
                ))
            }
        })();
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| cursor.read(messages).map(|event| event.0.clone()).collect())
        .unwrap_or_default();
    for event in events {
        let (id, result) = match event {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
                (id, Ok(rows))
            }
            ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owner = world
            .query::<(Entity, &Attachment)>()
            .iter(world)
            .find(|(_, card)| card.pending.as_deref() == Some(&id))
            .map(|(owner, _)| owner);
        let Some(owner) = owner else { continue };
        if let Some(sender) = world
            .get_resource_mut::<Subscriptions>()
            .and_then(|mut subs| subs.0.remove(&id))
        {
            let _ = sender.try_send(ClientMessage::Unsubscribe { id });
        }
        let result = result.and_then(|rows| {
            let mut card = world.get_mut::<Attachment>(owner).unwrap();
            card.pending = None;
            let data = rows
                .iter()
                .find(|row| row["uid"] == card.message)
                .and_then(|row| row["extension"]["data"].as_str())
                .ok_or("The attachment is unavailable or still syncing.")?;
            if data.len() > nucleus::message::CHUNK_BYTES {
                return Err("Attachment chunk exceeds its size limit.".into());
            }
            card.data.push_str(data);
            card.next += 1;
            let StoredPart::Attachment {
                chunks,
                bytes,
                sha256,
                ..
            } = &card.descriptor
            else {
                return Err("Invalid attachment.".into());
            };
            if card.next < *chunks {
                return Ok(false);
            }
            let decoded = nucleus::message::decode(&card.data)?;
            if decoded.len() != *bytes || nucleus::message::digest(&decoded) != *sha256 {
                return Err("Attachment contents are incomplete or changed.".into());
            }
            card.loaded = Some(Arc::new(decoded));
            card.data.clear();
            Ok(true)
        });
        match result {
            Ok(true) => render(world, owner),
            Ok(false) => {
                if let Err(message) = request(world, owner) {
                    error(world, owner, message);
                }
            }
            Err(message) => error(world, owner, message),
        }
    }
    let ready: Vec<_> = world
        .query::<(Entity, &Decoding)>()
        .iter(world)
        .filter_map(|(owner, pending)| {
            pending
                .0
                .lock()
                .ok()?
                .try_recv()
                .ok()
                .map(|result| (owner, result))
        })
        .collect();
    for (owner, result) in ready {
        world.entity_mut(owner).remove::<Decoding>();
        let Some(card) = world.get::<Attachment>(owner) else {
            continue;
        };
        let (preview, status) = (card.preview, card.status);
        world.get_mut::<Text>(status).unwrap().0.clear();
        world.entity_mut(preview).despawn_children();
        match result {
            Ok(Preview::Image(width, height, rgba)) => {
                let image = Image::new(
                    bevy::render::render_resource::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    rgba,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                let handle = world.resource_mut::<Assets<Image>>().add(image);
                world.spawn((
                    ImageNode::new(handle),
                    Node {
                        width: px(width.min(480) as f32),
                        height: px(height as f32 * width.min(480) as f32 / width as f32),
                        ..default()
                    },
                    ChildOf(preview),
                ));
            }
            Ok(Preview::Text(text)) => {
                crate::edit_mode::label(world, preview, &text, 13.0);
            }
            Ok(Preview::Saved) => {
                world.get_mut::<Text>(status).unwrap().0 = "Save dialog closed.".into()
            }
            Err(message) => error(world, owner, message),
        }
    }
    let active: std::collections::HashSet<_> = world
        .query::<&Attachment>()
        .iter(world)
        .filter_map(|card| card.pending.clone())
        .collect();
    if let Some(mut subs) = world.get_resource_mut::<Subscriptions>() {
        subs.0.retain(|id, sender| {
            active.contains(id)
                || (sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_err()
                    && !sender.is_closed())
        });
    }
}
