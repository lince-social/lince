use crate::{
    app::{self, Intent, Mobile, act, button, input, label},
    navigation::Page,
};
use bevy::prelude::*;
use engine::actions::Action;
use lince_interface::{
    controls,
    records::{KANBAN_COLUMNS, display},
};
use serde_json::Value;

pub fn render(world: &mut World, parent: Entity, page: Page) {
    match page {
        Page::Records => records(world, parent, false),
        Page::Kanban => records(world, parent, true),
        Page::Record(uid) => record(world, parent, &uid),
        Page::Organ => crate::organ::render(world, parent),
        Page::Karma => karma(world, parent),
        Page::Frequency => frequency(world, parent),
        Page::Credits => {
            for credit in lince_interface::credits::ATTRIBUTIONS {
                label(world, parent, credit.name, 22.0);
                label(world, parent, credit.author, 16.0);
                label(world, parent, credit.license, 14.0);
            }
            label(world, parent, "Loro", 22.0);
            label(
                world,
                parent,
                include_str!("../licenses/loro-MIT.txt"),
                14.0,
            );
            label(world, parent, "JNI bindings · jni-rs contributors", 22.0);
            label(world, parent, include_str!("../licenses/jni-MIT.txt"), 14.0);
            label(
                world,
                parent,
                "Rustls platform verifier · Rustls contributors",
                22.0,
            );
            label(
                world,
                parent,
                include_str!("../licenses/rustls-platform-verifier-MIT.txt"),
                14.0,
            );
            label(
                world,
                parent,
                "Kotlin and annotations · JetBrains and contributors",
                22.0,
            );
            label(
                world,
                parent,
                concat!(
                    include_str!("../licenses/kotlin-COPYRIGHT.txt"),
                    "\n",
                    include_str!("../licenses/kotlin-NOTICE.txt"),
                    "\n",
                    include_str!("../licenses/kotlin-Apache-2.0.txt"),
                ),
                14.0,
            );
        }
    }
}

fn rows(world: &World, topic: &str) -> Vec<Value> {
    world
        .resource::<Mobile>()
        .rows
        .get(topic)
        .cloned()
        .unwrap_or_default()
}

fn records(world: &mut World, parent: Entity, kanban: bool) {
    crate::views::render(world, parent);
    let search = world.resource::<Mobile>().search.clone();
    input(
        world,
        parent,
        "list",
        "search",
        "Find Records",
        &search,
        false,
    );
    let actions = controls::row(world, parent);
    button(world, actions, "Search", Intent::Search);
    let sort = crate::record::SORTS[world.resource::<Mobile>().sort].0;
    button(world, actions, &format!("Sort: {sort}"), Intent::Sort);
    button(
        world,
        actions,
        if kanban { "New task" } else { "New Record" },
        Intent::CreateRecord,
    );
    button(world, actions, "Refresh", Intent::Refresh);
    let filter = if world.resource::<Mobile>().negative_only {
        "Showing negative quantities"
    } else {
        "Showing all quantities"
    };
    button(world, parent, filter, Intent::ToggleNegative);
    let all_records = rows(world, "records");
    let page = world.resource::<Mobile>().record_pages.len() + 1;
    let records: Vec<_> = all_records
        .iter()
        .take(crate::views::PAGE_SIZE)
        .cloned()
        .collect();
    label(world, parent, &format!("Page {page}"), 14.0);
    if records.is_empty() {
        label(world, parent, "No Records to display", 18.0);
    }
    if kanban {
        let tabs = controls::row(world, parent);
        let selected = world.resource::<Mobile>().kanban_column;
        for (column, (title, _, _)) in KANBAN_COLUMNS.iter().enumerate() {
            button(
                world,
                tabs,
                &format!("{}{title}", if selected == column { "✓ " } else { "" }),
                Intent::KanbanColumn(column),
            );
        }
        button(
            world,
            tabs,
            "Other quantities",
            Intent::KanbanColumn(KANBAN_COLUMNS.len()),
        );
        crate::kanban::settings(world, parent);
        for (column, (title, _, quantity)) in KANBAN_COLUMNS.iter().enumerate() {
            if selected != column {
                continue;
            }
            label(world, parent, title, 22.0);
            for record in &records {
                if nucleus::DecimalValue::parse_inferred(&display(&record["quantity"])).ok()
                    != nucleus::DecimalValue::parse_inferred(&quantity.to_string()).ok()
                {
                    continue;
                }
                card(world, parent, record, Some(column));
            }
        }
        if selected == KANBAN_COLUMNS.len() {
            label(world, parent, "Other quantities", 22.0);
        }
        for record in &records {
            if selected != KANBAN_COLUMNS.len() {
                continue;
            }
            let quantity =
                nucleus::DecimalValue::parse_inferred(&display(&record["quantity"])).ok();
            if !KANBAN_COLUMNS.iter().any(|(_, _, value)| {
                quantity == nucleus::DecimalValue::parse_inferred(&value.to_string()).ok()
            }) {
                card(world, parent, record, Some(usize::MAX));
            }
        }
    } else {
        for record in &records {
            card(world, parent, record, None);
        }
    }
    if page > 1 {
        button(world, parent, "Previous page", Intent::Previous);
    }
    if all_records.len() > crate::views::PAGE_SIZE {
        button(world, parent, "Next page", Intent::More);
    }
}

fn card(world: &mut World, parent: Entity, record: &Value, column: Option<usize>) {
    let Some(uid) = record["uid"].as_str() else {
        return;
    };
    let group = controls::column(world, parent);
    let title = record["head"]
        .as_str()
        .filter(|head| !head.is_empty())
        .unwrap_or("Untitled Record");
    button(world, group, title, Intent::Open(Page::Record(uid.into())));
    let draft = crate::views::draft(world.resource::<Mobile>());
    for (key, title) in crate::views::FIELDS {
        if draft.query["fields"]
            .as_array()
            .is_none_or(|fields| fields.iter().any(|field| field == key))
        {
            label(
                world,
                group,
                &format!("{title}: {}", display(&record[*key])),
                14.0,
            );
        }
    }
    if nucleus::DecimalValue::parse_inferred(&display(&record["quantity"])).is_ok_and(|quantity| {
        quantity
            .exact_numeric_cmp(nucleus::DecimalValue::parse_inferred("0").expect("zero"))
            .is_lt()
    }) {
        button(world, group, "Set to 0", Intent::CompleteRecord(uid.into()));
    }
    if let Some(current) = column {
        button(world, group, "Move to…", Intent::MoveMenu(uid.into()));
        if world.resource::<Mobile>().moving.as_deref() != Some(uid) {
            return;
        }
        let handle = button(
            world,
            group,
            "Drag this task onto a destination below",
            Intent::Nothing,
        );
        world
            .entity_mut(handle)
            .insert(crate::kanban::Handle(uid.into()));
        let moves = controls::row(world, group);
        for (index, (title, _, _)) in KANBAN_COLUMNS.iter().enumerate() {
            if current != index {
                let destination = button(world, moves, title, Intent::Move(uid.into(), index));
                world
                    .entity_mut(destination)
                    .insert(crate::kanban::Destination(index));
            }
        }
    }
}

fn record(world: &mut World, parent: Entity, uid: &str) {
    let Some(record) = app::row(world, "record", uid) else {
        let missing = world
            .resource::<Mobile>()
            .rows
            .get("record")
            .is_some_and(Vec::is_empty);
        label(
            world,
            parent,
            if missing {
                "This Record is unavailable or has been deleted."
            } else {
                "Loading Record…"
            },
            18.0,
        );
        button(world, parent, "Refresh", Intent::Refresh);
        return;
    };
    for field in protein::record_schema::fields() {
        let value = display(&record[field.key]);
        if field.key == "body" {
            label(world, parent, "Description preview", 22.0);
            let draft = world.resource::<Mobile>().draft(uid, "body", &value);
            crate::body::render(world, parent, &draft);
            label(
                world,
                parent,
                "Edit with Markdown: **bold**, _italic_, headings and lists.",
                14.0,
            );
        }
        if matches!(
            field.key,
            "head" | "body" | "slug" | "quantity" | "start_date" | "due_date" | "estimate_min"
        ) {
            input(
                world,
                parent,
                uid,
                field.key,
                field.title,
                &value,
                field.key == "body",
            );
            button(
                world,
                parent,
                &format!("Save {}", field.title),
                Intent::SaveField(uid.into(), field.key.into()),
            );
        } else if field.key == "work_timer" {
            let running = record["running_since"].is_string();
            button(
                world,
                parent,
                if running { "Stop timer" } else { "Start timer" },
                Intent::Act(Action::ChangeRecord {
                    request: engine::record_change::Request {
                        id: nucleus::new_uid("op"),
                        record_uid: uid.into(),
                        mutation: engine::record_change::Mutation::Timer { running: !running },
                    },
                }),
            );
        } else if !matches!(
            field.key,
            "assertions" | "assignees" | "work_logs" | "threads"
        ) {
            label(world, parent, field.title, 18.0);
            label(world, parent, &value, 16.0);
        }
    }
    assertions(world, parent, uid, &record);
    logs(world, parent, uid, &record);
    button(
        world,
        parent,
        "New thread",
        Intent::Act(Action::CreateThread {
            target: uid.into(),
            head: String::new(),
        }),
    );
    for thread in record["threads"].as_array().into_iter().flatten() {
        if let Some(thread_uid) = thread["uid"].as_str() {
            button(
                world,
                parent,
                thread["head"].as_str().unwrap_or("Thread"),
                Intent::Open(Page::Record(thread_uid.into())),
            );
            for message in thread["messages"].as_array().into_iter().flatten() {
                crate::body::render(world, parent, message["body"].as_str().unwrap_or_default());
                if let Some(message_uid) = message["uid"].as_str() {
                    label(
                        world,
                        parent,
                        &format!(
                            "{} · {}",
                            display(&message["author_name"]),
                            display(&message["created_at"])
                        ),
                        12.0,
                    );
                    button(
                        world,
                        parent,
                        "Reply",
                        Intent::ReplyTo(thread_uid.into(), Some(message_uid.into())),
                    );
                    button(
                        world,
                        parent,
                        "Edit message",
                        Intent::Open(Page::Record(message_uid.into())),
                    );
                    button(
                        world,
                        parent,
                        "Delete message",
                        Intent::Ask(Action::DeleteRecord {
                            target: message_uid.into(),
                        }),
                    );
                    for (index, part) in message["content"]["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        if part["kind"] == "attachment" {
                            button(
                                world,
                                parent,
                                &format!("Save {}", part["name"].as_str().unwrap_or("attachment")),
                                Intent::Act(Action::ReadMessageAttachment {
                                    message: message_uid.into(),
                                    index,
                                }),
                            );
                        }
                    }
                }
            }
            if thread["messages_has_more"] == true {
                button(
                    world,
                    parent,
                    "Earlier messages",
                    Intent::MoreMessages(thread_uid.into()),
                );
            }
            if world
                .resource::<Mobile>()
                .thread_pages
                .get(thread_uid)
                .is_some_and(|pages| !pages.is_empty())
            {
                button(
                    world,
                    parent,
                    "Newer messages",
                    Intent::NewerMessages(thread_uid.into()),
                );
                button(
                    world,
                    parent,
                    "Latest messages",
                    Intent::LatestMessages(thread_uid.into()),
                );
            }
            if !world
                .resource::<Mobile>()
                .draft(thread_uid, "parent", "")
                .is_empty()
            {
                label(world, parent, "Replying to the selected message", 14.0);
                button(
                    world,
                    parent,
                    "Cancel reply",
                    Intent::ReplyTo(thread_uid.into(), None),
                );
            }
            input(world, parent, thread_uid, "message", "Message", "", true);
            button(
                world,
                parent,
                "Attach file (up to 4 MiB)",
                Intent::AttachFile(thread_uid.into()),
            );
            let attachments = world
                .resource::<Mobile>()
                .attachments
                .get(thread_uid)
                .cloned()
                .unwrap_or_default();
            for (index, part) in attachments.iter().enumerate() {
                if let nucleus::message::MessagePart::Attachment { name, .. } = part {
                    button(
                        world,
                        parent,
                        &format!("Remove {name}"),
                        Intent::RemoveAttachment(thread_uid.into(), index),
                    );
                }
            }
            button(
                world,
                parent,
                "Send",
                Intent::RecordControl(thread_uid.into(), RecordControl::Message),
            );
        }
    }
    button(
        world,
        parent,
        "Delete Record",
        Intent::DeleteRecord(uid.into()),
    );
}

#[derive(Clone)]
pub enum RecordControl {
    Assertion,
    Assignee,
    Log,
    Message,
}

fn mutation(uid: &str, mutation: engine::record_change::Mutation) -> Action {
    Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: uid.into(),
            mutation,
        },
    }
}

fn assertions(world: &mut World, parent: Entity, uid: &str, record: &Value) {
    label(world, parent, "Assertions and assignments", 22.0);
    for assertion in record["assertions"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &display(&Value::Array(vec![assertion.clone()])),
            16.0,
        );
        if let Some(id) = assertion["uid"].as_str() {
            button(
                world,
                parent,
                "Remove assertion",
                Intent::Ask(mutation(
                    uid,
                    engine::record_change::Mutation::RetractAssertion {
                        assertion: id.into(),
                    },
                )),
            );
        }
    }
    for (name, title) in [
        ("predicate", "Assertion"),
        ("object", "Related Record (optional)"),
        ("amount", "Quantity (optional)"),
        ("unit", "Unit (optional)"),
    ] {
        input(world, parent, uid, name, title, "", false);
        if matches!(name, "predicate" | "object" | "unit") {
            button(
                world,
                parent,
                &format!("Find {title}"),
                Intent::Pick(crate::picker::Picker {
                    scope: uid.into(),
                    field: name.into(),
                    kind: if name == "object" {
                        crate::picker::Kind::Record
                    } else {
                        crate::picker::Kind::Concept
                    },
                }),
            );
        }
    }
    button(
        world,
        parent,
        "Add assertion",
        Intent::RecordControl(uid.into(), RecordControl::Assertion),
    );
    label(world, parent, &display(&record["assignees"]), 16.0);
    input(world, parent, uid, "assignee", "Person Record", "", false);
    button(
        world,
        parent,
        "Find Person",
        Intent::Pick(crate::picker::Picker {
            scope: uid.into(),
            field: "assignee".into(),
            kind: crate::picker::Kind::Person,
        }),
    );
    button(
        world,
        parent,
        "Assign",
        Intent::RecordControl(uid.into(), RecordControl::Assignee),
    );
}

fn logs(world: &mut World, parent: Entity, uid: &str, record: &Value) {
    label(world, parent, "Work logs", 22.0);
    for log in record["work_logs"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!("{} → {}", display(&log["start"]), display(&log["end"])),
            16.0,
        );
        if let Some(id) = log["id"].as_str() {
            button(
                world,
                parent,
                "Edit work log",
                Intent::EditLog(uid.into(), id.into()),
            );
            button(
                world,
                parent,
                "Remove log",
                Intent::Ask(mutation(
                    uid,
                    engine::record_change::Mutation::WorkLog {
                        log_id: id.into(),
                        value: None,
                    },
                )),
            );
        }
    }
    input(
        world,
        parent,
        uid,
        "log_start",
        "Start with timezone",
        "",
        false,
    );
    input(
        world,
        parent,
        uid,
        "log_end",
        "End with timezone (optional)",
        "",
        false,
    );
    button(
        world,
        parent,
        if world
            .resource::<Mobile>()
            .draft(uid, "log_id", "")
            .is_empty()
        {
            "Add work log"
        } else {
            "Save work log changes"
        },
        Intent::RecordControl(uid.into(), RecordControl::Log),
    );
    if !world
        .resource::<Mobile>()
        .draft(uid, "log_id", "")
        .is_empty()
    {
        button(
            world,
            parent,
            "Cancel work log changes",
            Intent::CancelLog(uid.into()),
        );
    }
}

pub fn record_control(world: &mut World, uid: &str, control: RecordControl) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    let value = |name| state.draft(uid, name, "");
    let optional = |name| {
        let value = value(name);
        (!value.trim().is_empty()).then(|| value.trim().to_string())
    };
    let action = match control {
        RecordControl::Assertion => mutation(
            uid,
            engine::record_change::Mutation::Assertion {
                predicate: value("predicate"),
                object: optional("object"),
                quantity: optional("amount"),
                unit: optional("unit"),
            },
        ),
        RecordControl::Assignee => mutation(
            uid,
            engine::record_change::Mutation::Assertion {
                predicate: "assigned-to".into(),
                object: Some(value("assignee")),
                quantity: None,
                unit: None,
            },
        ),
        RecordControl::Log => mutation(
            uid,
            engine::record_change::Mutation::WorkLog {
                log_id: optional("log_id")
                    .unwrap_or_else(|| format!("work.log:{}", nucleus::new_uid("log"))),
                value: Some(
                    serde_json::json!({"start":value("log_start"),"end":optional("log_end")}),
                ),
            },
        ),
        RecordControl::Message => Action::CreateMessage {
            thread: uid.into(),
            body: value("message"),
            content: state.attachments.get(uid).cloned().unwrap_or_default(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: optional("parent"),
            references: Vec::new(),
        },
    };
    act(world, action, None)
}

fn karma(world: &mut World, parent: Entity) {
    label(
        world,
        parent,
        "This app edits Karma. Automatic execution needs an active desktop Cell.",
        16.0,
    );
    button(world, parent, "New Karma", Intent::EditKarma(None));
    if let Some(draft) = world.resource::<Mobile>().karma.clone() {
        let scope = karma_scope(&draft);
        input(world, parent, &scope, "name", "Name", &draft.name, false);
        input(world, parent, &scope, "slug", "Slug", &draft.slug, false);
        for (index, title) in ["Condition", "Threshold", "Consequence"]
            .into_iter()
            .enumerate()
        {
            input(
                world,
                parent,
                &scope,
                &index.to_string(),
                title,
                &draft.fields[index].text,
                true,
            );
        }
        button(world, parent, "Save Karma", Intent::SaveKarma);
    }
    for row in rows(world, "karma") {
        let Ok(rule) = serde_json::from_value::<lince_interface::karma::Rule>(row) else {
            continue;
        };
        let group = controls::column(world, parent);
        label(
            world,
            group,
            &format!("{} · {}", rule.name, rule.state),
            20.0,
        );
        button(
            world,
            group,
            "Edit",
            Intent::EditKarma(Some(rule.uid.clone())),
        );
        button(
            world,
            group,
            "Delete",
            Intent::Ask(Action::DeleteRecurrence {
                recurrence: rule.uid,
            }),
        );
    }
}

fn karma_scope(draft: &lince_interface::karma::Draft) -> String {
    format!("karma/{}", draft.rule.as_deref().unwrap_or("new"))
}

pub fn edit_karma(world: &mut World, uid: Option<String>) -> Result<(), String> {
    let draft = match uid {
        Some(uid) => {
            let row = app::row(world, "karma", &uid).ok_or("Karma is no longer available")?;
            let rule = serde_json::from_value(row).map_err(|error| error.to_string())?;
            lince_interface::karma::Draft::from_rule(&rule)
        }
        None => default(),
    };
    world.resource_mut::<Mobile>().karma = Some(draft);
    Ok(())
}

pub fn save_karma(world: &mut World) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    let mut draft = state.karma.clone().ok_or("Open a Karma first")?;
    let scope = karma_scope(&draft);
    draft.name = state.draft(&scope, "name", &draft.name);
    draft.slug = state.draft(&scope, "slug", &draft.slug);
    for index in 0..3 {
        let field = &mut draft.fields[index];
        let text = state.draft(&scope, &index.to_string(), &field.text);
        if text != field.text {
            field.linked = None;
            field.text = text;
        }
    }
    if !draft.valid() || draft.name.trim().is_empty() || !nucleus::valid_slug(&draft.slug) {
        return Err("Enter a name and a valid slug".into());
    }
    act(
        world,
        Action::SaveKarmaRule {
            identity: Some(nucleus::karma::rule_field::RuleIdentity {
                name: draft.name,
                slug: draft.slug,
            }),
            rule: draft.rule,
            expected_revision: draft.revision,
            fields: draft.fields.map(|field| field.input()),
            request_id: nucleus::new_uid("karma-edit"),
        },
        None,
    )
}

fn frequency(world: &mut World, parent: Entity) {
    label(
        world,
        parent,
        "This app edits schedules. Automatic execution needs an active desktop Cell.",
        16.0,
    );
    button(world, parent, "New Frequency", Intent::EditFrequency(None));
    if let Some(draft) = world.resource::<Mobile>().frequency.clone() {
        let scope = frequency_scope(&draft);
        for (index, title) in ["Slug", "Purpose", "Interval", "Anchor with timezone"]
            .into_iter()
            .enumerate()
        {
            input(
                world,
                parent,
                &scope,
                &index.to_string(),
                title,
                &draft.fields[index],
                false,
            );
        }
        if draft.cadence_editable {
            label(
                world,
                parent,
                "Allowed weekdays (none means every day)",
                16.0,
            );
            let days = controls::row(world, parent);
            for day in lince_interface::frequency::interval::WEEKDAYS {
                let title = format!(
                    "{} {}",
                    if draft.weekdays.contains(&day) {
                        "✓"
                    } else {
                        ""
                    },
                    lince_interface::frequency::interval::weekday_label(day)
                );
                button(world, days, &title, Intent::Weekday(day));
            }
        }
        button(world, parent, "Save Frequency", Intent::SaveFrequency);
    }
    for row in rows(world, "frequency") {
        let Ok(frequency) = serde_json::from_value::<lince_interface::frequency::Frequency>(row)
        else {
            continue;
        };
        let group = controls::column(world, parent);
        label(world, group, &frequency.slug, 22.0);
        label(
            world,
            group,
            &lince_interface::frequency::schedule(&frequency),
            16.0,
        );
        button(
            world,
            group,
            "Edit",
            Intent::EditFrequency(Some(frequency.uid.clone())),
        );
        button(
            world,
            group,
            "Delete",
            Intent::Ask(Action::DeleteFrequency {
                frequency: frequency.uid,
            }),
        );
    }
}

fn frequency_scope(draft: &lince_interface::frequency::Draft) -> String {
    format!("frequency/{}", draft.uid.as_deref().unwrap_or("new"))
}

pub fn edit_frequency(world: &mut World, uid: Option<String>) -> Result<(), String> {
    let draft = match uid {
        Some(uid) => {
            let row =
                app::row(world, "frequency", &uid).ok_or("Frequency is no longer available")?;
            let frequency = serde_json::from_value(row).map_err(|error| error.to_string())?;
            lince_interface::frequency::Draft::edit(&frequency)
        }
        None => default(),
    };
    world.resource_mut::<Mobile>().frequency = Some(draft);
    Ok(())
}

pub fn save_frequency(world: &mut World) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    let mut draft = state.frequency.clone().ok_or("Open a Frequency first")?;
    let scope = frequency_scope(&draft);
    for index in 0..4 {
        draft.fields[index] = state.draft(&scope, &index.to_string(), &draft.fields[index]);
    }
    let frequency = draft.definition()?;
    act(
        world,
        Action::SaveKarmaFrequency {
            request_id: nucleus::new_uid("frequency-edit"),
            frequency_uid: draft.uid,
            expected_handle_revision: draft.revision,
            frequency,
            restart: false,
        },
        None,
    )
}
