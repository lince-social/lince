// non-lean-yet
// Scope: mod attachments, mod drafts, mod parts, mod presentation.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 105, 121, 218,
//   260, 262, 447, 475, 588.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 446.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (194, 389, 459, 462, 482, 507, 532, 538).

mod attachments;
mod drafts;
mod parts;
mod presentation;
mod stored;
#[cfg(test)]
mod tests;

use crate::native::{actions::Action, protein_area::RecordBinding};
use base64::{Engine, prelude::BASE64_STANDARD};
use bevy::{prelude::*, text::EditableText};
use nucleus::message::{MessagePart, StoredPart};
pub(crate) use parts::Parts;
use std::{
    io::Read,
    path::PathBuf,
    sync::{Mutex, mpsc},
};
pub(crate) use stored::ContentRef;

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../../licenses/native/base64-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "rfd",
        author: "The rfd contributors",
        license: include_str!("../../licenses/native/document/rfd-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../../licenses/native/cpal-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "hound",
        author: "Ruud van Asseldonk",
        license: include_str!("../../licenses/native/hound-Apache-2.0.txt"),
    },
];

#[derive(Component)]
pub(crate) struct Draft {
    revision: u64,
    binding: RecordBinding,
    pub parts: Parts,
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
    progress: Entity,
    content: Entity,
    binding: RecordBinding,
}

fn sections(world: &mut World, parent: Entity) -> [Entity; 2] {
    std::array::from_fn(|_| {
        world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id()
    })
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
    let [progress, content] = sections(world, parent);
    world.entity_mut(owner).insert(ContentView {
        progress,
        content,
        binding: binding.clone(),
    });
    refresh(world, owner, data);
}

pub(crate) fn refresh(world: &mut World, owner: Entity, data: &serde_json::Value) {
    if data["uid"].as_str().is_none() {
        return;
    }
    let Some(view) = world.get::<ContentView>(owner) else {
        return;
    };
    let content_changed = !stored::matches(world, view.content, &view.binding, data);
    let (content, progress, binding) = (
        view.content,
        view.progress,
        content_changed.then(|| view.binding.clone()),
    );
    crate::native::message_progress::progress(world, progress, &data["progress"]);
    if let Some(binding) = binding {
        stored::refresh(world, content, &binding, data);
    }
}

pub struct Plugin;
impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            crate::native::message_commands::keyboard.before(bevy::text::EditableTextSystems),
        );
        app.add_systems(
            Update,
            (
                crate::native::speech::update.after(crate::native::cell_bridge::ReceiveCell),
                drafts::update,
                poll,
                dropped,
                crate::native::message_commands::refresh,
                crate::native::directory_list::poll,
                attachments::receive.after(crate::native::cell_bridge::ReceiveCell),
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
            crate::native::sand_store::SandCredits(CREDITS),
        ))
        .id();
    crate::native::description::button(world, bar, owner, "Attach files", Pick(false));
    crate::native::description::button(world, bar, owner, "Paste image", PasteImage);
    let reference = if private {
        None
    } else {
        crate::native::description::button(world, bar, owner, "Reference local file", Pick(true));
        let reference = world
            .spawn((
                crate::native::sand::text_editor(
                    "",
                    world.resource::<crate::native::theme::Typography>(),
                    0,
                ),
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
            .insert(crate::native::icons::Tooltip(
                "Resource URL or Record UID".into(),
            ));
        crate::native::description::button(world, bar, owner, "Add reference", Reference);
        crate::native::message_progress::composer(world, owner, bar);
        crate::native::message_questions::composer(world, owner, bar);
        Some(reference)
    };
    crate::native::speech::composer(world, owner, owner, input);
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
        parts: Parts::default(),
        list,
        status,
        reference,
        input,
        locked: false,
    });
    drafts::attach(world, owner, thread, binding);
}

pub(crate) fn contents(world: &World, owner: Entity) -> Result<Vec<MessagePart>, String> {
    if crate::native::speech::active(world, owner) {
        return Err("Stop or cancel dictation before sending this message.".into());
    }
    if world.get::<Picking>(owner).is_some() {
        return Err("Wait for the selected files to finish loading.".into());
    }
    Ok(world
        .get::<Draft>(owner)
        .map(|draft| draft.parts.to_vec())
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
    presentation::render(world, list, owner, &binding, &parts);
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
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
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
            parts.replace(
                self.0,
                MessagePart::Text {
                    text: format!("File snapshot: {name}\n{text}"),
                },
            );
            parts.validate()?;
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
    match combined.validate() {
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
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
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
        .filter(|(window, _)| !crate::native::external_drop::receives(world, *window))
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
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
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
