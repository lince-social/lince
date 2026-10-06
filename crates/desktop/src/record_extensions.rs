mod persistence;
#[cfg(test)]
mod tests;
mod ui;

use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use nucleus::record_extension::{Choice, Field, FieldKind, Inspection, Preset, Request, Schema};
pub(crate) use persistence::{SavedExtensions, snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
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
    original: Schema,
    schema: Schema,
    field: usize,
    choice: Option<usize>,
}

#[derive(Clone)]
struct ValueDraft {
    schema: String,
    schema_revision: u64,
    revision: u64,
    original: BTreeMap<String, Value>,
    fields: BTreeMap<String, Value>,
    attached: bool,
}

#[derive(Clone)]
struct Undo {
    schema: String,
    schema_revision: u64,
    fields: BTreeMap<String, Value>,
    attached: bool,
    revision: u64,
}

#[derive(Component, Default)]
struct View {
    reference_labels: BTreeMap<String, String>,
    focus: Option<(String, Option<String>)>,
    record_input: Option<String>,
    picker: Option<Picker>,
    feed: String,
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
            .is_some_and(|draft| draft.schema != draft.original)
            || self
                .values
                .as_ref()
                .is_some_and(|draft| draft.fields != draft.original)
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
    rows: Vec<Value>,
    request: Option<String>,
}

struct Feed {
    schemas: Vec<String>,
    source: Source,
    target: Option<String>,
    catalog: bool,
    data: Option<Inspection>,
    error: String,
    version: u64,
    next: Instant,
    pending: Option<String>,
}

enum PendingKind {
    Inspect(String),
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
    feeds: HashMap<String, Feed>,
    pending: HashMap<String, Pending>,
    wake: Option<Instant>,
}

pub(crate) struct ExtensionPlugin;
impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                update.after(crate::protein_area::UpdateProteinAreas),
            );
    }
}

fn sender(world: &World, source: &Source) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    crate::protein_area::editor_sender(
        world,
        &RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: String::new(),
            source: source.clone(),
        },
    )
}

fn key(settings: &Settings) -> String {
    serde_json::to_string(&(
        &settings.source,
        &settings.record,
        settings.mode != Mode::Column,
    ))
    .unwrap()
}

fn inspection(world: &World, owner: Entity) -> Option<&Inspection> {
    let view = world.get::<View>(owner)?;
    world
        .get_resource::<Runtime>()?
        .feeds
        .get(&view.feed)?
        .data
        .as_ref()
}

fn capture(world: &mut World, owner: Entity) {
    let inputs: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter(|(_, input, _)| input.owner == owner)
        .map(|(entity, input, text)| {
            (
                entity,
                input.key.clone(),
                text.value().to_string(),
                input.observed.clone(),
            )
        })
        .collect();
    for (entity, key, value, observed) in inputs {
        if value == observed {
            continue;
        }
        world.get_mut::<Input>(entity).unwrap().observed = value.clone();
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
                    draft.schema.name = value;
                }
            }
            InputKey::FieldName => {
                if let Some(draft) = &mut view.schema {
                    draft.schema.fields[draft.field].name = value;
                }
            }
            InputKey::ChoiceName => {
                if let Some(draft) = &mut view.schema
                    && let Some(choice) = draft.choice
                {
                    draft.schema.fields[draft.field].choices[choice].name = value;
                }
            }
            InputKey::Preset(index, part) => {
                if let Some(draft) = &mut view.schema
                    && let Some(choice) = draft.choice
                    && let Some(preset) = draft.schema.fields[draft.field].choices[choice]
                        .assertions
                        .get_mut(index)
                {
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
                    draft.fields.insert(field, Value::String(value));
                }
            }
        }
    }
}

fn draft_values(world: &mut World, owner: Entity, schema: &str) {
    if world
        .get::<View>(owner)
        .unwrap()
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
    world.get_mut::<View>(owner).unwrap().values = Some(ValueDraft {
        schema: schema.into(),
        schema_revision,
        revision: value.revision,
        original: value.fields.clone(),
        fields: value.fields,
        attached: value.attached,
    });
}

fn changes(draft: &ValueDraft) -> BTreeMap<String, Value> {
    let mut fields = draft.fields.clone();
    for key in draft.original.keys() {
        fields.entry(key.clone()).or_insert(Value::Null);
    }
    fields
}

fn search_picker(world: &mut World, owner: Entity) -> Result<(), String> {
    let settings = world.get::<Settings>(owner).unwrap().clone();
    let view = world.get::<View>(owner).unwrap();
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
    if let Some(picker) = &mut world.get_mut::<View>(owner).unwrap().picker {
        picker.request = Some(id.clone());
    }
    world.resource_mut::<Runtime>().pending.insert(
        id,
        Pending {
            source: settings.source,
            kind: PendingKind::Picker(owner),
            started: Instant::now(),
        },
    );
    world.get_mut::<View>(owner).unwrap().message = "Loading choices…".into();
    Ok(())
}

fn send_change(
    world: &mut World,
    owner: Entity,
    target: Option<String>,
    request: Request,
) -> Result<(), String> {
    if crate::laboratory::suspended(world, owner) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
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
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.pending = Some(id);
    view.retry = Some((target, request));
    view.message = "Saving…".into();
    view.render = true;
    Ok(())
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
            world.get_mut::<View>(owner).unwrap().picker = Some(Picker {
                key: key.clone(),
                rows: vec![],
                request: None,
            });
            world.get_mut::<View>(owner).unwrap().search.clear();
            return search_picker(world, owner);
        }
        Command::ClosePicker => {
            world.get_mut::<View>(owner).unwrap().picker = None;
        }
        Command::Picked(uid) => {
            let name = world
                .get::<View>(owner)
                .unwrap()
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
                world
                    .get_mut::<View>(owner)
                    .unwrap()
                    .reference_labels
                    .insert(uid.clone(), name);
            }
            let key = world
                .get::<View>(owner)
                .unwrap()
                .picker
                .as_ref()
                .ok_or("Picker closed")?
                .key
                .clone();
            match key {
                InputKey::Record => {
                    if world.get::<View>(owner).unwrap().dirty() {
                        return Err("Save or discard your draft first".into());
                    }
                    world.get_mut::<Settings>(owner).unwrap().record = Some(uid.clone());
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.values = None;
                    view.undo = None;
                    view.record_input = None;
                }
                InputKey::Preset(index, part) => {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    if let Some(draft) = &mut view.schema
                        && let Some(choice) = draft.choice
                        && let Some(preset) = draft.schema.fields[draft.field].choices[choice]
                            .assertions
                            .get_mut(index)
                    {
                        match part {
                            0 => preset.predicate = uid.clone(),
                            1 => preset.object = Some(uid.clone()),
                            _ => preset.unit = Some(uid.clone()),
                        }
                    }
                }
                _ => {}
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.picker = None;
            view.search.clear();
            view.message.clear();
        }
        Command::Record => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Apply or discard your draft before changing Records".into());
            }
            let value = world
                .get::<View>(owner)
                .unwrap()
                .record_input
                .clone()
                .or_else(|| world.get::<Settings>(owner).unwrap().record.clone())
                .unwrap_or_default();
            if value.len() > 200 || value.contains('\0') {
                return Err("Choose a Record from the list or enter its reference".into());
            }
            world.get_mut::<Settings>(owner).unwrap().record =
                (!value.trim().is_empty()).then(|| value.trim().into());
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.values = None;
            view.undo = None;
            view.schema = None;
            view.open = None;
            view.message.clear();
        }
        Command::New => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Save or discard your draft first".into());
            }
            let schema = Schema {
                name: "New schema".into(),
                fields: vec![new_field(FieldKind::Select)],
            };
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.schema = Some(SchemaDraft {
                uid: None,
                revision: 0,
                original: schema.clone(),
                schema,
                field: 0,
                choice: None,
            });
            view.values = None;
            view.message.clear();
        }
        Command::Edit(uid) => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Save or discard your draft first".into());
            }
            let definition = inspection(world, owner)
                .and_then(|data| data.schemas.iter().find(|schema| &schema.uid == uid))
                .ok_or("Schema unavailable")?
                .clone();
            if !definition.editable {
                return Err("This schema is read only".into());
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.schema = Some(SchemaDraft {
                uid: Some(uid.clone()),
                revision: definition.revision,
                original: definition.schema.clone(),
                schema: definition.schema,
                field: 0,
                choice: None,
            });
            view.values = None;
            view.open = None;
        }
        Command::Discard => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.schema = None;
            view.values = None;
            view.message = "Draft discarded".into();
            view.open = None;
        }
        Command::Done => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Save or discard your draft first".into());
            }
            world.get_mut::<View>(owner).unwrap().schema = None;
        }
        Command::SaveSchema => {
            let draft = world
                .get::<View>(owner)
                .unwrap()
                .schema
                .as_ref()
                .ok_or("Open a schema")?
                .clone();
            draft.schema.validate()?;
            if draft.uid.is_some() {
                draft.schema.update_from(&draft.original)?;
            }
            let request = if draft.uid.is_some() {
                Request::Save {
                    id: nucleus::new_uid("op"),
                    expected_revision: draft.revision,
                    schema: draft.schema,
                }
            } else {
                Request::Create {
                    id: nucleus::new_uid("op"),
                    schema: draft.schema,
                }
            };
            return send_change(world, owner, draft.uid, request);
        }
        Command::AddField(kind) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            let draft = view.schema.as_mut().ok_or("Open a schema")?;
            if draft.schema.fields.len() >= 64 {
                return Err("A schema can have at most 64 fields".into());
            }
            draft.schema.fields.push(new_field(*kind));
            draft.field = draft.schema.fields.len() - 1;
            draft.choice = None;
        }
        Command::Kind(kind) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            let draft = view.schema.as_mut().ok_or("Open a schema")?;
            let field = &mut draft.schema.fields[draft.field];
            if draft.uid.is_some() && draft.original.fields.iter().any(|old| old.id == field.id) {
                return Err(
                    "Saved field types stay fixed. Add a new field for another type.".into(),
                );
            }
            field.kind = *kind;
            if !kind.choices() {
                field.choices.clear();
                draft.choice = None;
            } else if field.choices.is_empty() {
                field.choices.push(new_choice());
            }
        }
        Command::Field(index) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
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
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema {
                let field = &mut draft.schema.fields[draft.field];
                field.archived = !field.archived;
            }
        }
        Command::MoveField(down) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema {
                let other = if *down {
                    draft.field + 1
                } else {
                    draft.field.saturating_sub(1)
                };
                if other < draft.schema.fields.len() {
                    draft.schema.fields.swap(draft.field, other);
                    draft.field = other;
                }
            }
        }
        Command::AddChoice => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema {
                let choices = &mut draft.schema.fields[draft.field].choices;
                if choices.len() >= 512 {
                    return Err("A field can have at most 512 choices".into());
                }
                choices.push(new_choice());
                draft.choice = Some(choices.len() - 1);
            }
        }
        Command::Choice(index) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema
                && *index < draft.schema.fields[draft.field].choices.len()
            {
                draft.choice = Some(*index);
            }
        }
        Command::ArchiveChoice => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                let choice = &mut draft.schema.fields[draft.field].choices[index];
                choice.archived = !choice.archived;
            }
        }
        Command::MoveChoice(down) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                let choices = &mut draft.schema.fields[draft.field].choices;
                let other = if *down {
                    index + 1
                } else {
                    index.saturating_sub(1)
                };
                if other < choices.len() {
                    choices.swap(index, other);
                    draft.choice = Some(other);
                }
            }
        }
        Command::AddPreset => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema
                && let Some(index) = draft.choice
            {
                let list = &mut draft.schema.fields[draft.field].choices[index].assertions;
                if list.len() >= 16 {
                    return Err("A choice can have at most 16 preset assertions".into());
                }
                list.push(Preset {
                    predicate: String::new(),
                    object: None,
                    quantity: None,
                    unit: None,
                });
            }
        }
        Command::RemovePreset(index) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            if let Some(draft) = &mut view.schema
                && let Some(choice) = draft.choice
            {
                let list = &mut draft.schema.fields[draft.field].choices[choice].assertions;
                if *index < list.len() {
                    list.remove(*index);
                }
            }
        }
        Command::Schema(uid) => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Apply or discard your draft first".into());
            }
            world.get_mut::<Settings>(owner).unwrap().schema = Some(uid.clone());
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.values = None;
            view.open = None;
            view.page = 0;
        }
        Command::Bind(schema, field) => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Apply or discard your draft first".into());
            }
            let column = world.get::<View>(owner).unwrap().column;
            if let Some(area) = column {
                let mut config =
                    crate::protein_area::configuration(world, area).ok_or("Area closed")?;
                let property = nucleus::record_extension::column(schema, field);
                if config.bindings.len() >= 32 {
                    return Err("A template can have at most 32 columns".into());
                }
                if !config
                    .bindings
                    .iter()
                    .any(|binding| binding.property == property)
                {
                    let mut binding = crate::protein_area::Binding::new(&property);
                    binding.editable = true;
                    binding.height = 180.0;
                    config.bindings.push(binding);
                    crate::protein_area::set_configuration(world, area, Some(config));
                }
                world.despawn(owner);
                return Ok(());
            }
            let mut settings = world.get_mut::<Settings>(owner).unwrap();
            settings.schema = Some(schema.clone());
            settings.field = Some(field.clone());
        }
        Command::Open(field) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.open = if view.open.as_ref() == Some(field) {
                None
            } else {
                Some(field.clone())
            };
            view.search.clear();
            view.page = 0;
        }
        Command::Search => {
            if world.get::<View>(owner).unwrap().picker.is_some() {
                return search_picker(world, owner);
            }
        }
        Command::Page(down) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.page = if *down {
                view.page.saturating_add(1).min(32)
            } else {
                view.page.saturating_sub(1)
            };
        }
        Command::Set(field, value) => {
            prepare_value(world, owner)?;
            world
                .get_mut::<View>(owner)
                .unwrap()
                .values
                .as_mut()
                .unwrap()
                .fields
                .insert(field.clone(), value.clone());
        }
        Command::Clear(field) => {
            prepare_value(world, owner)?;
            world
                .get_mut::<View>(owner)
                .unwrap()
                .values
                .as_mut()
                .unwrap()
                .fields
                .remove(field);
        }
        Command::Select(field, choice) => {
            prepare_value(world, owner)?;
            let schema = world
                .get::<Settings>(owner)
                .unwrap()
                .schema
                .clone()
                .ok_or("Choose a schema")?;
            let definition = inspection(world, owner)
                .and_then(|data| data.schemas.iter().find(|entry| entry.uid == schema))
                .and_then(|entry| entry.schema.fields.iter().find(|entry| entry.id == *field))
                .ok_or("Field unavailable")?
                .clone();
            let mut view = world.get_mut::<View>(owner).unwrap();
            let draft = view.values.as_mut().unwrap();
            let mut selected = definition
                .selection(draft.fields.get(field).unwrap_or(&Value::Null))?
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if definition.kind == FieldKind::Select {
                draft.fields.insert(field.clone(), json!(choice));
                view.open = None;
                view.focus = Some((field.clone(), None));
            } else {
                if let Some(index) = selected.iter().position(|id| id == choice) {
                    selected.remove(index);
                } else {
                    selected.push(choice.clone());
                }
                draft.fields.insert(field.clone(), json!(selected));
                view.focus = Some((field.clone(), Some(choice.clone())));
            }
        }
        Command::Apply | Command::Detach => {
            prepare_value(world, owner)?;
            let settings = world.get::<Settings>(owner).unwrap().clone();
            let draft = world
                .get::<View>(owner)
                .unwrap()
                .values
                .as_ref()
                .unwrap()
                .clone();
            let remove = matches!(command, Command::Detach);
            let values = if remove {
                BTreeMap::new()
            } else {
                changes(&draft)
            };
            return send_change(
                world,
                owner,
                settings.record,
                Request::Apply {
                    id: nucleus::new_uid("op"),
                    schema: draft.schema,
                    expected_revision: draft.revision,
                    expected_schema_revision: draft.schema_revision,
                    values,
                    remove,
                },
            );
        }
        Command::Undo => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Apply or discard your draft before undoing".into());
            }
            let undo = world
                .get::<View>(owner)
                .unwrap()
                .undo
                .clone()
                .ok_or("Nothing to undo")?;
            let target = world.get::<Settings>(owner).unwrap().record.clone();
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
                        undo.fields
                    } else {
                        BTreeMap::new()
                    },
                    remove: !undo.attached,
                },
            );
        }
        Command::Reload => {
            if world.get::<View>(owner).unwrap().dirty() {
                return Err("Discard your draft before reloading".into());
            }
            let feed = world.get::<View>(owner).unwrap().feed.clone();
            if let Some(feed) = world.resource_mut::<Runtime>().feeds.get_mut(&feed) {
                feed.next = Instant::now();
            }
            world.get_mut::<View>(owner).unwrap().values = None;
        }
        Command::Retry => {
            let (target, request) = world
                .get::<View>(owner)
                .unwrap()
                .retry
                .clone()
                .ok_or("Nothing to retry")?;
            return send_change(world, owner, target, request);
        }
        Command::Column(area) => {
            world.get_mut::<View>(owner).unwrap().column = Some(*area);
            world.get_mut::<Settings>(owner).unwrap().record = None;
        }
    }
    Ok(())
}

fn prepare_value(world: &mut World, owner: Entity) -> Result<(), String> {
    let schema = world
        .get::<Settings>(owner)
        .unwrap()
        .schema
        .clone()
        .ok_or("Choose a schema")?;
    if !world.get::<View>(owner).unwrap().editable
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
    if matches!(message,ServerMessage::Error{id,code,..} if id==crate::cell_bridge::CONNECTION||id=="connection"||code.as_deref()==Some("session_expired"))
    {
        for feed in world
            .resource_mut::<Runtime>()
            .feeds
            .values_mut()
            .filter(|feed| &feed.source == source)
        {
            feed.data = None;
            feed.error = "Access unavailable; reconnect to reload".into();
            feed.version += 1;
            feed.next = Instant::now() + Duration::from_secs(5);
        }
    }
    let (id, result) = match message {
        ServerMessage::Snapshot { id, rows } => (id, Ok(json!(rows))),
        ServerMessage::ActionOk { id, data, .. } => (id, Ok(data.clone().unwrap_or(Value::Null))),
        ServerMessage::Error { id, message, .. } => (id, Err(message.clone())),
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
    let pending = world.resource_mut::<Runtime>().pending.remove(id).unwrap();
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
                match result {
                    Ok(value) => {
                        if let Some(picker) = &mut view.picker {
                            picker.rows = value.as_array().cloned().unwrap_or_default();
                        }
                    }
                    Err(error) => view.message = error,
                }
                view.render = true;
            }
        }
        PendingKind::Inspect(key) => {
            let result = result.and_then(|value| {
                serde_json::from_value::<Inspection>(value).map_err(|error| error.to_string())
            });
            if let Some(feed) = world.resource_mut::<Runtime>().feeds.get_mut(&key) {
                feed.pending = None;
                feed.next = Instant::now() + Duration::from_secs(3);
                match result {
                    Ok(data) => {
                        let changed = feed.data.as_ref() != Some(&data);
                        if changed || !feed.error.is_empty() {
                            feed.version += 1;
                        }
                        feed.data = Some(data);
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
            if !world.entities().contains(owner) {
                return;
            }
            let retry = world.get::<View>(owner).unwrap().retry.clone();
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending = None;
            view.render = true;
            match result {
                Err(error) => view.message = error,
                Ok(data) => {
                    if let Some((_, request)) = &retry {
                        match request {
                            Request::Create { .. } | Request::Save { .. } => {
                                if let Some(uid) = data["schema"].as_str() {
                                    let mut settings = world.get_mut::<Settings>(owner).unwrap();
                                    settings.schema = Some(uid.into());
                                }
                                let mut view = world.get_mut::<View>(owner).unwrap();
                                view.schema = None;
                                view.values = None;
                                view.message =
                                    "Schema saved. Labels changed without changing selection IDs."
                                        .into();
                                view.retry = None;
                            }
                            Request::Apply { schema, .. } => {
                                let revision = data["revision"].as_u64().unwrap_or(0);
                                if let Some(draft) = &view.values {
                                    let mut fields = draft.original.clone();
                                    for key in draft.fields.keys() {
                                        fields.entry(key.clone()).or_insert(Value::Null);
                                    }
                                    view.undo = Some(Undo {
                                        schema: schema.clone(),
                                        schema_revision: draft.schema_revision,
                                        fields,
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
                            _ => {}
                        }
                    }
                    for feed in world
                        .resource_mut::<Runtime>()
                        .feeds
                        .values_mut()
                        .filter(|feed| &feed.source == source)
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
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &Source::Local, &message);
    }
    ui::keyboard(world);
    let owners = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect::<Vec<_>>();
    for owner in &owners {
        capture(world, *owner);
    }
    let now = Instant::now();
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    let mut active = std::collections::HashSet::new();
    for owner in &owners {
        let settings = world.get::<Settings>(*owner).unwrap().clone();
        let key = key(&settings);
        active.insert(key.clone());
        if runtime.feeds.len() < 256 || runtime.feeds.contains_key(&key) {
            runtime.feeds.entry(key.clone()).or_insert_with(|| Feed {
                schemas: vec![],
                source: settings.source.clone(),
                target: settings.record.clone(),
                catalog: settings.mode != Mode::Column,
                data: None,
                error: String::new(),
                version: 1,
                next: now,
                pending: None,
            });
            let mut view = world.get_mut::<View>(*owner).unwrap();
            if view.feed != key {
                view.feed = key;
                view.seen = 0;
                view.render = true;
            }
        }
    }
    runtime.feeds.retain(|key, _| active.contains(key));
    for (key, feed) in &mut runtime.feeds {
        let schemas = owners
            .iter()
            .filter_map(|owner| {
                let settings = world.get::<Settings>(*owner)?;
                (self::key(settings) == *key)
                    .then_some(settings.schema.clone())
                    .flatten()
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if schemas != feed.schemas {
            feed.schemas = schemas;
            feed.next = now;
        }
    }
    let expired = runtime
        .pending
        .iter()
        .filter(|(_, pending)| now.duration_since(pending.started) > Duration::from_secs(20))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for id in expired {
        if let Some(pending) = runtime.pending.remove(&id) {
            match pending.kind {
                PendingKind::Picker(owner) => {
                    if let Some(sender) = sender(world, &pending.source) {
                        let _ = sender.try_send(ClientMessage::Unsubscribe { id: id.clone() });
                    }
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        if !view
                            .picker
                            .as_ref()
                            .is_some_and(|picker| picker.request.as_ref() == Some(&id))
                        {
                            continue;
                        }
                        view.message = "Picker timed out. Search again.".into();
                        view.render = true;
                    }
                }
                PendingKind::Inspect(key) => {
                    if let Some(feed) = runtime.feeds.get_mut(&key) {
                        feed.pending = None;
                        feed.data = None;
                        feed.error = "Loading timed out; retrying".into();
                        feed.version += 1;
                        feed.next = now + Duration::from_secs(3);
                    }
                }
                PendingKind::Change(owner) => {
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        view.pending = None;
                        view.message="No confirmation received. Retry this same change to confirm its result.".into();
                        view.render = true;
                    }
                }
            }
        }
    }
    if !crate::laboratory::active(world) {
        for (key, feed) in &mut runtime.feeds {
            if feed.pending.is_some() || feed.next > now || runtime.pending.len() >= 128 {
                continue;
            }
            if let Some(sender) = sender(world, &feed.source) {
                let id = nucleus::new_uid("extension");
                if sender
                    .try_send(ClientMessage::Act {
                        id: id.clone(),
                        action: engine::actions::Action::RecordExtensions {
                            target: feed.target.clone(),
                            request: Request::Inspect {
                                catalog: feed.catalog,
                                schemas: feed.schemas.clone(),
                            },
                        },
                    })
                    .is_ok()
                {
                    feed.pending = Some(id.clone());
                    runtime.pending.insert(
                        id,
                        Pending {
                            source: feed.source.clone(),
                            kind: PendingKind::Inspect(key.clone()),
                            started: now,
                        },
                    );
                }
            } else if feed.data.is_some() || feed.error.is_empty() {
                feed.data = None;
                feed.error = "Connect to the Organ to load extensions".into();
                feed.version += 1;
            }
            feed.next = now + Duration::from_secs(3);
        }
    }
    if !active.is_empty() && runtime.wake.is_none_or(|deadline| deadline <= now) {
        let deadline = now + Duration::from_secs(3);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.after(Duration::from_secs(3));
        }
        runtime.wake = Some(deadline);
    }
    world.insert_resource(runtime);
    for owner in owners {
        let (key, seen, dirty) = {
            let view = world.get::<View>(owner).unwrap();
            (view.feed.clone(), view.seen, view.dirty())
        };
        let state = world
            .resource::<Runtime>()
            .feeds
            .get(&key)
            .map(|feed| (feed.version, feed.data.clone(), feed.error.clone()));
        if let Some((version, data, error)) = state
            && version != seen
        {
            if let Some(uid) = data.as_ref().and_then(|data| data.record.clone()) {
                world.get_mut::<Settings>(owner).unwrap().record = Some(uid);
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.seen = version;
            if data.is_none() {
                view.values = None;
                view.schema = None;
                view.reference_labels.clear();
                view.undo = None;
                view.open = None;
                view.message = error;
                view.render = true;
            } else if !dirty && view.pending.is_none() {
                view.values = None;
                view.render = true;
            }
        }
        if world.get::<View>(owner).unwrap().render {
            ui::render(world, owner);
            world.get_mut::<View>(owner).unwrap().render = false;
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
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            crate::workspace::WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(660.0, 640.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            settings,
            View {
                editable: true,
                render: true,
                ..default()
            },
        ))
        .id();
    crate::scroll_sand::attach(world, owner);
    ui::render(world, owner);
    owner
}

#[derive(Clone)]
pub(crate) struct Open(pub RecordBinding);
impl Action for Open {
    fn apply(&self, world: &mut World, parent: Entity) {
        let mut root = parent;
        while world.get::<crate::workspace::Workspaces>(root).is_none() {
            let Some(parent) = world.get::<ChildOf>(root) else {
                return;
            };
            root = parent.parent();
        }
        let workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let position = world
            .get::<crate::canvas::CanvasView>(root)
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
        let mut root = parent;
        while world.get::<crate::workspace::Workspaces>(root).is_none() {
            let Some(parent) = world.get::<ChildOf>(root) else {
                return;
            };
            root = parent.parent();
        }
        let Some(config) = crate::protein_area::configuration(world, self.0) else {
            return;
        };
        let workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let position = world
            .get::<crate::canvas::CanvasView>(root)
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
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return;
        };
        let position = world
            .get::<crate::canvas::CanvasView>(root)
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
        crate::edit_mode::EditAction::Close.apply(world, root);
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
        let entry = crate::description::button(
            world,
            parent,
            root,
            &format!("{title} · {description}"),
            Add(mode),
        );
        world
            .entity_mut(entry)
            .insert(crate::sand_store::StoreComponent {
                title: title.into(),
                description: description.into(),
                size: Vec2::new(660.0, 640.0),
            });
    }
}
