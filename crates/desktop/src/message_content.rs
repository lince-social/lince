mod attachments;
mod drafts;
#[cfg(test)]
mod tests;

use crate::{actions::Action, protein_area::RecordBinding};
use base64::{Engine, prelude::BASE64_STANDARD};
use bevy::{prelude::*, text::EditableText};
use nucleus::message::{MessagePart, StoredPart};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Mutex, mpsc},
};

const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../licenses/image-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../licenses/base64-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "rfd",
        author: "The rfd contributors",
        license: include_str!("../licenses/document/rfd-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../licenses/cpal-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "hound",
        author: "Ruud van Asseldonk",
        license: include_str!("../licenses/hound-Apache-2.0.txt"),
    },
];

#[derive(Component)]
pub(crate) struct Draft {
    revision: u64,
    binding: RecordBinding,
    pub parts: Vec<MessagePart>,
    list: Entity,
    status: Entity,
    reference: Option<Entity>,
    pub input: Entity,
    pub locked: bool,
}

#[derive(Component)]
struct Picking(Mutex<mpsc::Receiver<Result<Vec<MessagePart>, String>>>);

#[derive(Component)]
struct ContentView {
    parent: Entity,
    binding: RecordBinding,
    value: serde_json::Value,
}

pub(crate) fn view(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    binding: &RecordBinding,
    data: &serde_json::Value,
) {
    let parent = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world.entity_mut(owner).insert(ContentView {
        parent,
        binding: binding.clone(),
        value: serde_json::Value::Null,
    });
    refresh(world, owner, data);
}

pub(crate) fn refresh(world: &mut World, owner: Entity, data: &serde_json::Value) {
    let Some(mut view) = world.get_mut::<ContentView>(owner) else {
        return;
    };
    let value = serde_json::json!([data["content"], data["progress"]]);
    if view.value == value {
        return;
    }
    view.value = value;
    let (parent, binding) = (view.parent, view.binding.clone());
    world.entity_mut(parent).despawn_children();
    display(world, parent, &binding, data);
}

pub struct Plugin;
impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            crate::message_commands::keyboard.before(bevy::text::EditableTextSystems),
        );
        app.add_systems(
            Update,
            (
                crate::speech::update.after(crate::cell_bridge::ReceiveCell),
                drafts::update,
                poll,
                dropped,
                crate::message_commands::refresh,
                crate::directory_list::poll,
                attachments::receive.after(crate::cell_bridge::ReceiveCell),
            ),
        );
    }
}

pub(crate) fn draft(
    world: &mut World,
    owner: Entity,
    input: Entity,
    status: Entity,
    thread: &str,
    binding: &RecordBinding,
    private: bool,
) {
    let bar = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                ..default()
            },
            ChildOf(owner),
            crate::sand_store::SandCredits(CREDITS),
        ))
        .id();
    crate::description::button(world, bar, owner, "Attach files", Pick(false));
    crate::description::button(world, bar, owner, "Paste image", PasteImage);
    let reference = if private {
        None
    } else {
        crate::description::button(world, bar, owner, "Reference local file", Pick(true));
        let reference = world
            .spawn((
                crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
                ChildOf(bar),
            ))
            .insert(Node {
                width: px(220),
                max_width: percent(100),
                ..default()
            })
            .id();
        world
            .get_mut::<EditableText>(reference)
            .unwrap()
            .allow_newlines = false;
        world
            .entity_mut(reference)
            .insert(crate::icons::Tooltip("Resource URL or Record UID".into()));
        crate::description::button(world, bar, owner, "Add reference", Reference);
        crate::message_progress::composer(world, owner, bar);
        crate::message_questions::composer(world, owner, bar);
        Some(reference)
    };
    crate::speech::composer(world, owner, owner, input);
    let list = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.entity_mut(owner).insert(Draft {
        revision: 0,
        binding: binding.clone(),
        parts: Vec::new(),
        list,
        status,
        reference,
        input,
        locked: false,
    });
    drafts::attach(world, owner, thread, binding);
}

pub(crate) fn contents(world: &World, owner: Entity) -> Result<Vec<MessagePart>, String> {
    if crate::speech::active(world, owner) {
        return Err("Stop or cancel dictation before sending this message.".into());
    }
    if world.get::<Picking>(owner).is_some() {
        return Err("Wait for the selected files to finish loading.".into());
    }
    Ok(world
        .get::<Draft>(owner)
        .map(|draft| draft.parts.clone())
        .unwrap_or_default())
}

pub(crate) fn sent(world: &mut World, owner: Entity, success: bool) {
    if success {
        drafts::clear(world, owner);
    }
    if let Some(mut draft) = world.get_mut::<Draft>(owner) {
        draft.locked = false;
        if success {
            draft.parts.clear();
        }
    }
    render_draft(world, owner);
}

pub(super) fn render_draft(world: &mut World, owner: Entity) {
    let Some(mut draft) = world.get_mut::<Draft>(owner) else {
        return;
    };
    draft.revision = draft.revision.wrapping_add(1);
    let binding = draft.binding.clone();
    let list = draft.list;
    let parts = draft.parts.clone();
    world.entity_mut(list).despawn_children();
    for (index, part) in parts.iter().enumerate() {
        let label = match part {
            MessagePart::Question { question } => {
                format!("Question for {}: {}", question.responder, question.prompt)
            }
            MessagePart::Steps { steps } => nucleus::operation::steps_text(steps),
            MessagePart::Text { text } => text.clone(),
            MessagePart::Attachment {
                name,
                mime_type,
                data,
            } => format!(
                "{name} · {mime_type} · {} KiB · snapshot",
                data.len() * 3 / 4 / 1024
            ),
            MessagePart::Reference { name, uri } => format!("{name} · reference: {uri}"),
        };
        if matches!(part, MessagePart::Attachment { .. }) {
            attachments::draft_preview(world, list, &binding, part);
        } else {
            crate::edit_mode::label(world, list, &label, 13.0);
        }
        crate::description::button(world, list, owner, "Remove", Remove(index));
        if matches!(part, MessagePart::Attachment { mime_type, .. } if mime_type.starts_with("text/") || mime_type == "application/json")
        {
            crate::description::button(
                world,
                list,
                owner,
                "Convert this attachment to text",
                Convert(index),
            );
        }
    }
}

fn mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "json" => "application/json",
        "txt" | "md" | "rs" | "py" | "js" | "ts" | "toml" | "yaml" | "yml" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn read_file(path: PathBuf, reference: bool) -> Result<MessagePart, String> {
    let name = path
        .file_name()
        .ok_or("Choose a file.")?
        .to_string_lossy()
        .to_string();
    if reference {
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let uri: String = path
            .to_string_lossy()
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte) {
                    char::from(byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        return Ok(MessagePart::Reference {
            name,
            uri: format!("file:///{}", uri.trim_start_matches('/')),
        });
    }
    let file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Choose a regular file.".into());
    }
    let mut bytes = Vec::new();
    file.take(nucleus::message::MAX_CONTENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > nucleus::message::MAX_CONTENT_BYTES {
        return Err("Choose files totaling at most 4 MiB per message.".into());
    }
    Ok(MessagePart::Attachment {
        name,
        mime_type: mime(&path).into(),
        data: BASE64_STANDARD.encode(bytes),
    })
}

#[derive(Clone)]
struct Pick(bool);
impl Action for Pick {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<Draft>(owner) else {
            return;
        };
        if draft.locked || world.get::<Picking>(owner).is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        world
            .entity_mut(owner)
            .insert(Picking(Mutex::new(receiver)));
        let reference = self.0;
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .take(nucleus::message::MAX_PARTS + 1)
                .map(|path| read_file(path, reference))
                .collect();
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
}

#[derive(Clone)]
struct Reference;
impl Action for Reference {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<Draft>(owner) else {
            return;
        };
        if draft.locked {
            return;
        }
        let Some(input) = draft.reference else {
            return;
        };
        let text = world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string();
        let uri = if nucleus::valid_uid(text.trim(), "r") {
            format!("record:{}", text.trim())
        } else {
            text.trim().into()
        };
        append(
            world,
            owner,
            vec![MessagePart::Reference {
                name: text.trim().chars().take(512).collect(),
                uri,
            }],
        );
    }
}

#[derive(Clone)]
struct Remove(usize);
impl Action for Remove {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut draft) = world.get_mut::<Draft>(owner) else {
            return;
        };
        if !draft.locked && self.0 < draft.parts.len() {
            draft.parts.remove(self.0);
        }
        render_draft(world, owner);
    }
}

#[derive(Clone)]
struct Convert(usize);
impl Action for Convert {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<Draft>(owner) else {
            return;
        };
        if draft.locked {
            return;
        }
        let status = draft.status;
        let result = (|| {
            let Some(MessagePart::Attachment {
                name,
                mime_type,
                data,
            }) = draft.parts.get(self.0)
            else {
                return Err("Choose a text attachment.".into());
            };
            if !(mime_type.starts_with("text/") || mime_type == "application/json") {
                return Err("Only text attachments can be converted.".into());
            }
            let text = String::from_utf8(nucleus::message::decode(data)?)
                .map_err(|_| "This attachment is not UTF-8 text.")?;
            let mut parts = draft.parts.clone();
            parts[self.0] = MessagePart::Text {
                text: format!("File snapshot: {name}\n{text}"),
            };
            nucleus::message::validate(&parts)?;
            Ok::<_, String>(parts)
        })();
        match result {
            Ok(parts) => {
                world.get_mut::<Draft>(owner).unwrap().parts = parts;
                render_draft(world, owner);
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

fn append(world: &mut World, owner: Entity, parts: Vec<MessagePart>) {
    let Some(mut draft) = world.get_mut::<Draft>(owner) else {
        return;
    };
    if draft.locked {
        return;
    }
    let status = draft.status;
    let mut combined = draft.parts.clone();
    combined.extend(parts);
    match nucleus::message::validate(&combined) {
        Ok(()) => {
            draft.parts = combined;
            render_draft(world, owner);
        }
        Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
    }
}

fn poll(world: &mut World) {
    let results: Vec<_> = world
        .query::<(Entity, &Picking)>()
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
    for (owner, result) in results {
        world.entity_mut(owner).remove::<Picking>();
        match result {
            Ok(parts) => append(world, owner, parts),
            Err(error) => {
                if let Some(draft) = world.get::<Draft>(owner) {
                    let status = draft.status;
                    world.get_mut::<Text>(status).unwrap().0 = error;
                }
            }
        }
    }
}

#[derive(Clone)]
struct PasteImage;
impl Action for PasteImage {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<Draft>(owner) else {
            return;
        };
        if draft.locked || world.get::<Picking>(owner).is_some() {
            return;
        }
        let status = draft.status;
        let result = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .ok_or("Clipboard unavailable.".to_string())
            .and_then(|mut clipboard| clipboard.fetch_image().map_err(|error| error.to_string()));
        let image = match result {
            Ok(image) => image,
            Err(error) => {
                world.get_mut::<Text>(status).unwrap().0 = error;
                return;
            }
        };
        let size = image.texture_descriptor.size;
        if u64::from(size.width) * u64::from(size.height) > 16 * 1024 * 1024 {
            world.get_mut::<Text>(status).unwrap().0 =
                "The clipboard image exceeds 16 million pixels.".into();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        world
            .entity_mut(owner)
            .insert(Picking(Mutex::new(receiver)));
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        std::thread::spawn(move || {
            let result = (|| {
                let rgba = image::RgbaImage::from_raw(
                    size.width,
                    size.height,
                    image.data.ok_or("Clipboard image has no pixels.")?,
                )
                .ok_or("Clipboard image is not RGBA.")?;
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(rgba)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())?;
                let parts = vec![MessagePart::Attachment {
                    name: "pasted-image.png".into(),
                    mime_type: "image/png".into(),
                    data: BASE64_STANDARD.encode(bytes.into_inner()),
                }];
                nucleus::message::validate(&parts)?;
                Ok(parts)
            })();
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
}

fn dropped(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<bevy::window::FileDragAndDrop>>,
) {
    let files: Vec<_> = world
        .get_resource::<Messages<bevy::window::FileDragAndDrop>>()
        .map(|messages| {
            cursor
                .read(messages)
                .filter_map(|event| {
                    if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, window } = event {
                        Some((*window, path_buf.clone()))
                    } else {
                        None
                    }
                })
                .take(nucleus::message::MAX_PARTS + 1)
                .collect()
        })
        .unwrap_or_default();
    let paths: Vec<_> = files
        .into_iter()
        .filter(|(window, _)| !crate::external_drop::receives(world, *window))
        .map(|(_, path)| path)
        .collect();
    if paths.is_empty() {
        return;
    }
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let owner = world
        .query::<(Entity, &Draft)>()
        .iter(world)
        .find(|(_, draft)| Some(draft.input) == focus && !draft.locked)
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return };
    if world.get::<Picking>(owner).is_some() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    world
        .entity_mut(owner)
        .insert(Picking(Mutex::new(receiver)));
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    std::thread::spawn(move || {
        let _ = sender.send(
            paths
                .into_iter()
                .map(|path| read_file(path, false))
                .collect(),
        );
        if let Some(wake) = wake {
            wake.ring();
        }
    });
}

pub(crate) fn display(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    data: &serde_json::Value,
) {
    let Some(uid) = data["uid"].as_str() else {
        return;
    };
    crate::message_progress::progress(world, parent, &data["progress"]);
    if data["content"].is_null() {
        return;
    }
    let Ok(parts) = serde_json::from_value::<Vec<StoredPart>>(data["content"]["parts"].clone())
    else {
        crate::edit_mode::label(
            world,
            parent,
            "Message contents are unavailable or use an unsupported format.",
            13.0,
        );
        return;
    };
    for (index, part) in parts
        .into_iter()
        .take(nucleus::message::MAX_PARTS)
        .enumerate()
    {
        match part {
            StoredPart::Question { question } => {
                crate::message_questions::view(world, parent, binding, data, index, &question)
            }
            StoredPart::Steps { steps } => {
                crate::message_progress::steps(world, parent, binding, data, index, &steps)
            }
            StoredPart::Text { text } => {
                crate::edit_mode::label(world, parent, &text, 14.0);
            }
            StoredPart::Reference { name, uri } => {
                crate::edit_mode::label(world, parent, &format!("{name}\nReference: {uri}"), 13.0);
                if let Some(record) = uri.strip_prefix("record:") {
                    crate::description::button(
                        world,
                        parent,
                        parent,
                        "Open Record",
                        crate::full_record::Open(RecordBinding {
                            uid: record.into(),
                            ..binding.clone()
                        }),
                    );
                }
            }
            attachment => attachments::create(world, parent, binding, uid, index, attachment),
        }
    }
}
