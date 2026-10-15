use std::collections::BTreeMap;

use bevy::{prelude::*, text::EditableText};
use engine::record_change::Mutation;
use serde::{Deserialize, Serialize};
use simulation::{
    proposal::{Change, Edit, Record},
    scenario::Scenario,
};

use super::{SimulationCastle, View};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Field {
    #[default]
    Title,
    Description,
    Quantity,
    Schedule,
    Slug,
    Relation,
}

impl Field {
    fn key(self) -> &'static str {
        match self {
            Self::Title => "head",
            Self::Description => "body",
            Self::Quantity => "quantity",
            Self::Schedule => "work",
            Self::Slug => "slug",
            Self::Relation => "assertions",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Form {
    pub record: String,
    pub field: Field,
    selected: Option<String>,
    editing: Option<Editing>,
    value: String,
    start: String,
    due: String,
    estimate: String,
    predicate: String,
    object: String,
    relation_quantity: String,
    unit: String,
    after: String,
    horizon: String,
    open: bool,
    expanded: bool,
    drafts: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Editing {
    original: Change,
    edit: Edit,
    shown: BTreeMap<String, String>,
}

impl Editing {
    fn update(
        &mut self,
        field: Field,
        values: &BTreeMap<String, String>,
    ) -> simulation::Result<()> {
        let changed = |key: &str| values.get(key) != self.shown.get(key);
        let optional =
            |key: &str| (!values[key].trim().is_empty()).then(|| values[key].trim().to_owned());
        match (field, &mut self.edit) {
            (Field::Title, Edit::Text { head, .. }) if changed("value") => {
                *head = Some(values["value"].clone())
            }
            (Field::Description, Edit::Text { body, .. }) if changed("value") => {
                *body = Some(values["value"].clone())
            }
            (
                Field::Quantity,
                Edit::Property {
                    mutation: Mutation::Quantity { value },
                },
            ) if changed("value") => {
                *value = nucleus::DecimalValue::parse_inferred(values["value"].trim())?.to_string()
            }
            (
                Field::Slug,
                Edit::Property {
                    mutation: Mutation::Slug { value },
                },
            ) if changed("value") => *value = optional("value"),
            (
                Field::Relation,
                Edit::Property {
                    mutation:
                        Mutation::Assertion {
                            predicate,
                            object,
                            quantity,
                            unit,
                        },
                },
            ) => {
                if changed("predicate") {
                    *predicate = values["predicate"].trim().into();
                }
                if changed("object") {
                    *object = optional("object");
                }
                if changed("relation_quantity") {
                    *quantity = optional("relation_quantity");
                }
                if changed("unit") {
                    *unit = optional("unit");
                }
            }
            (Field::Schedule, Edit::Work { value }) => {
                let work = value
                    .as_object_mut()
                    .ok_or("Authored Work metadata is invalid")?;
                for key in ["start", "due"] {
                    if changed(key) {
                        if let Some(value) = optional(key) {
                            work.insert(key.into(), value.into());
                        } else {
                            work.remove(key);
                        }
                    }
                }
                if changed("estimate") {
                    if let Some(estimate) = optional("estimate") {
                        let estimate: f64 = estimate.parse()?;
                        if !estimate.is_finite() || estimate < 0.0 {
                            return Err("Choose a finite nonnegative estimate".into());
                        }
                        work.insert("estimate_min".into(), serde_json::json!(estimate));
                    } else {
                        work.remove("estimate_min");
                    }
                }
            }
            _ => {}
        }
        self.shown = values.clone();
        Ok(())
    }
}

impl Default for Form {
    fn default() -> Self {
        Self {
            record: String::new(),
            field: Field::Title,
            selected: None,
            editing: None,
            value: String::new(),
            start: String::new(),
            due: String::new(),
            estimate: String::new(),
            predicate: "related".into(),
            object: String::new(),
            relation_quantity: String::new(),
            unit: String::new(),
            after: "0".into(),
            horizon: "720".into(),
            open: false,
            expanded: false,
            drafts: BTreeMap::new(),
        }
    }
}

impl Form {
    pub(super) fn target(record: &str) -> Self {
        Self {
            record: record.into(),
            open: !record.is_empty(),
            ..Default::default()
        }
    }

    pub(super) fn valid(&self) -> bool {
        self.record.len() <= 256
            && self.value.len() <= 262_144
            && [
                &self.start,
                &self.due,
                &self.estimate,
                &self.predicate,
                &self.object,
                &self.relation_quantity,
                &self.unit,
                &self.after,
                &self.horizon,
            ]
            .iter()
            .all(|value| value.len() <= 256)
            && self.drafts.len() <= 128
            && self.drafts.keys().all(|key| key.len() <= 256)
            && serde_json::to_vec(&self.drafts).is_ok_and(|bytes| bytes.len() <= 1024 * 1024)
            && serde_json::to_vec(&self.editing).is_ok_and(|bytes| bytes.len() <= 1024 * 1024)
    }

    fn values(&self) -> BTreeMap<String, String> {
        [
            ("value", &self.value),
            ("start", &self.start),
            ("due", &self.due),
            ("estimate", &self.estimate),
            ("predicate", &self.predicate),
            ("object", &self.object),
            ("relation_quantity", &self.relation_quantity),
            ("unit", &self.unit),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.clone()))
        .collect()
    }

    fn update_selected(&mut self) -> simulation::Result<()> {
        let values = self.values();
        if let Some(editing) = &mut self.editing {
            editing.update(self.field, &values)?;
        }
        Ok(())
    }

    fn show_selected(&mut self, record: &Record, field: Field) -> simulation::Result<()> {
        let edit = self
            .editing
            .as_ref()
            .ok_or("Select this change again")?
            .edit
            .clone();
        self.field = field;
        self.fill(record);
        match (field, edit) {
            (Field::Title, Edit::Text {head,..}) => self.value = head.unwrap_or_else(|| record.head.clone()),
            (Field::Description, Edit::Text {body,..}) => self.value = body.unwrap_or_else(|| record.body.clone()),
            (Field::Quantity, Edit::Property {mutation:Mutation::Quantity {value}}) => self.value = value,
            (Field::Slug, Edit::Property {mutation:Mutation::Slug {value}}) => self.value = value.unwrap_or_default(),
            (Field::Relation, Edit::Property {mutation:Mutation::Assertion {predicate,object,quantity,unit}}) => {
                self.predicate = predicate; self.object = object.unwrap_or_default(); self.relation_quantity = quantity.unwrap_or_default(); self.unit = unit.unwrap_or_default();
            }
            (Field::Schedule, Edit::Work {value}) => {
                self.start = value["start"].as_str().unwrap_or_default().into();
                self.due = value["due"].as_str().unwrap_or_default().into();
                self.estimate = value["estimate_min"].as_f64().map(|value| value.to_string()).unwrap_or_default();
            }
            _ => return Err("Edit this mutation through JSON or create a new change before changing its operation".into()),
        }
        let shown = self.values();
        self.editing.as_mut().unwrap().shown = shown;
        Ok(())
    }

    fn remember(&mut self) {
        for (key, value) in [
            (self.field.key(), &self.value),
            ("start", &self.start),
            ("due", &self.due),
            ("estimate_min", &self.estimate),
            ("predicate", &self.predicate),
            ("object", &self.object),
            ("relation_quantity", &self.relation_quantity),
            ("unit", &self.unit),
        ] {
            self.drafts
                .insert(format!("{}:{key}", self.record), value.clone());
        }
    }

    fn fill(&mut self, record: &Record) {
        self.record = record.uid.clone();
        let key = format!("{}:{}", self.record, self.field.key());
        self.value = self
            .drafts
            .get(&key)
            .cloned()
            .unwrap_or_else(|| match self.field {
                Field::Title => record.head.clone(),
                Field::Description => record.body.clone(),
                Field::Quantity => record.quantity.to_string(),
                Field::Slug => record.slug.clone().unwrap_or_default(),
                _ => String::new(),
            });
        self.start = self
            .drafts
            .get(&format!("{}:start", self.record))
            .cloned()
            .unwrap_or_else(|| record.work["start"].as_str().unwrap_or_default().into());
        self.due = self
            .drafts
            .get(&format!("{}:due", self.record))
            .cloned()
            .unwrap_or_else(|| record.work["due"].as_str().unwrap_or_default().into());
        self.estimate = self
            .drafts
            .get(&format!("{}:estimate_min", self.record))
            .cloned()
            .unwrap_or_else(|| {
                record.work["estimate_min"]
                    .as_f64()
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            });
        for (key, value, fallback) in [
            ("predicate", &mut self.predicate, "related"),
            ("object", &mut self.object, ""),
            ("relation_quantity", &mut self.relation_quantity, ""),
            ("unit", &mut self.unit, ""),
        ] {
            *value = self
                .drafts
                .get(&format!("{}:{key}", self.record))
                .cloned()
                .unwrap_or_else(|| fallback.into());
        }
    }
}

#[derive(Clone, Copy)]
enum InputField {
    Record,
    Value,
    Start,
    Due,
    Estimate,
    Predicate,
    Object,
    RelationQuantity,
    Unit,
    After,
    Horizon,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    field: InputField,
}

#[derive(Component)]
struct SourceStatus(Entity);

#[derive(Component)]
struct LiveWatch {
    stamp: String,
    updates: tokio::sync::watch::Receiver<String>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for LiveWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn watch_source(world: &mut World, owner: Entity, proposal: &simulation::proposal::Proposal) {
    let Ok(stamp) = simulation::world::digest(&(&proposal.source, &proposal.records)) else {
        return;
    };
    if world
        .get::<LiveWatch>(owner)
        .is_some_and(|watch| watch.stamp == stamp.as_str() && !watch.task.is_finished())
    {
        return;
    }
    let Some(runtime) = crate::native::practice_cells::runtime(world, owner) else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let selected: Vec<_> = proposal
        .records
        .iter()
        .map(|record| (record.uid.clone(), record.revision))
        .collect();
    let mut changed = runtime.engine.watch_query_changes();
    let mut facts = runtime.engine.subscribe();
    let (status, updates) =
        tokio::sync::watch::channel("Checking captured Records against the live Cell".to_string());
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let task = handle.spawn(async move {
        let mut source_dirty = false;
        loop {
            let mut message = "Captured Records unchanged in live Cell".to_string();
            for (uid, revision) in &selected {
                let current = store::sqlx::query_as::<_, (i64, bool)>("SELECT rr.revision, r.deleted_at IS NOT NULL FROM record r JOIN record_revision rr ON rr.record_uid = r.uid WHERE r.uid = ?")
                    .bind(uid).fetch_optional(&runtime.store.pool).await;
                match current {
                    Ok(Some((current, false))) if current == *revision => {}
                    Ok(_) => { message = "Captured Records changed in live Cell · using the retained base".into(); break; }
                    Err(_) => { message = "Live Cell unavailable · using the retained base".into(); break; }
                }
            }
            if source_dirty { message.push_str(" · live source may have changed; inspect to check its fingerprint"); }
            status.send_replace(message);
            if let Some(wake) = &wake { wake.ring(); }
            let available = tokio::select! {
                result = changed.changed() => result.is_ok(),
                result = facts.recv() => !matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)),
            };
            if !available { break; }
            source_dirty = true;
        }
    });
    world.entity_mut(owner).insert(LiveWatch {
        stamp: stamp.as_str().into(),
        updates,
        task,
    });
}

pub(super) fn receive(world: &mut World) {
    let updates: Vec<_> = world
        .query::<(Entity, &mut LiveWatch)>()
        .iter_mut(world)
        .filter_map(|(owner, mut watch)| {
            watch
                .updates
                .has_changed()
                .ok()
                .filter(|changed| *changed)
                .map(|_| (owner, watch.updates.borrow_and_update().clone()))
        })
        .collect();
    for (owner, message) in updates {
        let labels: Vec<_> = world
            .query::<(Entity, &SourceStatus)>()
            .iter(world)
            .filter(|(_, status)| status.0 == owner)
            .map(|(entity, _)| entity)
            .collect();
        for label in labels {
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = message.clone();
            }
        }
    }
}

#[derive(Clone)]
pub(super) enum Command {
    Toggle,
    Capture(bool),
    Field(Field),
    Record(String),
    Select(String),
    Apply,
    New,
    Remove(String),
    Move(String, bool),
    Horizon,
    Original,
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) {
    crate::native::castle_feed::button(
        world,
        parent,
        owner,
        label,
        super::Command::Records(command),
    );
}

fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    label: &str,
    field: InputField,
    value: &str,
    multiline: bool,
) {
    let row = super::row(world, parent);
    crate::native::edit_mode::label(world, row, label, 13.0);
    let editor = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .insert((
            ChildOf(row),
            Input { owner, field },
            Node {
                width: px(460),
                min_width: px(0),
                flex_shrink: 1.0,
                ..default()
            },
        ))
        .id();
    let mut text = world.get_mut::<EditableText>(editor).unwrap();
    text.allow_newlines = multiline;
    text.visible_lines = Some(if multiline { 4.0 } else { 1.0 });
    text.max_characters = Some(if multiline { 262_144 } else { 65_536 });
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let panel = world.get::<View>(owner).unwrap().records;
    world.entity_mut(panel).despawn_children();
    let model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let form = &model.record_form;
    button(
        world,
        panel,
        owner,
        if form.open {
            "Hide Record changes"
        } else {
            "Record changes"
        },
        Command::Toggle,
    );
    if !form.open {
        world.entity_mut(owner).remove::<LiveWatch>();
        return;
    }
    let Ok(scenario) = serde_json::from_str::<Scenario>(&model.scenario) else {
        world.entity_mut(owner).remove::<LiveWatch>();
        return;
    };
    input(
        world,
        panel,
        owner,
        "Record UID or slug",
        InputField::Record,
        &form.record,
        false,
    );
    let row = super::row(world, panel);
    button(
        world,
        row,
        owner,
        "Capture Record from base",
        Command::Capture(false),
    );
    button(
        world,
        row,
        owner,
        "Capture fresh current base",
        Command::Capture(true),
    );
    let Some(proposal) = &scenario.proposal else {
        world.entity_mut(owner).remove::<LiveWatch>();
        crate::native::edit_mode::label(
            world,
            panel,
            "Capture a base to try edits without saving them to your live Records.",
            13.0,
        );
        return;
    };
    watch_source(world, owner, proposal);
    let source_status = world
        .get::<LiveWatch>(owner)
        .map(|watch| watch.updates.borrow().clone())
        .unwrap_or_else(|| "Retained base · live Cell unavailable".into());
    let label = crate::native::edit_mode::label(world, panel, &source_status, 13.0);
    world.entity_mut(label).insert(SourceStatus(owner));
    crate::native::edit_mode::label(
        world,
        panel,
        &format!(
            "Captured base · {}",
            chrono::DateTime::from_timestamp_millis(proposal.base_ms)
                .map(|time| time.to_rfc3339())
                .unwrap_or_default()
        ),
        13.0,
    );
    let row = super::row(world, panel);
    for record in &proposal.records {
        button(
            world,
            row,
            owner,
            &record.head.chars().take(40).collect::<String>(),
            Command::Record(record.uid.clone()),
        );
    }
    let row = super::row(world, panel);
    for (label, field) in [
        ("Title", Field::Title),
        ("Description", Field::Description),
        ("Quantity", Field::Quantity),
        ("Appointment", Field::Schedule),
        ("Slug", Field::Slug),
        ("Relation", Field::Relation),
    ] {
        button(world, row, owner, label, Command::Field(field));
    }
    if let Some(record) = proposal.records.iter().find(|record| {
        record.uid == form.record || record.slug.as_deref() == Some(form.record.as_str())
    }) {
        let original = match form.field {
            Field::Title => record.head.clone(),
            Field::Description => record.body.clone(),
            Field::Quantity => record.quantity.to_string(),
            Field::Slug => record.slug.clone().unwrap_or_default(),
            Field::Schedule => serde_json::to_string_pretty(&record.work).unwrap(),
            Field::Relation => serde_json::to_string_pretty(&record.assertions).unwrap(),
        };
        let preview = if form.expanded {
            original.clone()
        } else {
            original.chars().take(512).collect::<String>()
        };
        crate::native::edit_mode::label(world, panel, &format!("Captured value\n{preview}"), 13.0);
        if original.chars().count() > 512 {
            button(
                world,
                panel,
                owner,
                if form.expanded {
                    "Collapse captured value"
                } else {
                    "Show full captured value"
                },
                Command::Original,
            );
        }
    }
    match form.field {
        Field::Schedule => {
            input(
                world,
                panel,
                owner,
                "Proposed start (date or timestamp; empty clears)",
                InputField::Start,
                &form.start,
                false,
            );
            input(
                world,
                panel,
                owner,
                "Proposed due (date or timestamp; empty clears)",
                InputField::Due,
                &form.due,
                false,
            );
            input(
                world,
                panel,
                owner,
                "Estimated minutes (empty clears)",
                InputField::Estimate,
                &form.estimate,
                false,
            );
        }
        Field::Relation => {
            for (label, field, value) in [
                ("Predicate", InputField::Predicate, &form.predicate),
                (
                    "Object Record UID or slug (optional)",
                    InputField::Object,
                    &form.object,
                ),
                (
                    "Exact relation quantity (optional)",
                    InputField::RelationQuantity,
                    &form.relation_quantity,
                ),
                ("Unit (optional)", InputField::Unit, &form.unit),
            ] {
                input(world, panel, owner, label, field, value, false);
            }
        }
        _ => input(
            world,
            panel,
            owner,
            "Proposed value",
            InputField::Value,
            &form.value,
            form.field == Field::Description,
        ),
    }
    input(
        world,
        panel,
        owner,
        "Apply in this simulation after hours",
        InputField::After,
        &form.after,
        false,
    );
    let row = super::row(world, panel);
    button(
        world,
        row,
        owner,
        if form.selected.is_some() {
            "Update proposed change"
        } else {
            "Add proposed change"
        },
        Command::Apply,
    );
    button(world, row, owner, "New change", Command::New);
    for change in &proposal.changes {
        let row = super::row(world, panel);
        button(
            world,
            row,
            owner,
            &format!(
                "{} · {} · {} hours",
                change.id,
                change.record,
                change.after_ms as f64 / 3_600_000.0
            ),
            Command::Select(change.id.clone()),
        );
        button(
            world,
            row,
            owner,
            "Remove",
            Command::Remove(change.id.clone()),
        );
        button(
            world,
            row,
            owner,
            "Move earlier",
            Command::Move(change.id.clone(), false),
        );
        button(
            world,
            row,
            owner,
            "Move later",
            Command::Move(change.id.clone(), true),
        );
    }
    input(
        world,
        panel,
        owner,
        "Simulation window in hours",
        InputField::Horizon,
        &form.horizon,
        false,
    );
    button(
        world,
        panel,
        owner,
        "Set simulation window",
        Command::Horizon,
    );
}

pub(super) fn capture(world: &mut World) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.owner, input.field, text.value().to_string()))
        .collect();
    for (owner, field, value) in values {
        let Some(mut model) = world.get_mut::<SimulationCastle>(owner) else {
            continue;
        };
        let form = &mut model.record_form;
        *match field {
            InputField::Record => &mut form.record,
            InputField::Value => &mut form.value,
            InputField::Start => &mut form.start,
            InputField::Due => &mut form.due,
            InputField::Estimate => &mut form.estimate,
            InputField::Predicate => &mut form.predicate,
            InputField::Object => &mut form.object,
            InputField::RelationQuantity => &mut form.relation_quantity,
            InputField::Unit => &mut form.unit,
            InputField::After => &mut form.after,
            InputField::Horizon => &mut form.horizon,
        } = value;
    }
}

fn milliseconds(value: &str) -> simulation::Result<u64> {
    let hours: f64 = value.trim().parse()?;
    let millis = hours * 3_600_000.0;
    if !millis.is_finite() || millis < 0.0 || millis > i64::MAX as f64 {
        return Err("Choose a finite nonnegative number of hours".into());
    }
    Ok(millis.round() as u64)
}

pub(super) fn apply(command: &Command, world: &mut World, owner: Entity) -> simulation::Result<()> {
    let mut model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let mut scenario: Scenario = serde_json::from_str(&model.scenario)?;
    let form = &mut model.record_form;
    match command {
        Command::Toggle => form.open = !form.open,
        Command::Original => form.expanded = !form.expanded,
        Command::Capture(_) => return Err("Capture requires the current Cell".into()),
        Command::Field(field) => {
            form.remember();
            form.update_selected()?;
            if let Some(record) = scenario.proposal.as_ref().and_then(|proposal| {
                proposal
                    .records
                    .iter()
                    .find(|record| record.uid == form.record)
            }) {
                if form.editing.is_some() {
                    form.show_selected(record, *field)?;
                } else {
                    form.field = *field;
                    form.fill(record);
                }
            } else {
                form.field = *field;
            }
        }
        Command::Record(uid) => {
            form.remember();
            form.selected = None;
            form.editing = None;
            let record = scenario
                .proposal
                .as_ref()
                .and_then(|proposal| proposal.records.iter().find(|record| &record.uid == uid))
                .ok_or("Captured Record is missing")?;
            form.fill(record);
        }
        Command::New => {
            form.selected = None;
            form.editing = None;
            form.after = "0".into();
        }
        Command::Remove(id) => {
            scenario
                .proposal
                .as_mut()
                .ok_or("Capture a base first")?
                .changes
                .retain(|change| &change.id != id);
            if form.selected.as_ref() == Some(id) {
                form.selected = None;
                form.editing = None;
            }
        }
        Command::Move(id, forward) => {
            let changes = &mut scenario
                .proposal
                .as_mut()
                .ok_or("Capture a base first")?
                .changes;
            let index = changes
                .iter()
                .position(|change| &change.id == id)
                .ok_or("Change is missing")?;
            let next = if *forward {
                index.checked_add(1).filter(|next| *next < changes.len())
            } else {
                index.checked_sub(1)
            };
            if let Some(next) = next {
                let time = changes[index].after_ms;
                changes[index].after_ms = changes[next].after_ms;
                changes[next].after_ms = time;
                changes.swap(index, next);
            }
        }
        Command::Select(id) => {
            form.remember();
            let proposal = scenario.proposal.as_ref().ok_or("Capture a base first")?;
            let change = proposal
                .changes
                .iter()
                .find(|change| &change.id == id)
                .ok_or("Change is missing")?;
            form.fill(
                proposal
                    .records
                    .iter()
                    .find(|record| record.uid == change.record)
                    .ok_or("Captured Record is missing")?,
            );
            form.selected = Some(id.clone());
            form.after = (change.after_ms as f64 / 3_600_000.0).to_string();
            form.editing = Some(Editing {
                original: change.clone(),
                edit: change.edit.clone(),
                shown: BTreeMap::new(),
            });
            match &change.edit {
                Edit::Text { head, body } => {
                    form.field = if body.is_some() {
                        Field::Description
                    } else {
                        Field::Title
                    };
                    form.value = body.as_ref().or(head.as_ref()).cloned().unwrap_or_default();
                }
                Edit::Property {
                    mutation: Mutation::Quantity { value },
                } => {
                    form.field = Field::Quantity;
                    form.value = value.clone();
                }
                Edit::Property {
                    mutation: Mutation::Slug { value },
                } => {
                    form.field = Field::Slug;
                    form.value = value.clone().unwrap_or_default();
                }
                Edit::Property {
                    mutation:
                        Mutation::Assertion {
                            predicate,
                            object,
                            quantity,
                            unit,
                        },
                } => {
                    form.field = Field::Relation;
                    form.predicate = predicate.clone();
                    form.object = object.clone().unwrap_or_default();
                    form.relation_quantity = quantity.clone().unwrap_or_default();
                    form.unit = unit.clone().unwrap_or_default();
                }
                Edit::Work { value } => {
                    form.field = Field::Schedule;
                    form.start = value["start"].as_str().unwrap_or_default().into();
                    form.due = value["due"].as_str().unwrap_or_default().into();
                    form.estimate = value["estimate_min"]
                        .as_f64()
                        .map(|value| value.to_string())
                        .unwrap_or_default();
                }
                _ => return Err("Edit this advanced property through the scenario JSON".into()),
            }
            let field = form.field;
            let record = proposal
                .records
                .iter()
                .find(|record| record.uid == change.record)
                .unwrap();
            form.show_selected(record, field)?;
        }
        Command::Horizon => {
            scenario.end_ms = scenario
                .start_ms
                .checked_add(i64::try_from(milliseconds(&form.horizon)?)?)
                .ok_or("Window overflow")?;
        }
        Command::Apply => {
            let proposal = scenario.proposal.as_mut().ok_or("Capture a base first")?;
            let record = proposal
                .records
                .iter()
                .find(|record| {
                    record.uid == form.record
                        || record.slug.as_deref() == Some(form.record.as_str())
                })
                .ok_or("Capture this Record before adding a change")?;
            let optional =
                |value: &str| (!value.trim().is_empty()).then(|| value.trim().to_owned());
            let edit = if let Some(editing) = &form.editing {
                let existing = proposal
                    .changes
                    .iter()
                    .find(|change| change.id == editing.original.id)
                    .ok_or("Authored change was removed; select another change")?;
                if existing.record != record.uid
                    || form.selected.as_deref() != Some(existing.id.as_str())
                    || serde_json::to_value(&existing.edit)?
                        != serde_json::to_value(&editing.original.edit)?
                {
                    return Err(
                        "Authored change was modified; select it again before updating".into(),
                    );
                }
                form.update_selected()?;
                form.editing.as_ref().unwrap().edit.clone()
            } else {
                match form.field {
                    Field::Title => Edit::Text {
                        head: Some(form.value.clone()),
                        body: None,
                    },
                    Field::Description => Edit::Text {
                        head: None,
                        body: Some(form.value.clone()),
                    },
                    Field::Quantity => Edit::Property {
                        mutation: Mutation::Quantity {
                            value: nucleus::DecimalValue::parse_inferred(form.value.trim())?
                                .to_string(),
                        },
                    },
                    Field::Slug => Edit::Property {
                        mutation: Mutation::Slug {
                            value: optional(&form.value),
                        },
                    },
                    Field::Relation => Edit::Property {
                        mutation: Mutation::Assertion {
                            predicate: form.predicate.trim().into(),
                            object: optional(&form.object),
                            quantity: optional(&form.relation_quantity),
                            unit: optional(&form.unit),
                        },
                    },
                    Field::Schedule => {
                        let mut work = record.work.clone();
                        let object = work
                            .as_object_mut()
                            .ok_or("Captured Work metadata is invalid")?;
                        for (key, value) in [("start", &form.start), ("due", &form.due)] {
                            if let Some(value) = optional(value) {
                                object.insert(key.into(), value.into());
                            } else {
                                object.remove(key);
                            }
                        }
                        if let Some(estimate) = optional(&form.estimate) {
                            let estimate: f64 = estimate.parse()?;
                            if !estimate.is_finite() || estimate < 0.0 {
                                return Err("Choose a finite nonnegative estimate".into());
                            }
                            object.insert("estimate_min".into(), serde_json::json!(estimate));
                        } else {
                            object.remove("estimate_min");
                        }
                        Edit::Work { value: work }
                    }
                }
            };
            let id = form
                .selected
                .clone()
                .unwrap_or_else(|| nucleus::new_uid("e"));
            let change = Change {
                id: id.clone(),
                record: record.uid.clone(),
                after_ms: milliseconds(&form.after)?,
                edit,
            };
            let editing = Editing {
                original: change.clone(),
                edit: change.edit.clone(),
                shown: form.values(),
            };
            if let Some(existing) = proposal.changes.iter_mut().find(|change| change.id == id) {
                *existing = change;
            } else {
                proposal.changes.push(change);
            }
            form.selected = Some(id);
            form.editing = Some(editing);
        }
    }
    scenario.validate()?;
    if !form.valid() {
        return Err("Record draft exceeds its size limits".into());
    }
    model.scenario = serde_json::to_string_pretty(&scenario)?;
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text(&model.scenario);
    world.entity_mut(owner).insert(model);
    render(world, owner);
    Ok(())
}

pub(super) fn loaded(model: &mut SimulationCastle, scenario: &Scenario) {
    let form = &mut model.record_form;
    if let Some(proposal) = &scenario.proposal
        && let Some(record) = proposal
            .records
            .iter()
            .find(|record| {
                record.uid == form.record || record.slug.as_deref() == Some(form.record.as_str())
            })
            .or_else(|| proposal.records.last())
    {
        if form
            .editing
            .as_ref()
            .is_some_and(|editing| editing.original.record == record.uid)
        {
            let field = form.field;
            if form.show_selected(record, field).is_err() {
                form.selected = None;
                form.editing = None;
                form.fill(record);
            }
        } else {
            form.selected = None;
            form.editing = None;
            form.fill(record);
        }
    }
    form.open = true;
    form.horizon = ((scenario.end_ms - scenario.start_ms) as f64 / 3_600_000.0).to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::actions::Action;

    fn set(world: &mut World, owner: Entity, field: InputField, value: &str) {
        let entity = world
            .query::<(Entity, &Input)>()
            .iter(world)
            .find(|(_, input)| {
                input.owner == owner
                    && std::mem::discriminant(&input.field) == std::mem::discriminant(&field)
            })
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
    }

    fn command(world: &mut World, owner: Entity, command: Command) {
        super::super::Command::Records(command).apply(world, owner);
    }

    fn run<F: Future<Output = ()>>(work: impl FnOnce() -> F + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_stack_size(16 * 1024 * 1024)
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(work())
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn updating_advanced_edits_preserves_hidden_fields_and_empty_text() {
        let uid = nucleus::new_uid("r");
        let mut scenario = simulation::fixtures::current_database(1_893_456_000_000);
        let hash =
            nucleus::karma::CanonicalHash::parse(format!("sha256:{}", "0".repeat(64))).unwrap();
        scenario.cells[0].database = Some(simulation::scenario::Database {
            file: "source.sqlite".into(),
            hash: hash.clone(),
        });
        scenario.proposal = Some(simulation::proposal::Proposal {
            id: "draft".into(),
            cell: "current".into(),
            source: hash,
            base_ms: scenario.start_ms,
            actor: None,
            records: vec![Record {
                uid: uid.clone(),
                revision: 1,
                kind: "plain".into(),
                head: "Original title".into(),
                body: "Original body".into(),
                quantity: nucleus::DecimalValue::parse_inferred("-1").unwrap(),
                unit: None,
                slug: None,
                assertions: Vec::new(),
                work: serde_json::json!({"start":"2030-01-03T09:00:00Z","due":"2030-01-03T10:00:00Z","estimate_min":60}),
            }],
            changes: vec![
                Change {
                    id: "text".into(),
                    record: uid.clone(),
                    after_ms: 0,
                    edit: Edit::Text {
                        head: Some("Authored title".into()),
                        body: Some(String::new()),
                    },
                },
                Change {
                    id: "work".into(),
                    record: uid.clone(),
                    after_ms: 0,
                    edit: Edit::Work {
                        value: serde_json::json!({"start":"2030-01-04T09:00:00Z","due":"2030-01-04T10:00:00Z","estimate_min":90,"logs":[{"start":"2030-01-03T09:00:00Z","end":"2030-01-03T09:20:00Z"}]}),
                    },
                },
            ],
        });
        let (mut app, owner) = super::super::tests::fixture(SimulationCastle {
            scenario: serde_json::to_string_pretty(&scenario).unwrap(),
            record_form: Form::target(&uid),
            ..Default::default()
        });
        command(app.world_mut(), owner, Command::Select("text".into()));
        set(app.world_mut(), owner, InputField::After, "0.5");
        command(app.world_mut(), owner, Command::Apply);
        let updated: Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        let change = &updated.proposal.as_ref().unwrap().changes[0];
        assert_eq!(change.after_ms, 1_800_000);
        assert!(
            matches!(&change.edit, Edit::Text {head:Some(head),body:Some(body)} if head == "Authored title" && body.is_empty())
        );
        command(app.world_mut(), owner, Command::Field(Field::Title));
        set(app.world_mut(), owner, InputField::Value, "Updated title");
        command(app.world_mut(), owner, Command::Field(Field::Description));
        set(app.world_mut(), owner, InputField::Value, "Updated body");
        command(app.world_mut(), owner, Command::Apply);
        command(app.world_mut(), owner, Command::Select("work".into()));
        set(
            app.world_mut(),
            owner,
            InputField::Due,
            "2030-01-04T11:00:00Z",
        );
        command(app.world_mut(), owner, Command::Apply);
        let edited: Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        let changes = &edited.proposal.as_ref().unwrap().changes;
        assert!(
            matches!(&changes[0].edit, Edit::Text {head:Some(head),body:Some(body)} if head == "Updated title" && body == "Updated body")
        );
        let Edit::Work { value: expected } = &scenario.proposal.as_ref().unwrap().changes[1].edit
        else {
            panic!("Expected authored Work edit")
        };
        assert!(
            matches!(&changes[1].edit, Edit::Work {value} if value["logs"] == expected["logs"] && value["estimate_min"] == 90 && value["due"] == "2030-01-04T11:00:00Z")
        );
        let root = app.world().get::<ChildOf>(owner).unwrap().parent();
        let saved = super::super::snapshot(app.world_mut(), root).pop().unwrap();
        let saved: super::super::SavedSimulation =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        app.world_mut().despawn(owner);
        saved.restore(app.world_mut(), root);
        let restored = app
            .world_mut()
            .query::<(Entity, &SimulationCastle)>()
            .iter(app.world())
            .next()
            .unwrap()
            .0;
        command(app.world_mut(), restored, Command::Apply);
        assert_eq!(
            app.world()
                .get::<SimulationCastle>(restored)
                .unwrap()
                .scenario,
            serde_json::to_string_pretty(&edited).unwrap()
        );
    }

    #[test]
    fn capture_edit_compare_reload_and_live_staleness_work_without_live_mutation() {
        run(|| async {
            let directory = tempfile::tempdir().unwrap();
            let scenario = simulation::fixtures::daily();
            let source =
                simulation::world::World::open(scenario.clone(), &directory.path().join("source"))
                    .await
                    .unwrap();
            let node = &source.nodes["a"];
            let uid = node
                .execution
                .scope(node.engine().act(
                    engine::actions::Action::CreateRecord {
                        slug: Some("appointment".into()),
                        kind: nucleus::RecordKind::Plain,
                        head: "Appointment".into(),
                        body: "Original".into(),
                        quantity: -1.0,
                    },
                    None,
                ))
                .await
                .unwrap()
                .created
                .unwrap();
            node.execution.scope(node.engine().act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: "work".into(), fds: serde_json::json!({"start":"2030-01-03T09:00:00Z","due":"2030-01-03T10:00:00Z","estimate_min":60}) }, None)).await.unwrap();
            let other = node
                .execution
                .scope(node.engine().act(
                    engine::actions::Action::CreateRecord {
                        slug: Some("other-stock".into()),
                        kind: nucleus::RecordKind::Plain,
                        head: "Other stock".into(),
                        body: String::new(),
                        quantity: 10.0,
                    },
                    None,
                ))
                .await
                .unwrap()
                .created
                .unwrap();
            let before = node.engine().store.state_hash().await.unwrap();
            let mut current = simulation::fixtures::current_database(scenario.start_ms);
            current.end_ms = scenario.end_ms;
            let (mut app, owner) = super::super::tests::fixture(SimulationCastle {
                current_database: true,
                scenario: serde_json::to_string_pretty(&current).unwrap(),
                selected_cell: "current".into(),
                output_directory: directory.path().join("runs").to_string_lossy().into(),
                record_form: Form {
                    field: Field::Schedule,
                    ..Form::target(&uid)
                },
                ..Default::default()
            });
            app.world_mut()
                .insert_resource(crate::native::app::CellHandle(node.cell.runtime().clone()));
            command(app.world_mut(), owner, Command::Capture(false));
            super::super::tests::until(&mut app, owner, |view, status| {
                view.task.is_none() && status.starts_with("Base captured")
            })
            .await;
            let model = app.world().get::<SimulationCastle>(owner).unwrap().clone();
            let captured: Scenario = serde_json::from_str(&model.scenario).unwrap();
            let database = captured.cells[0].database.as_ref().unwrap();
            assert_eq!(
                database.hash,
                simulation::artifacts::file_hash(
                    &std::path::Path::new(&model.source_directory).join(&database.file)
                )
                .unwrap()
            );
            set(
                app.world_mut(),
                owner,
                InputField::Start,
                "2030-01-04T09:00:00Z",
            );
            set(
                app.world_mut(),
                owner,
                InputField::Due,
                "2030-01-04T10:00:00Z",
            );
            command(app.world_mut(), owner, Command::Apply);
            let first: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            let id = first.proposal.as_ref().unwrap().changes[0].id.clone();
            set(
                app.world_mut(),
                owner,
                InputField::Due,
                "2030-01-04T11:00:00Z",
            );
            command(app.world_mut(), owner, Command::Apply);
            let updated: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            assert_eq!(updated.proposal.as_ref().unwrap().changes.len(), 1);
            assert_eq!(updated.proposal.as_ref().unwrap().changes[0].id, id);
            command(app.world_mut(), owner, Command::New);
            command(app.world_mut(), owner, Command::Field(Field::Quantity));
            set(app.world_mut(), owner, InputField::Value, "-2.75");
            set(app.world_mut(), owner, InputField::After, "0.5");
            command(app.world_mut(), owner, Command::Apply);
            let authored: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            let second = authored.proposal.as_ref().unwrap().changes[1].id.clone();
            command(app.world_mut(), owner, Command::Move(id.clone(), true));
            let reordered: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            let changes = &reordered.proposal.as_ref().unwrap().changes;
            assert_eq!((&changes[0].id, changes[0].after_ms), (&second, 0));
            assert_eq!((&changes[1].id, changes[1].after_ms), (&id, 1_800_000));
            command(app.world_mut(), owner, Command::Remove(second));
            command(app.world_mut(), owner, Command::Select(id.clone()));
            set(app.world_mut(), owner, InputField::After, "0.25");
            command(app.world_mut(), owner, Command::Apply);
            set(app.world_mut(), owner, InputField::Horizon, "96");
            command(app.world_mut(), owner, Command::Horizon);
            let configured = app
                .world()
                .get::<SimulationCastle>(owner)
                .unwrap()
                .scenario
                .clone();
            set(app.world_mut(), owner, InputField::Horizon, "NaN");
            command(app.world_mut(), owner, Command::Horizon);
            assert_eq!(
                app.world().get::<SimulationCastle>(owner).unwrap().scenario,
                configured
            );
            set(app.world_mut(), owner, InputField::Horizon, "96");
            set(app.world_mut(), owner, InputField::Record, "stock");
            command(app.world_mut(), owner, Command::Capture(false));
            super::super::tests::until(&mut app, owner, |view, status| {
                view.task.is_none() && status.starts_with("Base captured")
            })
            .await;
            let expanded: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            let proposal = expanded.proposal.as_ref().unwrap();
            assert_eq!(proposal.source, captured.proposal.as_ref().unwrap().source);
            assert_eq!(proposal.changes.len(), 1);
            assert_eq!(
                (&proposal.changes[0].id, proposal.changes[0].after_ms),
                (&id, 900_000)
            );
            let stock = proposal
                .records
                .iter()
                .find(|record| record.slug.as_deref() == Some("stock"))
                .unwrap()
                .uid
                .clone();
            command(app.world_mut(), owner, Command::Field(Field::Relation));
            set(app.world_mut(), owner, InputField::Predicate, "selected");
            set(app.world_mut(), owner, InputField::Object, &uid);
            set(app.world_mut(), owner, InputField::RelationQuantity, "1.25");
            command(app.world_mut(), owner, Command::Record(uid.clone()));
            assert_eq!(
                app.world()
                    .get::<SimulationCastle>(owner)
                    .unwrap()
                    .record_form
                    .predicate,
                "related"
            );
            set(app.world_mut(), owner, InputField::Predicate, "reserved");
            set(app.world_mut(), owner, InputField::Object, &stock);
            command(app.world_mut(), owner, Command::Record(stock.clone()));
            let form = &app
                .world()
                .get::<SimulationCastle>(owner)
                .unwrap()
                .record_form;
            assert_eq!(
                (&form.predicate, &form.object, &form.relation_quantity),
                (&"selected".to_string(), &uid, &"1.25".to_string())
            );
            command(app.world_mut(), owner, Command::Record(uid.clone()));
            let form = &app
                .world()
                .get::<SimulationCastle>(owner)
                .unwrap()
                .record_form;
            assert_eq!(
                (&form.predicate, &form.object),
                (&"reserved".to_string(), &stock)
            );
            command(app.world_mut(), owner, Command::Select(id.clone()));
            let mut authored: Scenario =
                serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                    .unwrap();
            let proposal = authored.proposal.as_mut().unwrap();
            if let Edit::Work { value } = &mut proposal.changes[0].edit {
                value["logs"] = serde_json::json!([{"start":"2030-01-03T09:00:00Z","end":"2030-01-03T09:20:00Z"}]);
            }
            proposal.changes.push(Change {
                id: "paired-text".into(),
                record: uid.clone(),
                after_ms: 900_000,
                edit: Edit::Text {
                    head: Some("$100 paired title".into()),
                    body: Some(String::new()),
                },
            });
            let editor = app.world().get::<View>(owner).unwrap().editor;
            app.world_mut()
                .get_mut::<EditableText>(editor)
                .unwrap()
                .editor
                .set_text(&serde_json::to_string_pretty(&authored).unwrap());
            command(
                app.world_mut(),
                owner,
                Command::Select("paired-text".into()),
            );
            set(app.world_mut(), owner, InputField::After, "0.5");
            command(app.world_mut(), owner, Command::Apply);
            command(app.world_mut(), owner, Command::Select(id.clone()));
            set(
                app.world_mut(),
                owner,
                InputField::Due,
                "2030-01-04T12:00:00Z",
            );
            command(app.world_mut(), owner, Command::Apply);
            assert_eq!(before, node.engine().store.state_hash().await.unwrap());
            node.execution
                .scope(node.engine().act(
                    engine::actions::Action::AddQuantity {
                        target: other.clone(),
                        delta: 1.0,
                    },
                    None,
                ))
                .await
                .unwrap();
            let changed_source = node.engine().store.state_hash().await.unwrap();
            assert_ne!(before, changed_source);
            super::super::Command::Compare.apply(app.world_mut(), owner);
            super::super::tests::until(&mut app, owner, |view, status| {
                view.task.is_none() && status.starts_with("Proposed:")
            })
            .await;
            let view = app.world().get::<View>(owner).unwrap();
            let result: simulation::comparison::Comparison =
                serde_json::from_str(&app.world().get::<Text>(view.evidence).unwrap().0).unwrap();
            assert_eq!(
                result.proposed.verdict,
                nucleus::simulation::Verdict::Passed
            );
            let change = result
                .changes
                .iter()
                .find(|change| change.record == uid)
                .unwrap();
            assert_eq!(
                result.sources["current"].state,
                captured.proposal.as_ref().unwrap().source
            );
            assert_eq!(
                result.source_status["current"],
                simulation::artifacts::SourceStatus::Changed
            );
            assert!(
                app.world()
                    .get::<Text>(view.status)
                    .unwrap()
                    .0
                    .contains("Live source changed")
            );
            assert_eq!(
                change
                    .proposed
                    .as_ref()
                    .unwrap()
                    .state
                    .as_ref()
                    .unwrap()
                    .work["due"],
                "2030-01-04T12:00:00Z"
            );
            assert_eq!(
                change
                    .baseline
                    .as_ref()
                    .unwrap()
                    .state
                    .as_ref()
                    .unwrap()
                    .work["due"],
                "2030-01-03T10:00:00Z"
            );
            let state = change.proposed.as_ref().unwrap().state.as_ref().unwrap();
            assert_eq!(state.head, "$100 paired title");
            assert!(state.body.is_empty());
            assert_eq!(
                state.work["logs"],
                authored
                    .proposal
                    .as_ref()
                    .unwrap()
                    .changes
                    .iter()
                    .find_map(|change| if let Edit::Work { value } = &change.edit {
                        Some(value["logs"].clone())
                    } else {
                        None
                    })
                    .unwrap()
            );
            assert_eq!(
                changed_source,
                node.engine().store.state_hash().await.unwrap()
            );
            let root = app.world().get::<ChildOf>(owner).unwrap().parent();
            let saved = super::super::snapshot(app.world_mut(), root).pop().unwrap();
            let saved: super::super::SavedSimulation =
                serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            assert!(saved.valid());
            app.world_mut().despawn(owner);
            saved.restore(app.world_mut(), root);
            let restored = app
                .world_mut()
                .query_filtered::<Entity, With<SimulationCastle>>()
                .iter(app.world())
                .next()
                .unwrap();
            let restored_scenario: Scenario = serde_json::from_str(
                &app.world()
                    .get::<SimulationCastle>(restored)
                    .unwrap()
                    .scenario,
            )
            .unwrap();
            assert_eq!(
                restored_scenario.proposal.as_ref().unwrap().changes[0].id,
                id
            );
            assert!(
                matches!(&restored_scenario.proposal.as_ref().unwrap().changes[1].edit, Edit::Text {head:Some(head),body:Some(body)} if head == "$100 paired title" && body.is_empty())
            );
            super::super::Command::Compare.apply(app.world_mut(), restored);
            super::super::tests::until(&mut app, restored, |view, status| {
                view.task.is_none() && status.starts_with("Proposed:")
            })
            .await;
            let status = app.world().get::<View>(restored).unwrap().status;
            assert!(
                app.world()
                    .get::<Text>(status)
                    .unwrap()
                    .0
                    .contains("Live source changed")
            );
            assert_eq!(
                changed_source,
                node.engine().store.state_hash().await.unwrap()
            );
            node.execution
                .scope(node.engine().act(
                    engine::actions::Action::EditRecordText {
                        target: uid,
                        head: Some("Changed live".into()),
                        body: None,
                    },
                    None,
                ))
                .await
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    app.update();
                    if app.world().get::<LiveWatch>(restored).is_some_and(|watch| {
                        watch
                            .updates
                            .borrow()
                            .starts_with("Captured Records changed")
                    }) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            app.update();
            assert!(
                app.world_mut()
                    .query::<&Text>()
                    .iter(app.world())
                    .any(|text| text.0.starts_with("Captured Records changed"))
            );
            super::super::Command::Daily.apply(app.world_mut(), restored);
            assert!(app.world().get::<LiveWatch>(restored).is_none());
        });
    }
}
