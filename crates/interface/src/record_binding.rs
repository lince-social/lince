use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use bevy::{
    prelude::*,
    text::{EditableText, FontCx, LayoutCx},
};
use cell::{ClientMessage, ServerMessage};
use loro::{ExportMode, LoroDoc, UndoManager, VersionVector, cursor::Cursor};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::protein_area::{RecordBinding, Source};

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (
        World,
        std::sync::Arc<engine::Engine>,
        String,
        Entity,
        Entity,
    ) {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Title".into(),
                    body: "Olá 👩‍💻 עולם".into(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let runtime = cell::CellRuntime {
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
        };
        let mut world = World::new();
        world.insert_resource(crate::app::CellHandle(runtime.clone()));
        world.init_resource::<Bindings>();
        world.init_resource::<bevy::input_focus::InputFocus>();
        world.insert_non_send(crate::cell_bridge::connect(
            runtime,
            crate::wake::WakeSignal::new(|| {}),
        ));
        let first = world.spawn(EditableText::new("Olá 👩‍💻 עולם")).id();
        let second = world.spawn(EditableText::new("Olá 👩‍💻 עולם")).id();
        let binding = RecordBinding {
            area: first,
            uid: uid.clone(),
            source: Source::Local,
        };
        attach(&mut world, first, binding.clone(), "body", None);
        attach(&mut world, second, binding, "body", None);
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.joined)
                && !world.resource::<Bindings>().documents.is_empty()
        })
        .await;
        (world, engine, uid, first, second)
    }

    async fn pump(world: &mut World, done: impl Fn(&World) -> bool) {
        for _ in 0..500 {
            let mut messages = Vec::new();
            {
                let mut bridge = world.non_send_mut::<crate::cell_bridge::CellBridge>();
                while let Ok(message) = bridge.incoming.try_recv() {
                    messages.push(message);
                }
            }
            for message in messages {
                receive(world, Source::Local, message);
            }
            update(world);
            if done(world) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("Record bindings did not settle");
    }

    #[tokio::test]
    async fn two_views_share_unicode_edits_and_persist_before_transmission() {
        let (mut world, engine, uid, first, second) = fixture().await;
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Olá 👩‍💻 שלום עולם!");
        update(&mut world);
        assert_eq!(
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 שלום עולם!"
        );
        let document = world
            .resource::<Bindings>()
            .documents
            .values()
            .next()
            .unwrap();
        assert!(document.inflight.is_none());
        assert_eq!(document.pending.len(), 1);
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Olá 👩‍💻 שלום עולם!");
        assert!(
            engine
                .load_record_edit_draft(&serde_json::to_string(&Source::Local).unwrap(), &uid)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn composition_defers_remote_projection_and_merges_after_commit() {
        let (mut world, engine, uid, first, second) = fixture().await;
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .pending_edits
            .push(bevy::text::TextEdit::Insert("!".into()));
        engine
            .act(
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some("REMOTE Olá 👩‍💻 עולם".into()),
                },
                None,
            )
            .await
            .unwrap();
        for _ in 0..10 {
            let message = {
                world
                    .non_send_mut::<crate::cell_bridge::CellBridge>()
                    .incoming
                    .try_recv()
                    .ok()
            };
            if let Some(message) = message {
                receive(&mut world, Source::Local, message);
            }
            update(&mut world);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(
            world
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 עולם"
        );
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .pending_edits
            .clear();
        world
            .get_mut::<EditableText>(first)
            .unwrap()
            .editor
            .set_text("Olá 👩‍💻 עולם!");
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| document.pending.is_empty() && !document.saving)
        })
        .await;
        assert_eq!(
            world
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "REMOTE Olá 👩‍💻 עולם!"
        );
    }

    #[test]
    fn undo_keeps_another_peers_text_and_cursors_follow_unicode() {
        let mut document = Document::new(Source::Local, None);
        let remote = LoroDoc::new();
        remote.get_text("body").insert(0, "Olá 👩‍💻").unwrap();
        remote.commit();
        document
            .doc
            .import(&remote.export(ExportMode::Snapshot).unwrap())
            .unwrap();
        let anchor = document
            .doc
            .get_text("body")
            .get_cursor(4, Default::default())
            .unwrap();
        splice(&document.doc, "body", "Olá 👩‍💻", "Olá 👩‍💻!").unwrap();
        remote.get_text("body").insert(0, "X ").unwrap();
        remote.commit();
        document
            .doc
            .import(&remote.export(ExportMode::Snapshot).unwrap())
            .unwrap();
        assert_eq!(document.doc.get_cursor_pos(&anchor).unwrap().current.pos, 6);
        document.undo.undo().unwrap();
        assert_eq!(document.doc.get_text("body").to_string(), "X Olá 👩‍💻");
    }

    #[tokio::test]
    async fn reopening_restores_and_delivers_a_saved_unsent_edit() {
        let (mut world, engine, uid, first, _) = fixture().await;
        let binding = world.get::<TextBinding>(first).unwrap().record.clone();
        let key = key(&binding);
        let draft = {
            let mut bindings = world.resource_mut::<Bindings>();
            let document = bindings.documents.get_mut(&key).unwrap();
            let before = document.doc.oplog_vv();
            splice(&document.doc, "body", "Olá 👩‍💻 עולם", "Recovered 👩‍💻").unwrap();
            document.queue(&uid, &before).unwrap();
            serde_json::to_string(&Draft {
                snapshot: B64.encode(document.doc.export(ExportMode::Snapshot).unwrap()),
                pending: document.pending.clone(),
            })
            .unwrap()
        };
        engine
            .save_record_edit_draft(&storage_source(&key, None), &uid, &draft)
            .await
            .unwrap();
        world.insert_resource(Bindings::default());
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .get(&key)
                .is_some_and(|document| {
                    document.joined && document.pending.is_empty() && !document.saving
                })
                && world
                    .get::<EditableText>(first)
                    .unwrap()
                    .value()
                    .to_string()
                    == "Recovered 👩‍💻"
        })
        .await;
        assert_eq!(engine.doc_text(&uid).await.unwrap().1, "Recovered 👩‍💻");
        assert!(
            engine
                .load_record_edit_draft(&storage_source(&key, None), &uid)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn scalar_only_bindings_save_without_joining_the_text_document() {
        let (mut world, _, uid, first, second) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        world.entity_mut(first).remove::<TextBinding>();
        world.entity_mut(second).remove::<TextBinding>();
        world.insert_resource(Bindings::default());
        submit(
            &mut world,
            &record,
            engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: uid,
                mutation: engine::record_change::Mutation::Quantity {
                    value: "3.75".into(),
                },
            },
        )
        .unwrap();
        pump(&mut world, |world| {
            world
                .resource::<Bindings>()
                .documents
                .get(&key(&record))
                .is_some_and(|document| {
                    document.loaded && document.pending.is_empty() && !document.saving
                })
        })
        .await;
        let document = &world.resource::<Bindings>().documents[&key(&record)];
        assert!(!document.joined);
        assert!(document.joining.is_none());
        assert_eq!(document.status, "Saved");
    }

    #[tokio::test]
    async fn a_refused_scalar_can_be_corrected_without_reopening_the_record() {
        let (mut world, _, uid, first, _) = fixture().await;
        let record = world.get::<TextBinding>(first).unwrap().record.clone();
        let request = |value: &str| engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: uid.clone(),
            mutation: engine::record_change::Mutation::Quantity {
                value: value.into(),
            },
        };
        submit(&mut world, &record, request("incomplete")).unwrap();
        pump(&mut world, |world| {
            world.resource::<Bindings>().documents[&key(&record)].failed
        })
        .await;
        assert_eq!(
            world.resource::<Bindings>().documents[&key(&record)]
                .pending
                .len(),
            1
        );
        submit(&mut world, &record, request("3.75")).unwrap();
        pump(&mut world, |world| {
            let document = &world.resource::<Bindings>().documents[&key(&record)];
            document.pending.is_empty() && !document.saving
        })
        .await;
        assert!(!world.resource::<Bindings>().documents[&key(&record)].failed);
        assert_eq!(
            world
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Olá 👩‍💻 עולם"
        );
    }
}

type Key = (String, String);

#[derive(Component, Clone)]
pub struct TextBinding {
    pub record: RecordBinding,
    pub property: String,
    pub status: Option<Entity>,
    observed: String,
}

#[derive(Component)]
pub(crate) struct BindingStatus;

pub(crate) fn enabled(world: &World) -> bool {
    world.contains_resource::<Bindings>()
}

pub(crate) fn active(world: &World, entity: Entity) -> bool {
    world.get::<TextBinding>(entity).is_some_and(|binding| {
        world
            .get_resource::<Bindings>()
            .is_some_and(|bindings| bindings.documents.contains_key(&key(&binding.record)))
    })
}

fn visible(world: &World, entity: Entity) -> bool {
    if world
        .get::<InheritedVisibility>(entity)
        .is_some_and(|visibility| !visibility.get())
    {
        return false;
    }
    let Some(node) = world.get::<ComputedNode>(entity) else {
        return true;
    };
    if node.size.x <= 0.0 || node.size.y <= 0.0 {
        return false;
    }
    let Some(target) = world.get::<ComputedUiRenderTargetInfo>(entity) else {
        return false;
    };
    let Some(transform) = world.get::<UiGlobalTransform>(entity) else {
        return false;
    };
    let center = transform.affine().translation;
    let bounds = Rect::from_center_size(center, node.size);
    bounds.max.x >= 0.0
        && bounds.max.y >= 0.0
        && bounds.min.x <= target.physical_size().x as f32
        && bounds.min.y <= target.physical_size().y as f32
}

pub(crate) fn status(world: &World, record: &RecordBinding) -> Option<String> {
    world
        .get_resource::<Bindings>()?
        .documents
        .get(&key(record))
        .map(|document| document.status.clone())
}

#[derive(Serialize, Deserialize)]
struct Draft {
    snapshot: String,
    pending: VecDeque<engine::record_change::Request>,
}

struct Document {
    doc: LoroDoc,
    undo: UndoManager,
    source: Source,
    principal: Option<String>,
    sender: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
    pending: VecDeque<engine::record_change::Request>,
    inflight: Option<(String, Instant)>,
    loaded: bool,
    joined: bool,
    joining: Option<Instant>,
    writable: Vec<String>,
    revision: u64,
    saved: u64,
    saving: bool,
    status: String,
    failed: bool,
    incoming: VecDeque<ServerMessage>,
    cursors: Vec<cell::CollabCursor>,
    presence_at: Option<Instant>,
    presence_value: Option<(String, String, String)>,
    touched: Instant,
}

impl Document {
    fn new(source: Source, principal: Option<String>) -> Self {
        let doc = LoroDoc::new();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(500);
        Self {
            doc,
            undo,
            source,
            principal,
            sender: None,
            pending: VecDeque::new(),
            inflight: None,
            loaded: false,
            joined: false,
            joining: None,
            writable: Vec::new(),
            revision: 0,
            saved: 0,
            saving: false,
            status: "Opening Record…".into(),
            failed: false,
            incoming: VecDeque::new(),
            cursors: Vec::new(),
            presence_at: None,
            presence_value: None,
            touched: Instant::now(),
        }
    }

    fn queue(&mut self, uid: &str, before: &VersionVector) -> Result<(), String> {
        if before == &self.doc.oplog_vv() {
            return Ok(());
        }
        let delta = self
            .doc
            .export_json_updates_without_peer_compression(before, &self.doc.oplog_vv());
        let raw = serde_json::to_vec(&delta).map_err(|error| error.to_string())?;
        if raw.len() > engine::collab::limits().delta_bytes {
            return Err("Edit exceeds the change size limit".into());
        }
        self.pending.push_back(engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: uid.into(),
            mutation: engine::record_change::Mutation::Text {
                update_base64: B64.encode(raw),
            },
        });
        self.revision += 1;
        self.status = "Saving locally…".into();
        Ok(())
    }
}

enum StorageEvent {
    Loaded(Key, Result<Option<String>, String>),
    Saved(Key, u64, Result<(), String>),
}

#[derive(Resource)]
struct Bindings {
    documents: HashMap<Key, Document>,
    sender: tokio::sync::mpsc::UnboundedSender<StorageEvent>,
    incoming: tokio::sync::mpsc::UnboundedReceiver<StorageEvent>,
    wake_at: Option<Instant>,
    principals: HashMap<String, String>,
}

impl Default for Bindings {
    fn default() -> Self {
        let (sender, incoming) = tokio::sync::mpsc::unbounded_channel();
        Self {
            documents: HashMap::new(),
            sender,
            incoming,
            wake_at: None,
            principals: HashMap::new(),
        }
    }
}

pub struct RecordBindingPlugin;

impl Plugin for RecordBindingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bindings>()
            .add_systems(PostUpdate, gate.before(bevy::text::EditableTextSystems))
            .add_systems(
                Update,
                local_messages
                    .after(crate::cell_bridge::ReceiveCell)
                    .run_if(crate::laboratory::normal),
            )
            .add_systems(
                PostUpdate,
                update
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions)
                    .run_if(crate::laboratory::normal),
            )
            .add_systems(
                PostUpdate,
                paint_cursors.after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

fn gate(bindings: Res<Bindings>, mut fields: Query<(&TextBinding, &mut EditableText)>) {
    for (binding, mut text) in &mut fields {
        let writable = bindings
            .documents
            .get(&key(&binding.record))
            .is_some_and(|document| {
                document.joined && document.writable.contains(&binding.property) && !document.failed
            });
        if !writable {
            text.pending_paste = None;
            text.pending_edits.retain(|edit| {
                !matches!(
                    edit,
                    bevy::text::TextEdit::Cut
                        | bevy::text::TextEdit::Paste
                        | bevy::text::TextEdit::Insert(_)
                        | bevy::text::TextEdit::Backspace
                        | bevy::text::TextEdit::BackspaceWord
                        | bevy::text::TextEdit::Delete
                        | bevy::text::TextEdit::DeleteWord
                        | bevy::text::TextEdit::ImeSetCompose { .. }
                        | bevy::text::TextEdit::ImeCommit { .. }
                )
            });
        }
    }
}

fn key(record: &RecordBinding) -> Key {
    (
        serde_json::to_string(&record.source).expect("Record source"),
        record.uid.clone(),
    )
}

fn storage_source(key: &Key, principal: Option<&str>) -> String {
    principal.map_or_else(
        || key.0.clone(),
        |principal| format!("{}\n{principal}", key.0),
    )
}

pub fn attach(
    world: &mut World,
    entity: Entity,
    record: RecordBinding,
    property: &str,
    status: Option<Entity>,
) {
    if let Some(status) = status {
        world.entity_mut(status).insert(BindingStatus);
    }
    let observed = world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string())
        .unwrap_or_default();
    world.entity_mut(entity).insert((
        TextBinding {
            record,
            property: property.into(),
            status,
            observed,
        },
        crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
    ));
}

pub fn receive(world: &mut World, source: Source, message: ServerMessage) {
    let Some(mut bindings) = world.get_resource_mut::<Bindings>() else {
        return;
    };
    let source_key = serde_json::to_string(&source).expect("Record source");
    if let ServerMessage::SessionAuthenticated { person, .. } = &message {
        bindings
            .principals
            .insert(source_key.clone(), person.clone());
        for ((origin, _), document) in &mut bindings.documents {
            if origin == &source_key && document.principal.as_deref() != Some(person) {
                fail(
                    document,
                    "The login changed. Pending edits belong to the previous login",
                );
                document.sender = None;
                document.joined = false;
            }
        }
        return;
    }
    for ((origin, uid), document) in &mut bindings.documents {
        if origin != &source_key {
            continue;
        }
        if matches!(&message, ServerMessage::Error { id, .. } if id == "connection") {
            document.sender = None;
            document.joined = false;
            document.joining = None;
            document.inflight = None;
            document.status = "Saved locally · waiting for connection…".into();
            continue;
        }
        let applies = match &message {
            ServerMessage::CollabState { record_uid, .. }
            | ServerMessage::CollabChange { record_uid, .. }
            | ServerMessage::CollabCursors { record_uid, .. } => record_uid == uid,
            ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => {
                document
                    .inflight
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == id)
                    || id == &format!("binding-join-{uid}")
                    || id == crate::cell_bridge::CONNECTION
            }
            _ => false,
        };
        if applies {
            document.incoming.push_back(message.clone());
        }
    }
}

fn local_messages(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let messages: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        receive(world, Source::Local, message);
    }
}

fn fail(document: &mut Document, error: impl std::fmt::Display) {
    document.failed = true;
    document.status = format!("Not saved: {error}. Your draft is kept.");
}

fn load_draft(document: &mut Document, raw: Option<String>) -> Result<(), String> {
    if let Some(raw) = raw {
        let draft: Draft = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let bytes = B64
            .decode(draft.snapshot)
            .map_err(|error| error.to_string())?;
        document
            .doc
            .import(&bytes)
            .map_err(|error| error.to_string())?;
        let entered = std::mem::take(&mut document.pending);
        document.pending = draft.pending;
        document.pending.extend(entered);
    }
    document.loaded = true;
    Ok(())
}

pub(crate) fn submit(
    world: &mut World,
    record: &RecordBinding,
    request: engine::record_change::Request,
) -> Result<(), String> {
    if crate::laboratory::suspended(world, record.area) {
        return Err("Workspace is suspended".into());
    }
    let engine = world
        .get_resource::<crate::app::CellHandle>()
        .ok_or("Local storage is unavailable")?
        .0
        .engine
        .clone();
    let channel = crate::protein_area::editor_sender(world, record);
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let mut bindings = world
        .get_resource_mut::<Bindings>()
        .ok_or("Record bindings are unavailable")?;
    let key = key(record);
    if !bindings.documents.contains_key(&key) {
        if bindings.documents.len() >= 64 {
            return Err("Wait for pending editors to finish".into());
        }
        let principal = bindings.principals.get(&key.0).cloned();
        let storage = storage_source(&key, principal.as_deref());
        bindings
            .documents
            .insert(key.clone(), Document::new(record.source.clone(), principal));
        let sender = bindings.sender.clone();
        let key = key.clone();
        tokio::spawn(async move {
            let result = engine
                .load_record_edit_draft(&storage, &key.1)
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(StorageEvent::Loaded(key, result));
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
    let principal = bindings.principals.get(&key.0).cloned();
    let document = bindings.documents.get_mut(&key).unwrap();
    if document.failed {
        let correcting = document.principal == principal
            && document.pending.front().is_some_and(|previous| {
                document
                    .inflight
                    .as_ref()
                    .is_some_and(|(id, _)| id == &previous.id)
                    && match (&previous.mutation, &request.mutation) {
                        (engine::record_change::Mutation::Text { .. }, _) => false,
                        (
                            engine::record_change::Mutation::Work { field: before, .. },
                            engine::record_change::Mutation::Work { field: after, .. },
                        ) => before.key() == after.key(),
                        (before, after) => {
                            std::mem::discriminant(before) == std::mem::discriminant(after)
                        }
                    }
            });
        if !correcting {
            return Err(document.status.clone());
        }
        document.pending.pop_front();
        document.inflight = None;
        document.failed = false;
    }
    if document.pending.len() >= 512 {
        return Err("Wait for pending edits to sync".into());
    }
    document.sender = channel;
    document.pending.push_back(request);
    document.revision += 1;
    document.status = "Saving locally…".into();
    Ok(())
}

fn splice(doc: &LoroDoc, property: &str, before: &str, after: &str) -> Result<(), String> {
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let text = doc.get_text(property);
    let removed = old.len() - prefix - suffix;
    if removed != 0 {
        text.delete(prefix, removed)
            .map_err(|error| error.to_string())?;
    }
    let inserted: String = new[prefix..new.len() - suffix].iter().collect();
    if !inserted.is_empty() {
        text.insert(prefix, &inserted)
            .map_err(|error| error.to_string())?;
    }
    doc.commit();
    Ok(())
}

fn selection(
    document: &Document,
    binding: &TextBinding,
    text: &EditableText,
) -> (Option<Cursor>, Option<Cursor>) {
    let value = text.value().to_string();
    let convert = |index: usize| {
        value
            .char_indices()
            .take_while(|(byte, _)| *byte < index)
            .count()
    };
    let selected = text.editor.raw_selection();
    let field = document.doc.get_text(binding.property.as_str());
    (
        field.get_cursor(convert(selected.anchor().index()), Default::default()),
        field.get_cursor(convert(selected.focus().index()), Default::default()),
    )
}

fn set_text(world: &mut World, entity: Entity, value: &str, positions: Option<(usize, usize)>) {
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
        return;
    }
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            let mut text = world.get_mut::<EditableText>(entity).unwrap();
            text.editor.set_text(value);
            if let Some((anchor, focus)) = positions {
                let byte = |position: usize| {
                    value
                        .char_indices()
                        .nth(position)
                        .map_or(value.len(), |(index, _)| index)
                };
                text.editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .select_byte_range(byte(anchor), byte(focus));
            }
        });
    });
}

fn update(world: &mut World) {
    let Some(engine) = world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.engine.clone())
    else {
        return;
    };
    let Some(mut bindings) = world.remove_resource::<Bindings>() else {
        return;
    };
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let mut views: Vec<_> = world
        .query::<(Entity, &TextBinding)>()
        .iter(world)
        .map(|(entity, binding)| (entity, binding.clone()))
        .collect();
    views.sort_by_key(|(entity, _)| focus != Some(*entity));
    for (entity, binding) in &views {
        let key = key(&binding.record);
        if focus != Some(*entity) && !visible(world, *entity) {
            continue;
        }
        if !bindings.documents.contains_key(&key) {
            if bindings.documents.len() >= 64 {
                if focus != Some(*entity) {
                    continue;
                }
                let oldest = bindings
                    .documents
                    .iter()
                    .filter(|(_, document)| document.pending.is_empty() && !document.saving)
                    .min_by_key(|(_, document)| document.touched)
                    .map(|(key, _)| key.clone());
                let Some(oldest) = oldest else {
                    continue;
                };
                if let Some(document) = bindings.documents.remove(&oldest) {
                    if let Some(sender) = document.sender {
                        let _ = sender.try_send(ClientMessage::CollabLeave {
                            record_uid: oldest.1,
                        });
                    }
                }
            }
            let principal = bindings.principals.get(&key.0).cloned();
            let storage = storage_source(&key, principal.as_deref());
            bindings.documents.insert(
                key.clone(),
                Document::new(binding.record.source.clone(), principal),
            );
            let engine = engine.clone();
            let sender = bindings.sender.clone();
            let wake = wake.clone();
            let key = key.clone();
            tokio::spawn(async move {
                let result = engine
                    .load_record_edit_draft(&storage, &key.1)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(StorageEvent::Loaded(key, result));
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
        }
        let document = bindings.documents.get_mut(&key).unwrap();
        document.touched = Instant::now();
        if document
            .sender
            .as_ref()
            .is_none_or(|sender| sender.is_closed())
        {
            document.sender = crate::protein_area::editor_sender(world, &binding.record);
            document.joined = false;
            document.joining = None;
        }
    }
    while let Ok(event) = bindings.incoming.try_recv() {
        match event {
            StorageEvent::Loaded(key, result) => {
                if let Some(document) = bindings.documents.get_mut(&key) {
                    match result.and_then(|raw| load_draft(document, raw)) {
                        Ok(()) => {}
                        Err(error) => fail(document, error),
                    }
                }
            }
            StorageEvent::Saved(key, revision, result) => {
                if let Some(document) = bindings.documents.get_mut(&key) {
                    document.saving = false;
                    match result {
                        Ok(()) => document.saved = revision,
                        Err(error) => fail(document, error),
                    }
                }
            }
        }
    }
    let undo = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(|keys| {
            let modifier = keys.pressed(KeyCode::ControlLeft)
                || keys.pressed(KeyCode::ControlRight)
                || keys.pressed(KeyCode::SuperLeft)
                || keys.pressed(KeyCode::SuperRight);
            let redo = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            (modifier && keys.just_pressed(KeyCode::KeyZ), redo)
        })
        .unwrap_or_default();
    for (key, document) in &mut bindings.documents {
        if document
            .sender
            .as_ref()
            .is_none_or(|sender| sender.is_closed())
        {
            document.sender = crate::protein_area::editor_sender(
                world,
                &RecordBinding {
                    area: Entity::PLACEHOLDER,
                    uid: key.1.clone(),
                    source: document.source.clone(),
                },
            );
            document.joined = false;
            document.joining = None;
        }
        let members: Vec<_> = views
            .iter()
            .filter(|(_, binding)| key == &self::key(&binding.record))
            .collect();
        let needs_text = !members.is_empty()
            || document.pending.iter().any(|request| {
                matches!(
                    request.mutation,
                    engine::record_change::Mutation::Text { .. }
                )
            });
        let composing = members.iter().any(|(entity, _)| {
            world
                .get::<EditableText>(*entity)
                .is_some_and(|text| text.is_composing() || crate::record_view::pending_text(text))
        });
        let mut anchors = HashMap::new();
        if document.joined && !composing {
            for (entity, binding) in &members {
                let Some(text) = world.get::<EditableText>(*entity) else {
                    continue;
                };
                let value = text.value().to_string();
                if value != binding.observed
                    && !document.failed
                    && document.writable.contains(&binding.property)
                {
                    let before = document.doc.oplog_vv();
                    if let Err(error) =
                        splice(&document.doc, &binding.property, &binding.observed, &value)
                            .and_then(|()| document.queue(&key.1, &before))
                    {
                        fail(document, error);
                    }
                    world.get_mut::<TextBinding>(*entity).unwrap().observed = value;
                }
                let text = world.get::<EditableText>(*entity).unwrap();
                anchors.insert(*entity, selection(document, binding, text));
            }
            if undo.0
                && members.iter().any(|(entity, _)| focus == Some(*entity))
                && !document.failed
            {
                let before = document.doc.oplog_vv();
                let result = if undo.1 {
                    document.undo.redo()
                } else {
                    document.undo.undo()
                };
                if let Err(error) = result
                    .map_err(|error| error.to_string())
                    .and_then(|_| document.queue(&key.1, &before))
                {
                    fail(document, error);
                }
            }
        }
        if document.loaded && !composing {
            while let Some(message) = document.incoming.pop_front() {
                match message {
                    ServerMessage::CollabState {
                        snapshot_base64,
                        writable,
                        ..
                    } => {
                        match B64
                            .decode(snapshot_base64)
                            .map_err(|error| error.to_string())
                            .and_then(|bytes| {
                                document
                                    .doc
                                    .import(&bytes)
                                    .map(|_| ())
                                    .map_err(|error| error.to_string())
                            }) {
                            Ok(()) => {
                                document.joined = true;
                                document.joining = None;
                                document.writable = writable;
                                document.status = if document.pending.is_empty() {
                                    "Saved".into()
                                } else {
                                    "Waiting to sync…".into()
                                };
                            }
                            Err(error) => fail(document, error),
                        }
                    }
                    ServerMessage::CollabChange { update_base64, .. } => {
                        if let Err(error) = B64
                            .decode(update_base64)
                            .map_err(|error| error.to_string())
                            .and_then(|bytes| {
                                document
                                    .doc
                                    .import(&bytes)
                                    .map(|_| ())
                                    .map_err(|error| error.to_string())
                            })
                        {
                            fail(document, error);
                        }
                    }
                    ServerMessage::CollabCursors { cursors, .. } => document.cursors = cursors,
                    ServerMessage::ActionOk { id, .. } => {
                        if document
                            .pending
                            .front()
                            .is_some_and(|request| request.id == id)
                        {
                            document.pending.pop_front();
                            document.inflight = None;
                            document.revision += 1;
                            document.status = if document.pending.is_empty() {
                                "Saved".into()
                            } else {
                                "Waiting to sync…".into()
                            };
                        }
                    }
                    ServerMessage::Error { id, message, .. }
                        if id == crate::cell_bridge::CONNECTION =>
                    {
                        document.joined = false;
                        document.joining = None;
                        document.inflight = None;
                        document.status = format!("Disconnected: {message}. Your draft is kept.");
                    }
                    ServerMessage::Error { message, .. } => fail(document, message),
                    _ => {}
                }
            }
        }
        if needs_text
            && document.loaded
            && !document.joined
            && !document.failed
            && document
                .joining
                .is_none_or(|at| at.elapsed().as_secs() >= 5)
        {
            if document.sender.as_ref().is_some_and(|sender| {
                sender
                    .try_send(ClientMessage::CollabJoin {
                        id: format!("binding-join-{}", key.1),
                        record_uid: key.1.clone(),
                    })
                    .is_ok()
            }) {
                document.joining = Some(Instant::now());
            }
        }
        if document.loaded && document.revision > document.saved && !document.saving {
            let draft = document
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    serde_json::to_string(&Draft {
                        snapshot: B64.encode(bytes),
                        pending: document.pending.clone(),
                    })
                    .map_err(|error| error.to_string())
                });
            match draft {
                Ok(draft) => {
                    document.saving = true;
                    let sender = bindings.sender.clone();
                    let engine = engine.clone();
                    let key = key.clone();
                    let storage = storage_source(&key, document.principal.as_deref());
                    let empty = document.pending.is_empty();
                    let revision = document.revision;
                    let wake = wake.clone();
                    tokio::spawn(async move {
                        let result = if empty {
                            engine.clear_record_edit_draft(&storage, &key.1).await
                        } else {
                            engine
                                .save_record_edit_draft(&storage, &key.1, &draft)
                                .await
                        }
                        .map_err(|error| error.to_string());
                        let _ = sender.send(StorageEvent::Saved(key, revision, result));
                        if let Some(wake) = wake {
                            wake.ring();
                        }
                    });
                }
                Err(error) => fail(document, error),
            }
        }
        if (document.joined || !needs_text)
            && !document.failed
            && document.saved == document.revision
            && document
                .inflight
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed().as_secs() >= 5)
        {
            if let Some(request) = document.pending.front() {
                if document.sender.as_ref().is_some_and(|sender| {
                    sender
                        .try_send(ClientMessage::Act {
                            id: request.id.clone(),
                            action: engine::actions::Action::ChangeRecord {
                                request: request.clone(),
                            },
                        })
                        .is_ok()
                }) {
                    document.inflight = Some((request.id.clone(), Instant::now()));
                    document.status = match document.source {
                        Source::Local => "Saved locally · syncing…".into(),
                        Source::Organ(_) => "Waiting for origin…".into(),
                    };
                }
            }
        }
        for (entity, binding) in &members {
            if document.joined && !composing && !document.failed {
                let value = document.doc.get_text(binding.property.as_str()).to_string();
                if world
                    .get::<EditableText>(*entity)
                    .is_some_and(|text| text.value().to_string() != value)
                {
                    let positions = anchors.get(entity).and_then(|(anchor, focus)| {
                        Some((
                            document
                                .doc
                                .get_cursor_pos(anchor.as_ref()?)
                                .ok()?
                                .current
                                .pos,
                            document
                                .doc
                                .get_cursor_pos(focus.as_ref()?)
                                .ok()?
                                .current
                                .pos,
                        ))
                    });
                    set_text(world, *entity, &value, positions);
                }
                if world.get::<TextBinding>(*entity).unwrap().observed != value {
                    world.get_mut::<TextBinding>(*entity).unwrap().observed = value;
                }
            }
            if let Some(status) = binding.status {
                if world
                    .get::<Text>(status)
                    .is_some_and(|label| label.0 != document.status)
                {
                    world.get_mut::<Text>(status).unwrap().0 = document.status.clone();
                }
            }
            if document.joined && document.pending.is_empty() && binding.property == "head" {
                if let Some(mut record) = world.get_mut::<crate::record_view::RecordEditor>(*entity)
                {
                    let confirmed = document.doc.get_text("head").to_string();
                    if record.confirmed != confirmed {
                        record.confirmed = confirmed;
                    }
                }
            }
            if focus == Some(*entity)
                && document.joined
                && !composing
                && document
                    .presence_at
                    .is_none_or(|at| at.elapsed() >= Duration::from_millis(150))
            {
                let selected = selection(
                    document,
                    binding,
                    world.get::<EditableText>(*entity).unwrap(),
                );
                if let (Some(anchor), Some(focus)) = selected {
                    let payload = (
                        binding.property.clone(),
                        B64.encode(anchor.encode()),
                        B64.encode(focus.encode()),
                    );
                    let changed = document.presence_value.as_ref() != Some(&payload)
                        || document
                            .presence_at
                            .is_none_or(|at| at.elapsed().as_secs() >= 5);
                    if changed
                        && document.sender.as_ref().is_some_and(|sender| {
                            sender
                                .try_send(ClientMessage::CollabPresence {
                                    record_uid: key.1.clone(),
                                    property: payload.0.clone(),
                                    anchor: payload.1.clone(),
                                    focus: payload.2.clone(),
                                })
                                .is_ok()
                        })
                    {
                        document.presence_at = Some(Instant::now());
                        document.presence_value = Some(payload);
                    }
                }
            }
        }
    }
    bindings.documents.retain(|key, document| {
        let keep = document.touched.elapsed().as_secs() < 30
            || !document.pending.is_empty()
            || document.saving;
        if !keep {
            if let Some(sender) = &document.sender {
                let _ = sender.try_send(ClientMessage::CollabLeave {
                    record_uid: key.1.clone(),
                });
            }
        }
        keep
    });
    let active = bindings
        .documents
        .values()
        .any(|document| !document.pending.is_empty() || document.joining.is_some())
        || focus.is_some_and(|entity| world.get::<TextBinding>(entity).is_some());
    if active
        && bindings
            .wake_at
            .is_none_or(|at| at.elapsed() >= Duration::from_millis(150))
    {
        if let Some(wake) = wake {
            bindings.wake_at = Some(Instant::now());
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                wake.ring();
            });
        }
    }
    world.insert_resource(bindings);
}

#[derive(Component)]
struct RemoteCursor(String);

fn paint_cursors(world: &mut World) {
    let mut old: HashMap<_, _> = world
        .query::<(Entity, &RemoteCursor)>()
        .iter(world)
        .map(|(entity, cursor)| (cursor.0.clone(), entity))
        .collect();
    let Some(bindings) = world.remove_resource::<Bindings>() else {
        return;
    };
    let mut cursors = Vec::new();
    for (entity, binding, text) in world
        .query::<(Entity, &TextBinding, &EditableText)>()
        .iter(world)
    {
        let Some(document) = bindings.documents.get(&key(&binding.record)) else {
            continue;
        };
        for cursor in &document.cursors {
            if cursor.property != binding.property {
                continue;
            }
            let position = B64
                .decode(&cursor.focus)
                .ok()
                .and_then(|bytes| Cursor::decode(&bytes).ok())
                .and_then(|cursor| document.doc.get_cursor_pos(&cursor).ok())
                .map(|position| position.current.pos);
            if let Some(position) = position {
                let value = text.value().to_string();
                let byte = value
                    .char_indices()
                    .nth(position)
                    .map_or(value.len(), |(index, _)| index);
                cursors.push((
                    entity,
                    byte,
                    cursor
                        .person
                        .clone()
                        .unwrap_or_else(|| "Local editor".into()),
                    cursor.session.clone(),
                ));
            }
        }
    }
    for (entity, byte, name, session) in cursors {
        let Some(mut fonts) = world.remove_resource::<FontCx>() else {
            break;
        };
        let Some(mut layout) = world.remove_resource::<LayoutCx>() else {
            world.insert_resource(fonts);
            break;
        };
        let mut editor = world.get::<EditableText>(entity).unwrap().editor.clone();
        editor
            .driver(&mut fonts.context, &mut layout.0)
            .move_to_byte(byte);
        let geometry = editor.cursor_geometry(2.0);
        world.insert_resource(fonts);
        world.insert_resource(layout);
        if let Some(rect) = geometry {
            let key = format!("{}:{session}", entity.to_bits());
            let computed = world
                .get::<ComputedNode>(entity)
                .copied()
                .unwrap_or_default();
            let scroll = world
                .get::<bevy::ui::widget::TextScroll>(entity)
                .map_or(Vec2::ZERO, |scroll| scroll.0);
            let origin = Vec2::new(rect.x0 as f32, rect.y0 as f32) - scroll;
            let bottom = (rect.y1 as f32 - scroll.y).min(computed.content_box().height());
            if computed.size != Vec2::ZERO
                && (origin.x < 0.0
                    || origin.x > computed.content_box().width()
                    || bottom <= 0.0
                    || origin.y >= computed.content_box().height())
            {
                continue;
            }
            let scale = computed.inverse_scale_factor;
            let node = Node {
                position_type: PositionType::Absolute,
                left: px((computed.padding.min_inset.x + origin.x) * scale),
                top: px((computed.padding.min_inset.y + origin.y.max(0.0)) * scale),
                width: px(2),
                height: px((bottom - origin.y.max(0.0)).max(0.0) * scale),
                ..default()
            };
            if let Some(cursor) = old.remove(&key) {
                if world.get::<Node>(cursor) != Some(&node) {
                    world.entity_mut(cursor).insert(node);
                }
            } else {
                let caret = world
                    .spawn((
                        RemoteCursor(key),
                        node,
                        Pickable::IGNORE,
                        crate::token_style::background(crate::tokens::Token::Accent),
                        ChildOf(entity),
                    ))
                    .id();
                if world.contains_resource::<crate::theme::Typography>() {
                    let label = crate::edit_mode::label(world, caret, &name, 10.0);
                    world.entity_mut(label).insert((
                        BindingStatus,
                        Pickable::IGNORE,
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: percent(100),
                            left: px(0),
                            width: px(180),
                            ..default()
                        },
                        TextLayout::linebreak(bevy::text::LineBreak::NoWrap),
                    ));
                }
            }
        }
    }
    for entity in old.into_values() {
        world.despawn(entity);
    }
    world.insert_resource(bindings);
}
