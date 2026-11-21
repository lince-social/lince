// non-lean-yet
// Scope: mod drafts, mod feeds, mod persistence, mod tests.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 229, 237, 238,
//   268, 276, 364, 366, 375, 413, 419, 443, 453, 455, 456, 461, 545, 572, 581, 590, 608, 609, 610,
//   680, 684, 713, 723, 728, 891, 926, 927, 934, 963, 999, 1005, 1008, 1017, 1030, 1046, 1052,
//   1063, 1079, 1092, 1112, 1184, 1197, 1210, 1226, 1263, 1270, 1294, 1335, 1361, 1364, 1496, 1497.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (353).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   295, 302, 309, 313, 314, 601, 605, 606, 746, 759, 782, 803, 806, 814, 824, 838, 839, 851, 858,
//   874, 878.

mod drafts;
mod feeds;
mod persistence;
#[cfg(test)]
mod tests;
mod ui;

use crate::native::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use drafts::DraftData;
use nucleus::record_extension::{Choice, Field, FieldKind, Inspection, Preset, Request, Schema};
pub(crate) use persistence::{SavedExtensions, snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Mode {
    #[default]
    Editor,
    Dropdown,
    Column,
}

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Settings {
    mode: Mode,
    record: Option<String>,
    source: Source,
    schema: Option<String>,
    field: Option<String>,
}

impl Settings {
    fn valid(&self) -> bool {
        self.record.as_ref().is_none_or(|value| {
            nucleus::valid_uid(value, "r") || value.len() <= 200 && nucleus::valid_slug(value)
        }) && self
            .schema
            .as_ref()
            .is_none_or(|value| nucleus::valid_uid(value, "r"))
            && self.field.as_ref().is_none_or(|field| {
                nucleus::record_extension::column_binding(&nucleus::record_extension::column(
                    self.schema.as_deref().unwrap_or(""),
                    field,
                ))
                .is_some()
            })
            && match &self.source {
                Source::Local => true,
                Source::Organ(uid) => nucleus::valid_uid(uid, "r"),
            }
    }
}

#[derive(Clone)]
struct SchemaDraft {
    uid: Option<String>,
    revision: u64,
    schema: DraftData<Schema>,
    field: usize,
    choice: Option<usize>,
}

#[derive(Clone)]
struct ValueDraft {
    schema: String,
    schema_revision: u64,
    revision: u64,
    fields: DraftData<BTreeMap<String, Value>>,
    attached: bool,
}

impl ValueDraft {
    fn set(&mut self, field: &str, value: Value) {
        if self
            .fields
            .get(field)
            .is_some_and(|old| nucleus::json::same_value_encoding(old, &value))
        {
            return;
        }
        self.fields.edit().insert(field.into(), value);
    }

    fn remove(&mut self, field: &str) {
        if self.fields.contains_key(field) {
            self.fields.edit().remove(field);
        }
    }
}

#[derive(Clone)]
struct Undo {
    schema: String,
    schema_revision: u64,
    fields: Arc<BTreeMap<String, Value>>,
    attached: bool,
    revision: u64,
}

#[derive(Component, Default)]
struct View {
    reference_labels: BTreeMap<String, String>,
    focus: Option<(String, Option<String>)>,
    record_input: Option<String>,
    picker: Option<Picker>,
    feed: Option<Arc<FeedKey>>,
    seen: u64,
    schema: Option<SchemaDraft>,
    values: Option<ValueDraft>,
    undo: Option<Undo>,
    pending: Option<String>,
    retry: Option<(Option<String>, Request)>,
    column: Option<Entity>,
    editable: bool,
    open: Option<String>,
    search: String,
    page: usize,
    message: String,
    render: bool,
}

impl View {
    fn dirty(&self) -> bool {
        self.schema
            .as_ref()
            .is_some_and(|draft| draft.schema.changed())
            || self
                .values
                .as_ref()
                .is_some_and(|draft| draft.fields.changed())
    }
}

#[derive(Component, Clone)]
struct Input {
    owner: Entity,
    key: InputKey,
    observed: String,
}

#[derive(Clone)]
enum InputKey {
    Record,
    Search,
    SchemaName,
    FieldName,
    ChoiceName,
    Preset(usize, u8),
    Value(String),
}

#[derive(Clone)]
struct Picker {
    key: InputKey,
    rows: Arc<Vec<Value>>,
    request: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct FeedKey {
    source: Source,
    target: Option<String>,
    catalog: bool,
}

impl FeedKey {
    fn matches(&self, settings: &Settings) -> bool {
        self.source == settings.source
            && self.target == settings.record
            && self.catalog == (settings.mode != Mode::Column)
    }
}

struct Feed {
    schemas: Vec<String>,
    data: Option<Arc<Inspection>>,
    error: String,
    version: u64,
    next: Instant,
    pending: Option<String>,
}

enum PendingKind {
    Inspect(Arc<FeedKey>),
    Change(Entity),
    Picker(Entity),
}
struct Pending {
    source: Source,
    kind: PendingKind,
    started: Instant,
}

#[derive(Resource, Default)]
struct Runtime {
    feeds: HashMap<Arc<FeedKey>, Feed>,
    pending: HashMap<String, Pending>,
    wake: Option<Instant>,
}

pub(crate) struct ExtensionPlugin;
impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                update.after(crate::native::protein_area::UpdateProteinAreas),
            );
    }
}

fn sender(world: &World, source: &Source) -> Option<crate::native::cell_bridge::Sender> {
    crate::native::protein_area::editor_sender(
        world,
        &RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: String::new(),
            source: source.clone(),
        },
    )
}

fn key(settings: &Settings) -> Arc<FeedKey> {
    nucleus::performance::add("extension-feed-keys-prepared", 1);
    Arc::new(FeedKey {
        source: settings.source.clone(),
        target: settings.record.clone(),
        catalog: settings.mode != Mode::Column,
    })
}

fn feed(world: &World, owner: Entity) -> Option<&Feed> {
    let key = world.get::<View>(owner)?.feed.as_ref()?;
    world.get_resource::<Runtime>()?.feeds.get(key)
}

fn inspection(world: &World, owner: Entity) -> Option<&Arc<Inspection>> {
    if crate::native::laboratory::blocked(world, owner) {
        return None;
    }
    feed(world, owner)?.data.as_ref()
}

fn capture(world: &mut World, owner: Entity) {
    capture_matching(world, |candidate| candidate == owner);
}

fn capture_matching(world: &mut World, includes: impl Fn(Entity) -> bool) {
    let _capture = nucleus::performance::Span::new("extension-field-capture");
    let inputs: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter(|(_, input, _)| includes(input.owner))
        .filter_map(|(entity, input, text)| {
            let text = text.value();
            (text != input.observed.as_str())
                .then(|| (entity, input.owner, input.key.clone(), text.to_string()))
        })
        .collect();
    nucleus::performance::add("extension-field-values-prepared", inputs.len() as u64);
    for (entity, owner, key, value) in inputs {
        let Some(mut input) = world.get_mut::<Input>(entity) else {
            continue;
        };
        input.observed = value.clone();
        let Some(mut view) = world.get_mut::<View>(owner) else {
            continue;
        };
        match key {
            InputKey::Record => {
                view.record_input = Some(value);
            }
            InputKey::Search => {
                view.search = value;
                view.page = 0;
            }
            InputKey::SchemaName => {
                if let Some(draft) = &mut view.schema {
                    draft.schema.edit().name = value;
                }
            }
            InputKey::FieldName => {
                if let Some(draft) = &mut view.schema {
                    draft.schema.edit().fields[draft.field].name = value;
                }
            }
            InputKey::ChoiceName => {
                if let Some(draft) = &mut view.schema
                    && let Some(choice) = draft.choice
                {
                    draft.schema.edit().fields[draft.field].choices[choice].name = value;
                }
            }
            InputKey::Preset(index, part) => {
                if let Some(draft) = &mut view.schema
                    && let Some(choice) = draft.choice
                    && index
                        < draft.schema.fields[draft.field].choices[choice]
                            .assertions
                            .len()
                {
                    let preset = &mut draft.schema.edit().fields[draft.field].choices[choice]
                        .assertions[index];
                    let optional = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                    match part {
                        0 => preset.predicate = value.trim().into(),
                        1 => preset.object = optional,
                        2 => preset.quantity = optional,
                        _ => preset.unit = optional,
                    }
                }
            }
            InputKey::Value(field) => {
                if let Some(draft) = &mut view.values {
                    draft.set(&field, Value::String(value));
                }
            }
        }
    }
}

fn draft_values(world: &mut World, owner: Entity, schema: &str) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if view
        .values
        .as_ref()
        .is_some_and(|draft| draft.schema == schema)
    {
        return;
    }
    let value = inspection(world, owner)
        .and_then(|data| data.values.get(schema))
        .cloned()
        .unwrap_or_default();
    let schema_revision = inspection(world, owner)
        .and_then(|data| data.schemas.iter().find(|entry| entry.uid == schema))
        .map_or(0, |entry| entry.revision);
    let mut view = world
        .get_mut::<View>(owner)
        .expect("the inspected extension view still exists");
    view.values = Some(ValueDraft {
        schema: schema.into(),
        schema_revision,
        revision: value.revision,
        fields: DraftData::new(value.fields),
        attached: value.attached,
    });
}

fn search_picker(world: &mut World, owner: Entity) -> Result<(), String> {
    let settings = world
        .get::<Settings>(owner)
        .ok_or("Record extension settings are unavailable")?
        .clone();
    let view = world
        .get::<View>(owner)
        .ok_or("The Record extension view is closed")?;
    let picker = view.picker.as_ref().ok_or("Picker closed")?;
    let concept = matches!(picker.key, InputKey::Preset(_, 0 | 3));
    let query = protein::Protein {
        source: if concept {
            protein::Source::Concept
        } else {
            protein::Source::Record
        },
        filter: if view.search.trim().is_empty() {
            vec![]
        } else {
            vec![protein::Predicate::TextContains(view.search.trim().into())]
        },
        fields: if concept {
            None
        } else {
            Some(vec!["uid".into(), "head".into(), "slug".into()])
        },
        limit: Some(50),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
    };
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Choices are unavailable in the Laboratory".into());
    }
    let sender = sender(world, &settings.source).ok_or("Connect to the Organ first")?;
    world.init_resource::<Runtime>();
    if world.resource::<Runtime>().pending.len() >= 128 {
        return Err("Picker busy; try again".into());
    }
    let id = nucleus::new_uid("extension");
    sender
        .try_send(ClientMessage::Subscribe {
            id: id.clone(),
            protein: query,
        })
        .map_err(|error| error.to_string())?;
    let mut view = editor_mut(world, owner)?;
    if let Some(picker) = &mut view.picker {
        picker.request = Some(id.clone());
    }
    view.message = "Loading choices…".into();
    world.resource_mut::<Runtime>().pending.insert(
        id,
        Pending {
            source: settings.source,
            kind: PendingKind::Picker(owner),
            started: Instant::now(),
        },
    );
    Ok(())
}

fn send_change(
    world: &mut World,
    owner: Entity,
    target: Option<String>,
    request: Request,
) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    world.get::<View>(owner).ok_or("Editor closed")?;
    let settings = world.get::<Settings>(owner).ok_or("Editor closed")?.clone();
    let source = settings.source;
    let sender = sender(world, &source).ok_or("Connect to the Record's Organ first")?;
    world.init_resource::<Runtime>();
    if world.resource::<Runtime>().pending.len() >= 128 {
        return Err("Extension editor busy; try again".into());
    }
    let id = nucleus::new_uid("extension");
    sender
        .try_send(ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::RecordExtensions {
                target: target.clone(),
                request: request.clone(),
            },
        })
        .map_err(|error| error.to_string())?;
    world.resource_mut::<Runtime>().pending.insert(
        id.clone(),
        Pending {
            source,
            kind: PendingKind::Change(owner),
            started: Instant::now(),
        },
    );
    let mut view = editor_mut(world, owner)?;
    view.pending = Some(id);
    view.retry = Some((target, request));
    view.message = "Saving…".into();
    view.render = true;
    Ok(())
}

fn editor_mut(world: &mut World, owner: Entity) -> Result<Mut<'_, View>, &'static str> {
    world.get_mut::<View>(owner).ok_or("Editor closed")
}

#[derive(Clone)]
enum Command {
    Pick(InputKey),
    Picked(String),
    ClosePicker,
    Record,
    New,
    Edit(String),
    Done,
    Discard,
    SaveSchema,
    AddField(FieldKind),
    Kind(FieldKind),
    Field(usize),
    ArchiveField,
    MoveField(bool),
    AddChoice,
    Choice(usize),
    ArchiveChoice,
    MoveChoice(bool),
    AddPreset,
    RemovePreset(usize),
    Schema(String),
    Bind(String, String),
    Open(String),
    Search,
    Page(bool),
    Set(String, Value),
    Select(String, String),
    Clear(String),
    Apply,
    Detach,
    Undo,
    Reload,
    Retry,
    Column(Entity),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !world.entities().contains(owner) {
            return;
        }
        capture(world, owner);
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        if view.pending.is_some() {
            return;
        }
        let result = apply(world, owner, self);
        if let Some(mut view) = world.get_mut::<View>(owner) {
            if let Err(error) = result {
                view.message = error;
            }
            view.render = true;
        }
    }
}

fn apply(world: &mut World, owner: Entity, command: &Command) -> Result<(), String> {
    match command {
        Command::Pick(key) => {
            let mut view = editor_mut(world, owner)?;
            view.picker = Some(Picker {
                key: key.clone(),
                rows: Default::default(),
                request: None,
            });
            view.search.clear();
            return search_picker(world, owner);
        }
        Command::ClosePicker => {
            editor_mut(world, owner)?.picker = None;
        }
        Command::Picked(uid) => {
            let name = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .picker
                .as_ref()
                .and_then(|picker| {
                    picker
                        .rows
                        .iter()
                        .find(|row| row["uid"].as_str() == Some(uid))
                })
                .and_then(|row| row["name"].as_str().or(row["head"].as_str()))
                .map(|name| name.chars().take(160).collect::<String>());
            if let Some(name) = name {
                editor_mut(world, owner)?
                    .reference_labels
                    .insert(uid.clone(), name);
            }
            let key = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .picker
                .as_ref()
                .ok_or("Picker closed")?
                .key
                .clone();
            match key {
                InputKey::Record => {
                    if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                        return Err("Save or discard your draft first".into());
                    }
                    world
                        .get_mut::<Settings>(owner)
                        .ok_or("Editor settings are unavailable")?
                        .record = Some(uid.clone());
                    let mut view = editor_mut(world, owner)?;
                    view.values = None;
                    view.undo = None;
                    view.record_input = None;
                }
                InputKey::Preset(index, part) => {
                    let mut view = editor_mut(world, owner)?;
                    if let Some(draft) = &mut view.schema
                        && let Some(choice) = draft.choice
                        && index
                            < draft.schema.fields[draft.field].choices[choice]
                                .assertions
                                .len()
                    {
                        let preset = &mut draft.schema.edit().fields[draft.field].choices[choice]
                            .assertions[index];
                        match part {
                            0 => preset.predicate = uid.clone(),
                            1 => preset.object = Some(uid.clone()),
                            _ => preset.unit = Some(uid.clone()),
                        }
                    }
                }
                _ => {}
            }
            let mut view = editor_mut(world, owner)?;
            view.picker = None;
            view.search.clear();
            view.message.clear();
        }
        Command::Record => {
            let view = world.get::<View>(owner).ok_or("Editor closed")?;
            if view.dirty() {
                return Err("Apply or discard your draft before changing Records".into());
            }
            let settings = world
                .get::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?;
            let value = view
                .record_input
                .as_ref()
                .or(settings.record.as_ref())
                .cloned()
                .unwrap_or_default();
            if value.len() > 200 || value.contains('\0') {
                return Err("Choose a Record from the list or enter its reference".into());
            }
            world
                .get_mut::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .record = (!value.trim().is_empty()).then(|| value.trim().into());
            let mut view = editor_mut(world, owner)?;
            view.values = None;
            view.undo = None;
            view.schema = None;
            view.open = None;
            view.message.clear();
        }
        Command::New => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Save or discard your draft first".into());
            }
            let schema = Schema {
                name: "New schema".into(),
                fields: vec![new_field(FieldKind::Select)],
            };
            let mut view = editor_mut(world, owner)?;
            view.schema = Some(SchemaDraft {
                uid: None,
                revision: 0,
                schema: DraftData::new(schema),
                field: 0,
                choice: None,
            });
            view.values = None;
            view.message.clear();
        }
        Command::Edit(uid) => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Save or discard your draft first".into());
            }
            let definition = inspection(world, owner)
                .and_then(|data| data.schemas.iter().find(|schema| &schema.uid == uid))
                .ok_or("Schema unavailable")?;
            if !definition.editable {
                return Err("This schema is read only".into());
            }
            let (revision, schema) = (
                definition.revision,
                DraftData::new(definition.schema.clone()),
            );
            let mut view = editor_mut(world, owner)?;
            view.schema = Some(SchemaDraft {
                uid: Some(uid.clone()),
                revision,
                schema,
                field: 0,
                choice: None,
            });
            view.values = None;
            view.open = None;
        }
        Command::Discard => {
            let mut view = editor_mut(world, owner)?;
            view.schema = None;
            view.values = None;
            view.message = "Draft discarded".into();
            view.open = None;
        }
        Command::Done => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Save or discard your draft first".into());
            }
            editor_mut(world, owner)?.schema = None;
        }
        Command::SaveSchema => {
            let draft = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .schema
                .as_ref()
                .ok_or("Open a schema")?
                .clone();
            if draft.uid.is_some() {
                draft.schema.update_from(draft.schema.original())?;
            } else {
                draft.schema.validate()?;
            }
            let request = if draft.uid.is_some() {
                Request::Save {
                    id: nucleus::new_uid("op"),
                    expected_revision: draft.revision,
                    schema: (*draft.schema).clone(),
                }
            } else {
                Request::Create {
                    id: nucleus::new_uid("op"),
                    schema: (*draft.schema).clone(),
                }
            };
            return send_change(world, owner, draft.uid, request);
        }
        Command::AddField(kind) => {
            let mut view = editor_mut(world, owner)?;
            let draft = view.schema.as_mut().ok_or("Open a schema")?;
            if draft.schema.fields.len() >= 64 {
                return Err("A schema can have at most 64 fields".into());
            }
            draft.schema.edit().fields.push(new_field(*kind));
            draft.field = draft.schema.fields.len() - 1;
            draft.choice = None;
        }
        Command::Kind(kind) => {
            let mut view = editor_mut(world, owner)?;
            let draft = view.schema.as_mut().ok_or("Open a schema")?;
            let field = &draft.schema.fields[draft.field];
            if draft.uid.is_some()
                && draft
                    .schema
                    .original()
                    .fields
                    .iter()
                    .any(|old| old.id == field.id)
            {
                return Err(
                    "Saved field types stay fixed. Add a new field for another type.".into(),
                );
            }
            let field = &mut draft.schema.edit().fields[draft.field];
            field.kind = *kind;
            if !kind.choices() {
                field.choices.clear();
                draft.choice = None;
            } else if field.choices.is_empty() {
                field.choices.push(new_choice());
            }
        }
        Command::Field(index) => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && *index < draft.schema.fields.len()
            {
                draft.field = *index;
                draft.choice = None;
                view.page = 0;
                view.search.clear();
            }
        }
        Command::ArchiveField => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema {
                let field = &mut draft.schema.edit().fields[draft.field];
                field.archived = !field.archived;
            }
        }
        Command::MoveField(down) => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema {
                let other = if *down {
                    draft.field + 1
                } else {
                    draft.field.saturating_sub(1)
                };
                if other < draft.schema.fields.len() && other != draft.field {
                    draft.schema.edit().fields.swap(draft.field, other);
                    draft.field = other;
                }
            }
        }
        Command::AddChoice => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema {
                if draft.schema.fields[draft.field].choices.len() >= 512 {
                    return Err("A field can have at most 512 choices".into());
                }
                let choices = &mut draft.schema.edit().fields[draft.field].choices;
                choices.push(new_choice());
                draft.choice = Some(choices.len() - 1);
            }
        }
        Command::Choice(index) => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && *index < draft.schema.fields[draft.field].choices.len()
            {
                draft.choice = Some(*index);
            }
        }
        Command::ArchiveChoice => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                let choice = &mut draft.schema.edit().fields[draft.field].choices[index];
                choice.archived = !choice.archived;
            }
        }
        Command::MoveChoice(down) => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                let other = if *down {
                    index + 1
                } else {
                    index.saturating_sub(1)
                };
                if other < draft.schema.fields[draft.field].choices.len() && other != index {
                    draft.schema.edit().fields[draft.field]
                        .choices
                        .swap(index, other);
                    draft.choice = Some(other);
                }
            }
        }
        Command::AddPreset => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                if draft.schema.fields[draft.field].choices[index]
                    .assertions
                    .len()
                    >= 16
                {
                    return Err("A choice can have at most 16 preset assertions".into());
                }
                draft.schema.edit().fields[draft.field].choices[index]
                    .assertions
                    .push(Preset {
                        predicate: String::new(),
                        object: None,
                        quantity: None,
                        unit: None,
                    });
            }
        }
        Command::RemovePreset(index) => {
            let mut view = editor_mut(world, owner)?;
            if let Some(draft) = &mut view.schema
                && let Some(choice) = draft.choice
            {
                if *index
                    < draft.schema.fields[draft.field].choices[choice]
                        .assertions
                        .len()
                {
                    draft.schema.edit().fields[draft.field].choices[choice]
                        .assertions
                        .remove(*index);
                }
            }
        }
        Command::Schema(uid) => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Apply or discard your draft first".into());
            }
            world
                .get_mut::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .schema = Some(uid.clone());
            let mut view = editor_mut(world, owner)?;
            view.values = None;
            view.open = None;
            view.page = 0;
        }
        Command::Bind(schema, field) => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Apply or discard your draft first".into());
            }
            let column = world.get::<View>(owner).ok_or("Editor closed")?.column;
            if let Some(area) = column {
                let mut config =
                    crate::native::protein_area::configuration(world, area).ok_or("Area closed")?;
                let property = nucleus::record_extension::column(schema, field);
                if config.bindings.len() >= 32 {
                    return Err("A template can have at most 32 columns".into());
                }
                if !config
                    .bindings
                    .iter()
                    .any(|binding| binding.property == property)
                {
                    let mut binding = crate::native::protein_area::Binding::new(&property);
                    binding.editable = true;
                    binding.height = 180.0;
                    config.bindings.push(binding);
                    crate::native::protein_area::set_configuration(world, area, Some(config));
                }
                world.despawn(owner);
                return Ok(());
            }
            let mut settings = world
                .get_mut::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?;
            settings.schema = Some(schema.clone());
            settings.field = Some(field.clone());
        }
        Command::Open(field) => {
            let mut view = editor_mut(world, owner)?;
            view.open = if view.open.as_ref() == Some(field) {
                None
            } else {
                Some(field.clone())
            };
            view.search.clear();
            view.page = 0;
        }
        Command::Search => {
            if world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .picker
                .is_some()
            {
                return search_picker(world, owner);
            }
        }
        Command::Page(down) => {
            let mut view = editor_mut(world, owner)?;
            view.page = if *down {
                view.page.saturating_add(1).min(32)
            } else {
                view.page.saturating_sub(1)
            };
        }
        Command::Set(field, value) => {
            prepare_value(world, owner)?;
            editor_mut(world, owner)?
                .values
                .as_mut()
                .ok_or("The value draft is unavailable")?
                .set(field, value.clone());
        }
        Command::Clear(field) => {
            prepare_value(world, owner)?;
            editor_mut(world, owner)?
                .values
                .as_mut()
                .ok_or("The value draft is unavailable")?
                .remove(field);
        }
        Command::Select(field, choice) => {
            prepare_value(world, owner)?;
            let data = inspection(world, owner)
                .cloned()
                .ok_or("Field unavailable")?;
            let schema = world
                .get::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .schema
                .as_ref()
                .ok_or("Choose a schema")?;
            let definition = data
                .schemas
                .iter()
                .find(|entry| &entry.uid == schema)
                .and_then(|entry| entry.schema.fields.iter().find(|entry| entry.id == *field))
                .ok_or("Field unavailable")?;
            let mut view = editor_mut(world, owner)?;
            let draft = view
                .values
                .as_mut()
                .ok_or("The value draft is unavailable")?;
            let selected = definition.selection(draft.fields.get(field).unwrap_or(&Value::Null))?;
            if definition.kind == FieldKind::Select {
                draft.set(field, json!(choice));
                view.open = None;
                view.focus = Some((field.clone(), None));
            } else {
                let mut selected = selected.into_iter().map(str::to_owned).collect::<Vec<_>>();
                if let Ok(index) = selected.binary_search(choice) {
                    selected.remove(index);
                } else {
                    selected.push(choice.clone());
                }
                draft.set(field, json!(selected));
                view.focus = Some((field.clone(), Some(choice.clone())));
            }
        }
        Command::Apply | Command::Detach => {
            prepare_value(world, owner)?;
            let target = world
                .get::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .record
                .clone();
            let draft = world
                .get::<View>(owner)
                .and_then(|view| view.values.as_ref())
                .ok_or("The value draft is unavailable")?;
            let remove = matches!(command, Command::Detach);
            let values = if remove {
                BTreeMap::new()
            } else {
                draft.fields.apply_values()
            };
            let request = Request::Apply {
                id: nucleus::new_uid("op"),
                schema: draft.schema.clone(),
                expected_revision: draft.revision,
                expected_schema_revision: draft.schema_revision,
                values,
                remove,
            };
            return send_change(world, owner, target, request);
        }
        Command::Undo => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Apply or discard your draft before undoing".into());
            }
            let undo = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .undo
                .clone()
                .ok_or("Nothing to undo")?;
            let target = world
                .get::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .record
                .clone();
            return send_change(
                world,
                owner,
                target,
                Request::Apply {
                    id: nucleus::new_uid("op"),
                    schema: undo.schema,
                    expected_revision: undo.revision,
                    expected_schema_revision: undo.schema_revision,
                    values: if undo.attached {
                        (*undo.fields).clone()
                    } else {
                        BTreeMap::new()
                    },
                    remove: !undo.attached,
                },
            );
        }
        Command::Reload => {
            if world.get::<View>(owner).ok_or("Editor closed")?.dirty() {
                return Err("Discard your draft before reloading".into());
            }
            let feed = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .feed
                .clone();
            if let Some(key) = feed
                && let Some(feed) = world.resource_mut::<Runtime>().feeds.get_mut(&key)
            {
                feed.next = Instant::now();
            }
            editor_mut(world, owner)?.values = None;
        }
        Command::Retry => {
            let (target, request) = world
                .get::<View>(owner)
                .ok_or("Editor closed")?
                .retry
                .clone()
                .ok_or("Nothing to retry")?;
            return send_change(world, owner, target, request);
        }
        Command::Column(area) => {
            editor_mut(world, owner)?.column = Some(*area);
            world
                .get_mut::<Settings>(owner)
                .ok_or("Editor settings are unavailable")?
                .record = None;
        }
    }
    Ok(())
}

fn prepare_value(world: &mut World, owner: Entity) -> Result<(), String> {
    let schema = world
        .get::<Settings>(owner)
        .ok_or("Editor settings are unavailable")?
        .schema
        .clone()
        .ok_or("Choose a schema")?;
    if !world.get::<View>(owner).ok_or("Editor closed")?.editable
        || !inspection(world, owner).is_some_and(|data| data.writable.contains(&schema))
    {
        return Err("These fields are read only or unavailable".into());
    }
    draft_values(world, owner, &schema);
    Ok(())
}

fn new_choice() -> Choice {
    Choice {
        id: nucleus::new_uid("choice"),
        name: "New choice".into(),
        archived: false,
        assertions: vec![],
    }
}
fn new_field(kind: FieldKind) -> Field {
    Field {
        id: nucleus::new_uid("field"),
        name: kind.name().into(),
        kind,
        archived: false,
        choices: if kind.choices() {
            vec![new_choice()]
        } else {
            vec![]
        },
    }
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    if world.get_resource::<Runtime>().is_none() {
        return;
    }
    if matches!(message,ServerMessage::Error{id,code,..} if id==crate::native::cell_bridge::CONNECTION||id=="connection"||code.as_deref()==Some("session_expired"))
    {
        for (_, feed) in world
            .resource_mut::<Runtime>()
            .feeds
            .iter_mut()
            .filter(|(key, _)| &key.source == source)
        {
            feed.data = None;
            feed.error = "Access unavailable; reconnect to reload".into();
            feed.version += 1;
            feed.next = Instant::now() + Duration::from_secs(5);
        }
    }
    let id = match message {
        ServerMessage::Snapshot { id, .. }
        | ServerMessage::ActionOk { id, .. }
        | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    if !world
        .resource::<Runtime>()
        .pending
        .get(id)
        .is_some_and(|pending| &pending.source == source)
    {
        return;
    }
    let _reply = nucleus::performance::Span::new("extension-reply-preparation");
    let Some(pending) = world.resource_mut::<Runtime>().pending.remove(id) else {
        return;
    };
    match pending.kind {
        PendingKind::Picker(owner) => {
            if let Some(sender) = sender(world, source) {
                let _ = sender.try_send(ClientMessage::Unsubscribe { id: id.clone() });
            }
            if let Some(mut view) = world.get_mut::<View>(owner) {
                if !view
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.request.as_ref() == Some(id))
                {
                    return;
                }
                match message {
                    ServerMessage::Snapshot { rows, .. } => {
                        if let Some(picker) = &mut view.picker {
                            picker.rows = rows.clone();
                        }
                    }
                    ServerMessage::ActionOk { data, .. } => {
                        if let Some(picker) = &mut view.picker {
                            picker.rows = Arc::new(
                                data.as_ref()
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default(),
                            );
                        }
                    }
                    ServerMessage::Error { message, .. } => view.message = message.clone(),
                    _ => unreachable!(),
                }
                view.render = true;
            }
        }
        PendingKind::Inspect(key) => {
            let result = match message {
                ServerMessage::ActionOk { data, .. } => {
                    Inspection::deserialize(data.as_ref().unwrap_or(&Value::Null))
                        .map_err(|error| error.to_string())
                }
                ServerMessage::Snapshot { rows, .. } => {
                    serde_json::from_value::<Inspection>(json!(rows))
                        .map_err(|error| error.to_string())
                }
                ServerMessage::Error { message, .. } => Err(message.clone()),
                _ => unreachable!(),
            };
            if let Some(feed) = world.resource_mut::<Runtime>().feeds.get_mut(&key) {
                feed.pending = None;
                feed.next = Instant::now() + Duration::from_secs(3);
                match result {
                    Ok(data) => {
                        let changed = feed.data.as_deref() != Some(&data);
                        if changed || !feed.error.is_empty() {
                            feed.version += 1;
                        }
                        feed.data = Some(Arc::new(data));
                        nucleus::performance::add("extension-inspection-snapshots-prepared", 1);
                        feed.error.clear();
                    }
                    Err(error) => {
                        feed.data = None;
                        feed.error = error;
                        feed.version += 1;
                    }
                }
            }
        }
        PendingKind::Change(owner) => {
            let Some(mut view) = world.get_mut::<View>(owner) else {
                return;
            };
            let result = match message {
                ServerMessage::ActionOk { data, .. } => Ok(data.as_ref().unwrap_or(&Value::Null)),
                ServerMessage::Snapshot { .. } => Ok(&Value::Null),
                ServerMessage::Error { message, .. } => Err(message),
                _ => unreachable!(),
            };
            view.pending = None;
            view.render = true;
            match result {
                Err(error) => view.message = error.clone(),
                Ok(data) => {
                    let schema_saved = view.retry.as_ref().is_some_and(|(_, request)| {
                        matches!(request, Request::Create { .. } | Request::Save { .. })
                    });
                    let applied_schema =
                        view.retry.as_ref().and_then(|(_, request)| match request {
                            Request::Apply { schema, .. } => Some(schema.clone()),
                            _ => None,
                        });
                    if schema_saved {
                        if let Some(uid) = data["schema"].as_str() {
                            let Some(mut settings) = world.get_mut::<Settings>(owner) else {
                                return;
                            };
                            settings.schema = Some(uid.into());
                        }
                        let Some(mut view) = world.get_mut::<View>(owner) else {
                            return;
                        };
                        view.schema = None;
                        view.values = None;
                        view.message =
                            "Schema saved. Labels changed without changing selection IDs.".into();
                        view.retry = None;
                    } else if let Some(schema) = applied_schema {
                        let revision = data["revision"].as_u64().unwrap_or(0);
                        if let Some(draft) = &view.values {
                            view.undo = Some(Undo {
                                schema,
                                schema_revision: draft.schema_revision,
                                fields: draft.fields.undo_values(),
                                attached: draft.attached,
                                revision,
                            });
                        } else {
                            view.undo = None;
                        }
                        view.values = None;
                        view.retry = None;
                        view.open = None;
                        view.message = "Saved values and preset assertions".into();
                    }
                    for (_, feed) in world
                        .resource_mut::<Runtime>()
                        .feeds
                        .iter_mut()
                        .filter(|(key, _)| &key.source == source)
                    {
                        feed.next = Instant::now();
                    }
                }
            }
        }
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages.iter().map(|message| message.as_ref()) {
        receive(world, &Source::Local, &message);
    }
    ui::keyboard(world);
    let owners = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .filter(|owner| !crate::native::laboratory::blocked(world, *owner))
        .collect::<Vec<_>>();
    let active: HashSet<_> = owners.iter().copied().collect();
    capture_matching(world, |owner| active.contains(&owner));
    feeds::update(world, &owners);
    for owner in owners {
        let Some(view) = world.get::<View>(owner) else {
            continue;
        };
        let state = {
            feed(world, owner)
                .filter(|feed| feed.version != view.seen)
                .map(|feed| {
                    (
                        feed.version,
                        feed.data.as_ref().and_then(|data| data.record.clone()),
                        feed.data.is_none(),
                        if feed.data.is_none() {
                            feed.error.clone()
                        } else {
                            String::new()
                        },
                        feed.data.is_some() && view.pending.is_none() && !view.dirty(),
                    )
                })
        };
        if let Some((version, record, unavailable, error, refresh_values)) = state {
            if let Some(uid) = record {
                let Some(mut settings) = world.get_mut::<Settings>(owner) else {
                    continue;
                };
                if settings.record.as_ref() != Some(&uid) {
                    settings.record = Some(uid);
                }
            }
            let Some(mut view) = world.get_mut::<View>(owner) else {
                continue;
            };
            view.seen = version;
            if unavailable {
                view.values = None;
                view.schema = None;
                view.reference_labels.clear();
                view.undo = None;
                view.open = None;
                view.message = error;
                view.render = true;
            } else if refresh_values {
                view.values = None;
                view.render = true;
            }
        }
        if world.get::<View>(owner).is_some_and(|view| view.render) {
            ui::render(world, owner);
            if let Some(mut view) = world.get_mut::<View>(owner) {
                view.render = false;
            }
        }
    }
}

pub(crate) fn field(
    world: &mut World,
    parent: Entity,
    binding: RecordBinding,
    schema: &str,
    field: &str,
    editable: bool,
) {
    world.entity_mut(parent).insert((
        Settings {
            mode: Mode::Column,
            record: Some(binding.uid),
            source: binding.source,
            schema: Some(schema.into()),
            field: Some(field.into()),
        },
        View {
            editable,
            render: true,
            ..default()
        },
    ));
}

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    settings: Settings,
) -> Entity {
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(660.0, 640.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            settings,
            View {
                editable: true,
                render: true,
                ..default()
            },
        ))
        .id();
    crate::native::scroll_sand::attach(world, owner);
    ui::render(world, owner);
    owner
}

#[derive(Clone)]
pub(crate) struct Open(pub RecordBinding);

fn workspace_ancestor(world: &World, mut entity: Entity) -> Option<(Entity, u64)> {
    loop {
        if let Some(workspaces) = world.get::<crate::native::workspace::Workspaces>(entity) {
            return Some((entity, workspaces.active));
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

impl Action for Open {
    fn apply(&self, world: &mut World, parent: Entity) {
        let Some((root, workspace)) = workspace_ancestor(world, parent) else {
            return;
        };
        let position = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        spawn(
            world,
            root,
            workspace,
            position,
            Settings {
                record: Some(self.0.uid.clone()),
                source: self.0.source.clone(),
                ..default()
            },
        );
    }
}

#[derive(Clone)]
pub(crate) struct AddColumn(pub Entity);
impl Action for AddColumn {
    fn apply(&self, world: &mut World, parent: Entity) {
        let Some((root, workspace)) = workspace_ancestor(world, parent) else {
            return;
        };
        let Some(config) = crate::native::protein_area::configuration(world, self.0) else {
            return;
        };
        let position = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        let owner = spawn(
            world,
            root,
            workspace,
            position,
            Settings {
                source: config.source,
                ..default()
            },
        );
        Command::Column(self.0).apply(world, owner);
    }
}

#[derive(Clone)]
struct Add(Mode);
impl Action for Add {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(workspace) = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return;
        };
        let position = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        spawn(
            world,
            root,
            workspace,
            position,
            Settings {
                mode: self.0,
                ..default()
            },
        );
        crate::native::edit_mode::EditAction::Close.apply(world, root);
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    for (title, description, mode) in [
        (
            "Extension Editor",
            "Schemas, fields and state labels",
            Mode::Editor,
        ),
        (
            "Dropdown Sand",
            "Single or multiple choices",
            Mode::Dropdown,
        ),
    ] {
        let entry = crate::native::description::button(
            world,
            parent,
            root,
            &format!("{title} · {description}"),
            Add(mode),
        );
        world
            .entity_mut(entry)
            .insert(crate::native::sand_store::StoreComponent {
                title: title.into(),
                description: description.into(),
                size: Vec2::new(660.0, 640.0),
            });
    }
}
