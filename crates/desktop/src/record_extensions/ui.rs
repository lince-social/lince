use super::*;
use crate::sand_panel::{button, column, row};

fn label(world: &mut World, parent: Entity, text: &str) {
    crate::edit_mode::label(world, parent, text, 14.0);
}

fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    key: InputKey,
    caption: &str,
    value: &str,
) {
    let entity = crate::sand_panel::field(world, parent, caption, value);
    world.entity_mut(entity).insert(Input {
        owner,
        key,
        observed: value.into(),
    });
}

fn controls(world: &mut World, parent: Entity, owner: Entity, values: Vec<(&str, Command)>) {
    let row = row(world, parent);
    if let Some(mut node) = world.get_mut::<Node>(row) {
        node.flex_wrap = FlexWrap::Wrap;
    }
    for (name, command) in values {
        button(world, row, owner, name, command);
    }
}

fn pages(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    page: usize,
    count: usize,
) -> std::ops::Range<usize> {
    let total = count.div_ceil(20).max(1);
    let page = page.min(total - 1);
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.page = page;
    }
    if total > 1 {
        label(
            world,
            parent,
            &format!("Page {} of {} · {} entries", page + 1, total, count),
        );
        controls(
            world,
            parent,
            owner,
            vec![
                ("Previous", Command::Page(false)),
                ("Next", Command::Page(true)),
            ],
        );
    }
    page * 20..((page + 1) * 20).min(count)
}

pub(super) fn render(world: &mut World, owner: Entity) {
    render_inner(world, owner);
    let focus = world
        .get_mut::<View>(owner)
        .and_then(|mut view| view.focus.take());
    if let Some((field, choice)) = focus {
        let target = world
            .query::<(Entity, &ChoiceFocus)>()
            .iter(world)
            .find(|(_, key)| key.owner == owner && key.field == field && key.choice == choice)
            .map(|(entity, _)| entity);
        if let Some(target) = target
            && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
        {
            focus.set(target, bevy::input_focus::FocusCause::Navigated);
        }
    }
}

fn render_inner(world: &mut World, owner: Entity) {
    let Some(settings) = world.get::<Settings>(owner).cloned() else {
        return;
    };
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let column = view.column;
    let schema = view.schema.clone();
    let message = view.message.clone();
    let pending = view.pending.is_some();
    let retry = view.retry.is_some();
    let undo = view.undo.is_some();
    let open = view.open.clone();
    let search = view.search.clone();
    let page = view.page;
    let editable = view.editable;
    let record_input = view.record_input.clone();
    let picker = view.picker.clone();
    let data = inspection(world, owner).cloned();
    crate::sand_panel::clear(world, owner);
    if settings.mode != Mode::Column {
        crate::edit_mode::label(
            world,
            owner,
            if column.is_some() {
                "Add extension column"
            } else if settings.mode == Mode::Dropdown {
                "Dropdown Sand"
            } else {
                "Extension Editor"
            },
            20.0,
        );
        if column.is_none() {
            let record_label = settings
                .record
                .as_deref()
                .map(|uid| reference_name(world, owner, uid));
            if let Some(record_label) = record_label {
                label(world, owner, &format!("Record: {record_label}"));
            }
            input(
                world,
                owner,
                owner,
                InputKey::Record,
                "Record reference (optional)",
                record_input.as_deref().unwrap_or(""),
            );
            controls(
                world,
                owner,
                owner,
                vec![
                    ("Open Record", Command::Record),
                    ("Browse Records", Command::Pick(InputKey::Record)),
                    ("New schema", Command::New),
                    ("Reload", Command::Reload),
                ],
            );
        } else {
            button(world, owner, owner, "New schema", Command::New);
        }
    }
    if !message.is_empty() {
        label(world, owner, &message);
    }
    if pending {
        label(
            world,
            owner,
            "Waiting for the Organ to confirm this change…",
        );
        return;
    }
    if retry {
        button(world, owner, owner, "Retry same change", Command::Retry);
    }
    if let Some(picker) = picker {
        input(
            world,
            owner,
            owner,
            InputKey::Search,
            "Find by name",
            &search,
        );
        controls(
            world,
            owner,
            owner,
            vec![("Search", Command::Search), ("Back", Command::ClosePicker)],
        );
        for value in &picker.rows {
            if let Some(uid) = value["uid"].as_str() {
                let name = value["name"]
                    .as_str()
                    .or(value["head"].as_str())
                    .unwrap_or(uid);
                button(world, owner, owner, name, Command::Picked(uid.into()));
            }
        }
        label(
            world,
            owner,
            "Showing at most 50 results. Search to narrow the list.",
        );
        return;
    }
    if let Some(draft) = schema {
        schema_form(world, owner, &draft, &search, page);
        return;
    }
    let Some(data) = data else {
        let error = world
            .get_resource::<Runtime>()
            .and_then(|runtime| runtime.feeds.get(&world.get::<View>(owner).unwrap().feed))
            .map(|feed| feed.error.clone())
            .unwrap_or_default();
        label(
            world,
            owner,
            if error.is_empty() {
                "Loading schemas and fields…"
            } else {
                &error
            },
        );
        return;
    };
    if data.more {
        label(
            world,
            owner,
            "Some schemas are omitted from this large catalogue. Choose fewer schemas per Record.",
        );
    }
    if settings.mode == Mode::Column {
        let (Some(schema), Some(field)) = (settings.schema.as_deref(), settings.field.as_deref())
        else {
            return;
        };
        let Some(definition) = data.schemas.iter().find(|entry| entry.uid == schema) else {
            label(
                world,
                owner,
                "Schema unavailable or not attached. Open Extensions to attach it.",
            );
            return;
        };
        let Some(field) = definition
            .schema
            .fields
            .iter()
            .find(|entry| entry.id == field)
        else {
            label(world, owner, "Field unavailable");
            return;
        };
        let writable = editable && data.writable.contains(&definition.uid);
        draft_values(world, owner, schema);
        let values = world
            .get::<View>(owner)
            .unwrap()
            .values
            .as_ref()
            .unwrap()
            .fields
            .clone();
        value_field(
            world,
            owner,
            owner,
            field,
            values.get(&field.id).unwrap_or(&Value::Null),
            writable,
            &open,
            &search,
            page,
        );
        finish_values(world, owner, definition, &values, writable, undo);
        return;
    }
    if column.is_some()
        || settings.mode == Mode::Dropdown && settings.field.is_none()
        || settings.schema.is_none()
    {
        label(
            world,
            owner,
            "Choose a reusable schema. A Record can use several schemas.",
        );
        input(
            world,
            owner,
            owner,
            InputKey::Search,
            "Find schema",
            &search,
        );
        button(world, owner, owner, "Search", Command::Search);
        let schemas = data
            .schemas
            .iter()
            .filter(|entry| {
                entry
                    .schema
                    .name
                    .to_lowercase()
                    .contains(&search.to_lowercase())
            })
            .collect::<Vec<_>>();
        if schemas.is_empty() {
            label(
                world,
                owner,
                "No saved schemas match. Create a schema to define your fields and choices.",
            );
        }
        for index in pages(world, owner, owner, page, schemas.len()) {
            let entry = schemas[index];
            label(world, owner, &entry.schema.name);
            if column.is_some() || settings.mode == Mode::Dropdown {
                for field in
                    entry.schema.fields.iter().filter(|field| {
                        !field.archived && (column.is_some() || field.kind.choices())
                    })
                {
                    button(
                        world,
                        owner,
                        owner,
                        &format!("{} · {}", field.name, field.kind.name()),
                        Command::Bind(entry.uid.clone(), field.id.clone()),
                    );
                }
            } else {
                let row = row(world, owner);
                let attached = data
                    .values
                    .get(&entry.uid)
                    .is_some_and(|value| value.attached);
                button(
                    world,
                    row,
                    owner,
                    if attached {
                        "Edit Record values"
                    } else {
                        "Use this schema"
                    },
                    Command::Schema(entry.uid.clone()),
                );
                if entry.editable {
                    button(
                        world,
                        row,
                        owner,
                        "Edit schema",
                        Command::Edit(entry.uid.clone()),
                    );
                }
            }
        }
        return;
    }
    let schema = settings.schema.as_deref().unwrap();
    let Some(definition) = data.schemas.iter().find(|entry| entry.uid == schema) else {
        label(
            world,
            owner,
            "Schema unavailable; reload or choose another schema.",
        );
        controls(
            world,
            owner,
            owner,
            vec![("Choose schema", Command::Discard)],
        );
        return;
    };
    label(world, owner, &definition.schema.name);
    if settings.record.is_none() {
        label(
            world,
            owner,
            "Open a Record to enter values. Schemas are reusable across Records.",
        );
    }
    let writable = settings.record.is_some() && editable && data.writable.contains(&definition.uid);
    draft_values(world, owner, schema);
    let draft = world
        .get::<View>(owner)
        .unwrap()
        .values
        .as_ref()
        .unwrap()
        .clone();
    if !draft.attached {
        label(
            world,
            owner,
            "Apply to attach this schema to the Record. Its other schemas are kept.",
        );
    }
    for field in &definition.schema.fields {
        if settings.mode == Mode::Dropdown && settings.field.as_deref() != Some(&field.id) {
            continue;
        }
        if field.archived && !draft.fields.contains_key(&field.id) {
            continue;
        }
        value_field(
            world,
            owner,
            owner,
            field,
            draft.fields.get(&field.id).unwrap_or(&Value::Null),
            writable,
            &open,
            &search,
            page,
        );
    }
    finish_values(world, owner, definition, &draft.fields, writable, undo);
    let row = row(world, owner);
    button(world, row, owner, "Choose schema or field", ChooseSchema);
    if definition.editable {
        button(
            world,
            row,
            owner,
            "Edit schema",
            Command::Edit(schema.into()),
        );
    }
    if draft.attached && writable {
        button(
            world,
            owner,
            owner,
            "Hide schema and remove its managed assertions",
            Command::Detach,
        );
        label(
            world,
            owner,
            "Hidden values are kept. Manual assertions remain.",
        );
    }
}

#[derive(Clone)]
struct ChooseSchema;
impl Action for ChooseSchema {
    fn apply(&self, world: &mut World, owner: Entity) {
        capture(world, owner);
        if world
            .get::<View>(owner)
            .is_none_or(|view| view.pending.is_some() || view.dirty())
        {
            return;
        }
        let mut settings = world.get_mut::<Settings>(owner).unwrap();
        settings.schema = None;
        settings.field = None;
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.values = None;
        view.render = true;
        view.search.clear();
        view.page = 0;
    }
}

fn schema_form(world: &mut World, owner: Entity, draft: &SchemaDraft, search: &str, page: usize) {
    input(
        world,
        owner,
        owner,
        InputKey::SchemaName,
        "Schema name",
        &draft.schema.name,
    );
    label(
        world,
        owner,
        "Rename or reorder freely. Archive fields and choices to keep existing selections. Saved field types stay fixed; add a new field for a different type.",
    );
    controls(
        world,
        owner,
        owner,
        vec![
            ("Save schema", Command::SaveSchema),
            ("Back", Command::Done),
            ("Discard draft", Command::Discard),
        ],
    );
    let fields = row(world, owner);
    world.get_mut::<Node>(fields).unwrap().flex_wrap = FlexWrap::Wrap;
    for (index, field) in draft.schema.fields.iter().enumerate() {
        button(
            world,
            fields,
            owner,
            &format!(
                "{}{}",
                field.name,
                if field.archived { " (archived)" } else { "" }
            ),
            Command::Field(index),
        );
    }
    let choices = FieldKind::ALL
        .into_iter()
        .map(|kind| (kind.name().into(), crate::actions![Command::AddField(kind)]))
        .collect();
    crate::dropdown::spawn(world, owner, owner, "Add field", "Add field…", choices);
    let field = &draft.schema.fields[draft.field];
    input(
        world,
        owner,
        owner,
        InputKey::FieldName,
        "Field name",
        &field.name,
    );
    label(world, owner, &format!("Type: {}", field.kind.name()));
    if draft.uid.is_none()
        || !draft
            .original
            .fields
            .iter()
            .any(|saved| saved.id == field.id)
    {
        let choices = FieldKind::ALL
            .into_iter()
            .map(|kind| (kind.name().into(), crate::actions![Command::Kind(kind)]))
            .collect();
        crate::dropdown::spawn(
            world,
            owner,
            owner,
            "Field type",
            field.kind.name(),
            choices,
        );
    }
    controls(
        world,
        owner,
        owner,
        vec![
            (
                if field.archived {
                    "Restore field"
                } else {
                    "Archive field"
                },
                Command::ArchiveField,
            ),
            ("Move field up", Command::MoveField(false)),
            ("Move field down", Command::MoveField(true)),
        ],
    );
    if !field.kind.choices() {
        return;
    }
    input(world, owner, owner, InputKey::Search, "Find choice", search);
    controls(
        world,
        owner,
        owner,
        vec![
            ("Search", Command::Search),
            ("Add choice", Command::AddChoice),
        ],
    );
    let choices = field
        .choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| choice.name.to_lowercase().contains(&search.to_lowercase()))
        .collect::<Vec<_>>();
    for index in pages(world, owner, owner, page, choices.len()) {
        let (index, choice) = choices[index];
        button(
            world,
            owner,
            owner,
            &format!(
                "{}{} · {} assertions",
                choice.name,
                if choice.archived { " (archived)" } else { "" },
                choice.assertions.len()
            ),
            Command::Choice(index),
        );
    }
    if let Some(index) = draft.choice {
        let choice = &field.choices[index];
        let form = column(world, owner);
        input(
            world,
            form,
            owner,
            InputKey::ChoiceName,
            "Choice label",
            &choice.name,
        );
        controls(
            world,
            form,
            owner,
            vec![
                (
                    if choice.archived {
                        "Restore choice"
                    } else {
                        "Archive choice"
                    },
                    Command::ArchiveChoice,
                ),
                ("Move choice up", Command::MoveChoice(false)),
                ("Move choice down", Command::MoveChoice(true)),
                ("Add preset assertion", Command::AddPreset),
            ],
        );
        label(
            world,
            form,
            "Selecting this choice applies the assertions below when you press Apply. Use existing concept names and optional Record names; no code or JSON is needed. Editing a preset affects selections when their values are next applied.",
        );
        for (index, preset) in choice.assertions.iter().enumerate() {
            input(
                world,
                form,
                owner,
                InputKey::Preset(index, 0),
                "Assertion concept",
                &reference_name(world, owner, &preset.predicate),
            );
            button(
                world,
                form,
                owner,
                "Choose assertion concept",
                Command::Pick(InputKey::Preset(index, 0)),
            );
            input(
                world,
                form,
                owner,
                InputKey::Preset(index, 1),
                "Object Record (optional)",
                &preset
                    .object
                    .as_deref()
                    .map(|uid| reference_name(world, owner, uid))
                    .unwrap_or_default(),
            );
            button(
                world,
                form,
                owner,
                "Choose object Record",
                Command::Pick(InputKey::Preset(index, 1)),
            );
            input(
                world,
                form,
                owner,
                InputKey::Preset(index, 2),
                "Quantity (optional)",
                preset.quantity.as_deref().unwrap_or(""),
            );
            input(
                world,
                form,
                owner,
                InputKey::Preset(index, 3),
                "Unit concept (optional)",
                &preset
                    .unit
                    .as_deref()
                    .map(|uid| reference_name(world, owner, uid))
                    .unwrap_or_default(),
            );
            button(
                world,
                form,
                owner,
                "Choose unit concept",
                Command::Pick(InputKey::Preset(index, 3)),
            );
            button(
                world,
                form,
                owner,
                "Remove preset",
                Command::RemovePreset(index),
            );
        }
    }
}

fn value_field(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    field: &Field,
    value: &Value,
    writable: bool,
    open: &Option<String>,
    search: &str,
    page: usize,
) {
    let writable = writable && !field.archived;
    if field.archived {
        label(world, parent, &format!("{} (archived)", field.name));
    }
    match field.kind {
        FieldKind::Text | FieldKind::Number => {
            if writable {
                input(
                    world,
                    parent,
                    owner,
                    InputKey::Value(field.id.clone()),
                    &field.name,
                    value.as_str().unwrap_or(""),
                );
            } else {
                label(
                    world,
                    parent,
                    &format!("{}: {}", field.name, value.as_str().unwrap_or("—")),
                );
            }
        }
        FieldKind::Boolean => {
            let selected = value.as_bool().unwrap_or(false);
            if writable {
                button(
                    world,
                    parent,
                    owner,
                    &format!("{} {}", if selected { "☑" } else { "☐" }, field.name),
                    Command::Set(field.id.clone(), json!(!selected)),
                );
            } else {
                label(
                    world,
                    parent,
                    &format!("{}: {}", field.name, if selected { "Yes" } else { "No" }),
                );
            }
        }
        FieldKind::Select | FieldKind::MultiSelect => {
            let selected = field.selection(value).unwrap_or_default();
            let caption = field
                .choices
                .iter()
                .filter(|choice| selected.contains(&choice.id.as_str()))
                .map(|choice| {
                    format!(
                        "{}{}",
                        choice.name,
                        if choice.archived { " (archived)" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let caption = if caption.is_empty() {
                if selected.is_empty() {
                    "Choose…".into()
                } else {
                    "Unavailable choice".into()
                }
            } else {
                caption
            };
            if writable {
                let toggle = button(
                    world,
                    parent,
                    owner,
                    &format!("{}: {}", field.name, caption),
                    Command::Open(field.id.clone()),
                );
                world.entity_mut(toggle).insert(ChoiceFocus {
                    owner,
                    field: field.id.clone(),
                    choice: None,
                });
                if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) {
                    node.set_expanded(open.as_ref() == Some(&field.id));
                }
                if open.as_ref() == Some(&field.id) {
                    let menu = column(world, parent);
                    world.entity_mut(menu).insert(ChoiceMenu { owner, toggle });
                    input(world, menu, owner, InputKey::Search, "Find choice", search);
                    controls(
                        world,
                        menu,
                        owner,
                        vec![
                            ("Search", Command::Search),
                            ("Clear selection", Command::Clear(field.id.clone())),
                        ],
                    );
                    let choices = field
                        .choices
                        .iter()
                        .filter(|choice| {
                            (!choice.archived || selected.contains(&choice.id.as_str()))
                                && choice.name.to_lowercase().contains(&search.to_lowercase())
                        })
                        .collect::<Vec<_>>();
                    for index in pages(world, menu, owner, page, choices.len()) {
                        let choice = choices[index];
                        if choice.archived && !selected.contains(&choice.id.as_str()) {
                            continue;
                        }
                        let caption = format!(
                            "{} {}{}",
                            if selected.contains(&choice.id.as_str()) {
                                "✓"
                            } else {
                                "○"
                            },
                            choice.name,
                            if choice.archived { " (archived)" } else { "" }
                        );
                        let option = button(
                            world,
                            menu,
                            owner,
                            &caption,
                            Command::Select(field.id.clone(), choice.id.clone()),
                        );
                        world.entity_mut(option).insert(ChoiceFocus {
                            owner,
                            field: field.id.clone(),
                            choice: Some(choice.id.clone()),
                        });
                        if let Some(mut node) =
                            world.get_mut::<bevy::a11y::AccessibilityNode>(option)
                        {
                            node.set_selected(selected.contains(&choice.id.as_str()));
                        }
                    }
                    if choices.is_empty() {
                        label(world, menu, "No matching choices");
                    }
                }
            } else {
                label(world, parent, &format!("{}: {}", field.name, caption));
            }
        }
    }
    if writable && !value.is_null() && !field.kind.choices() {
        button(
            world,
            parent,
            owner,
            "Clear value",
            Command::Clear(field.id.clone()),
        );
    }
}

#[derive(Component)]
struct ChoiceMenu {
    owner: Entity,
    toggle: Entity,
}

#[derive(Component)]
struct ChoiceFocus {
    owner: Entity,
    field: String,
    choice: Option<String>,
}

fn finish_values(
    world: &mut World,
    owner: Entity,
    definition: &nucleus::record_extension::Definition,
    fields: &BTreeMap<String, Value>,
    writable: bool,
    undo: bool,
) {
    let wanted = selected_presets(&definition.schema, fields);
    let original = world
        .get::<View>(owner)
        .and_then(|view| view.values.as_ref())
        .map(|draft| selected_presets(&definition.schema, &draft.original))
        .unwrap_or_default();
    let added = wanted
        .iter()
        .filter(|preset| !original.contains(preset))
        .collect::<Vec<_>>();
    let removed = original
        .iter()
        .filter(|preset| !wanted.contains(preset))
        .collect::<Vec<_>>();
    if !wanted.is_empty() || !removed.is_empty() {
        label(
            world,
            owner,
            "Preset assertion preview · Manual assertions are preserved.",
        );
        for preset in &wanted {
            label(
                world,
                owner,
                &format!(
                    "{} {}",
                    if added.contains(&preset) {
                        "Apply if missing:"
                    } else {
                        "Keep or restore:"
                    },
                    preset_name(world, owner, preset)
                ),
            );
        }
        for preset in removed {
            label(
                world,
                owner,
                &format!(
                    "Remove if created by this selection: {}",
                    preset_name(world, owner, preset)
                ),
            );
        }
    }
    if writable {
        let mut commands = vec![
            ("Apply values and assertions", Command::Apply),
            ("Discard draft", Command::Discard),
        ];
        if undo {
            commands.push(("Undo last change", Command::Undo));
        }
        controls(world, owner, owner, commands);
    } else {
        label(world, owner, "Read only");
    }
}

fn selected_presets(schema: &Schema, fields: &BTreeMap<String, Value>) -> Vec<Preset> {
    let mut presets = Vec::new();
    for field in &schema.fields {
        for id in field
            .selection(fields.get(&field.id).unwrap_or(&Value::Null))
            .unwrap_or_default()
        {
            if let Some(choice) = field.choices.iter().find(|choice| choice.id == id) {
                for preset in &choice.assertions {
                    if !presets.contains(preset) {
                        presets.push(preset.clone());
                    }
                }
            }
        }
    }
    presets
}

fn reference_name(world: &World, owner: Entity, uid: &str) -> String {
    world
        .get::<View>(owner)
        .and_then(|view| view.reference_labels.get(uid))
        .or_else(|| inspection(world, owner).and_then(|data| data.labels.get(uid)))
        .cloned()
        .unwrap_or_else(|| {
            if nucleus::valid_uid(uid, "c") {
                "Selected concept".into()
            } else if nucleus::valid_uid(uid, "r") {
                "Selected Record".into()
            } else {
                uid.into()
            }
        })
}

fn preset_name(world: &World, owner: Entity, preset: &Preset) -> String {
    format!(
        "#{}{}{}{}",
        reference_name(world, owner, &preset.predicate),
        preset
            .object
            .as_ref()
            .map(|value| format!(" → {}", reference_name(world, owner, value)))
            .unwrap_or_default(),
        preset
            .quantity
            .as_ref()
            .map(|value| format!(": {value}"))
            .unwrap_or_default(),
        preset
            .unit
            .as_ref()
            .map(|value| format!(" {}", reference_name(world, owner, value)))
            .unwrap_or_default()
    )
}

pub(super) fn keyboard(world: &mut World) {
    if !world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
    {
        return;
    }
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let menus = world
        .query::<(Entity, &ChoiceMenu)>()
        .iter(world)
        .map(|(entity, menu)| (entity, menu.owner, menu.toggle))
        .collect::<Vec<_>>();
    for (menu, owner, toggle) in menus {
        let mut current = focused;
        while let Some(entity) = current {
            if entity == menu || entity == owner {
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.open = None;
                }
                world.get_mut::<Node>(menu).unwrap().display = Display::None;
                if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(toggle) {
                    node.set_expanded(false);
                }
                if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
                    focus.set(toggle, bevy::input_focus::FocusCause::Navigated);
                }
                break;
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}
