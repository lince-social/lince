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
        Page::Organ => organ(world, parent),
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
    let records = rows(world, "records");
    if records.is_empty() {
        label(world, parent, "No Records to display", 18.0);
    }
    if kanban {
        for (column, (title, _, quantity)) in KANBAN_COLUMNS.iter().enumerate() {
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
        label(world, parent, "Other quantities", 22.0);
        for record in &records {
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
    let limit = world.resource::<Mobile>().limit;
    if records.len() >= limit && limit < 500 {
        button(world, parent, "Load more", Intent::More);
    }
    if records.len() >= 500 {
        label(
            world,
            parent,
            "Showing the first 500 Records. Narrow your search to see others.",
            16.0,
        );
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
    let detail = format!(
        "{} · {}",
        display(&record["quantity"]),
        display(&record["due_date"])
    );
    label(world, group, &detail, 14.0);
    if let Some(current) = column {
        button(world, group, "Move to…", Intent::MoveMenu(uid.into()));
        if world.resource::<Mobile>().moving.as_deref() != Some(uid) {
            return;
        }
        let moves = controls::row(world, group);
        for (index, (title, _, _)) in KANBAN_COLUMNS.iter().enumerate() {
            if current != index {
                button(world, moves, title, Intent::Move(uid.into(), index));
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
                label(
                    world,
                    parent,
                    message["body"].as_str().unwrap_or_default(),
                    16.0,
                );
            }
            input(world, parent, thread_uid, "message", "Message", "", true);
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
        "Add work log",
        Intent::RecordControl(uid.into(), RecordControl::Log),
    );
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
                log_id: format!("work.log:{}", nucleus::new_uid("log")),
                value: Some(
                    serde_json::json!({"start":value("log_start"),"end":optional("log_end")}),
                ),
            },
        ),
        RecordControl::Message => Action::CreateMessage {
            thread: uid.into(),
            body: value("message"),
            content: Vec::new(),
            author: None,
            state: nucleus::MessageState::Finished,
            parent: None,
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

fn organ(world: &mut World, parent: Entity) {
    input(world, parent, "organ", "invite", "Pairing code", "", true);
    input(world, parent, "organ", "name", "Contact name", "", false);
    button(world, parent, "Add known Organ", Intent::Pair);
    input(
        world,
        parent,
        "organ",
        "enrol",
        "Device enrolment code",
        "",
        true,
    );
    button(world, parent, "Join that Organ", Intent::JoinOrgan);
    button(
        world,
        parent,
        "Show devices",
        Intent::Act(Action::RosterStatus),
    );
    button(
        world,
        parent,
        "Create enrolment code",
        Intent::Act(Action::RosterEnrolToken),
    );
    for row in rows(world, "enrolment") {
        if let Some(code) = row["code"].as_str() {
            input(
                world,
                parent,
                "enrolment",
                "code",
                "Use this code on the other device",
                code,
                true,
            );
            label(
                world,
                parent,
                &format!("Expires in {} minutes", display(&row["expires_in_minutes"])),
                16.0,
            );
        }
    }
    for row in rows(world, "pairing") {
        if let Some(invite) = row["extension"]["invite"].as_str() {
            input(
                world,
                parent,
                "pairing",
                "code",
                "This Organ’s pairing code",
                invite,
                true,
            );
        }
    }
    for row in rows(world, "roster_records") {
        let roster = &row["extension"];
        label(
            world,
            parent,
            &format!("Roster expires {}", display(&roster["not_after"])),
            16.0,
        );
        for device in roster["cells"].as_array().into_iter().flatten() {
            let Some(uid) = device["cell_uid"].as_str() else {
                continue;
            };
            label(world, parent, device["label"].as_str().unwrap_or(uid), 18.0);
            button(
                world,
                parent,
                "Revoke device",
                Intent::Ask(Action::RosterRevokeCell {
                    cell_uid: uid.into(),
                }),
            );
        }
    }
    for row in rows(world, "organs") {
        let Some(uid) = row["uid"].as_str() else {
            continue;
        };
        label(world, parent, row["head"].as_str().unwrap_or(uid), 22.0);
        button(
            world,
            parent,
            "Open Organ",
            Intent::Open(Page::Record(uid.into())),
        );
        if row["slug"] == "local-organ" {
            continue;
        }
        for (title, trust) in [("Trust", "known"), ("Block", "blocked")] {
            button(
                world,
                parent,
                title,
                Intent::Ask(Action::SetContactTrust {
                    target: uid.into(),
                    trust: trust.into(),
                }),
            );
        }
        button(
            world,
            parent,
            "Forget contact",
            Intent::Ask(Action::ForgetOrganContact { target: uid.into() }),
        );
    }
}
