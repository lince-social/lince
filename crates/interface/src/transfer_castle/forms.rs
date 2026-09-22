use super::*;
use model::FieldKind;
use ui::{Command, button, input, row};

fn choice(values: &[&str]) -> FieldKind {
    FieldKind::Choice(values.iter().map(|value| value.to_string()).collect())
}
fn reference(kind: &str) -> FieldKind {
    FieldKind::Reference(kind.into())
}

fn place_fields(form: &mut Form, prefix: &str) {
    if form.data.pointer(prefix).is_none_or(Value::is_null)
        && let Some(place) = form.data.pointer_mut(prefix)
    {
        *place = model::place();
    }
    form.field(
        &format!("{prefix}/address"),
        "Place / address",
        FieldKind::Optional,
    );
    form.field(
        &format!("{prefix}/lat"),
        "Latitude",
        FieldKind::OptionalNumber,
    );
    form.field(
        &format!("{prefix}/lon"),
        "Longitude",
        FieldKind::OptionalNumber,
    );
}

fn composer_fields(form: &mut Form) {
    form.fields.clear();
    match form.step.unwrap_or(0) {
        0 => {
            form.field("/head", "Title", FieldKind::Text);
            form.field("/slug", "Slug (optional)", FieldKind::Optional);
            form.field(
                "/agreement",
                "Agreement",
                choice(&["full", "individual", "percentage", "dependency"]),
            );
            if form.data["agreement"] == "percentage" {
                form.field("/agreement_pct", "Required percentage", FieldKind::Number);
            }
        }
        1 => {
            form.field("/creator", "Creator", reference("person"));
            form.field(
                "/invitees",
                "Invited People (comma-separated references)",
                FieldKind::People,
            );
        }
        2 => {
            for index in 0..array(&form.data, "promises").len() {
                let prefix = format!("/promises/{index}");
                form.field(
                    &format!("{prefix}/record"),
                    &format!("Promise {} · Record", index + 1),
                    reference("record"),
                );
                form.field(
                    &format!("{prefix}/open"),
                    "OPEN offer",
                    choice(&["false", "true"]),
                );
                if form.data["promises"][index]["open"] != true {
                    form.field(
                        &format!("{prefix}/party"),
                        "Responsible Person",
                        reference("person"),
                    );
                }
                form.field(
                    &format!("{prefix}/delta"),
                    "Quantity (negative gives, positive receives)",
                    FieldKind::Number,
                );
                form.field(
                    &format!("{prefix}/unit"),
                    "Unit (optional)",
                    reference("unit"),
                );
                form.field(
                    &format!("{prefix}/window_start"),
                    "Start (date, time, timezone)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/window_end"),
                    "Deadline (date, time, timezone)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/condition"),
                    "Condition",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/reserve_from"),
                    "Reserve from",
                    choice(&["inherit", "none", "proposed", "agreed", "active"]),
                );
                form.field(
                    &format!("{prefix}/reuse_policy"),
                    "OPEN reuse",
                    choice(&["duplicate", "consume"]),
                );
                place_fields(form, &format!("{prefix}/place"));
            }
        }
        3 => {
            form.field(
                "/visibility",
                "Who can discover this transfer",
                choice(&["hidden", "public", "proximity"]),
            );
            if form.data["visibility"] == "proximity" {
                form.field("/max_proximity", "Maximum proximity", FieldKind::Number);
            }
            form.field(
                "/reserve_default",
                "Default reservation",
                choice(&["inherit", "none", "proposed", "agreed", "active"]),
            );
            form.field(
                "/require_confirmation",
                "Require delivery and receipt confirmation",
                choice(&["true", "false"]),
            );
            form.field(
                "/satiation",
                "Completion policy",
                choice(&["none", "first_completes"]),
            );
            form.field("/parent", "Parent transfer", reference("transfer"));
            form.field("/source", "Shared source", reference("record"));
            place_fields(form, "/default_place");
            for index in 0..array(&form.data, "dependencies").len() {
                let prefix = format!("/dependencies/{index}");
                form.field(
                    &format!("{prefix}/scope"),
                    &format!("Dependency {} · Scope", index + 1),
                    choice(&["transfer", "promise"]),
                );
                form.field(
                    &format!("{prefix}/promise"),
                    "Dependent promise (for promise scope)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/upstream_kind"),
                    "Upstream kind",
                    choice(&["transfer", "promise"]),
                );
                form.field(
                    &format!("{prefix}/upstream"),
                    "Upstream transfer or promise",
                    reference("transfer"),
                );
                form.field(
                    &format!("{prefix}/required_state"),
                    "Required state",
                    choice(&[
                        "kept",
                        "active",
                        "agreed",
                        "open",
                        "proposed",
                        "broken",
                        "withdrawn",
                    ]),
                );
            }
        }
        _ => {}
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let (container, list) = (view.form, view.list);
    let body = world.get::<ChildOf>(list).unwrap().parent();
    let form = world.get::<TransferCastle>(owner).unwrap().form.clone();
    world.get_mut::<Node>(body).unwrap().display = if form.is_some() {
        Display::None
    } else {
        Display::Flex
    };
    world.get_mut::<Node>(container).unwrap().display = if form.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    ui::clear(world, container);
    let Some(mut form) = form else { return };
    match form.commit_fields() {
        Ok(()) => {
            if form.step.is_some() {
                composer_fields(&mut form);
            }
        }
        Err(error) => status(world, owner, error),
    }
    world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form.clone());
    let heading = row(world, container);
    crate::edit_mode::label(world, heading, &form.title, 20.0);
    button(world, heading, owner, "Cancel", Command::Cancel);
    if let Some(step) = form.step {
        let steps = row(world, container);
        for (index, label) in model::STEPS.iter().enumerate() {
            button(
                world,
                steps,
                owner,
                &format!(
                    "{}{}. {label}",
                    if step == index { "● " } else { "" },
                    index + 1
                ),
                Command::Step(index),
            );
        }
    }
    let records = world.get::<View>(owner).unwrap().records.clone();
    for (index, field) in form.fields.iter().enumerate() {
        match &field.kind {
            FieldKind::Choice(choices) => {
                crate::edit_mode::label(world, container, &field.label, 12.0);
                ui::picker(
                    world,
                    container,
                    owner,
                    &model::human(&field.value),
                    choices
                        .iter()
                        .map(|value| (model::human(value), Command::Set(index, value.clone())))
                        .collect(),
                );
            }
            _ => {
                input(
                    world,
                    container,
                    owner,
                    Some(index),
                    &field.label,
                    &field.value,
                    matches!(field.path.as_str(), "/body" | "/formula"),
                );
                if let FieldKind::Reference(kind) = &field.kind {
                    let choices: Vec<_> = records
                        .iter()
                        .filter(|record| match kind.as_str() {
                            "record" => record["transfer_picker_unit"] != true,
                            "unit" => record["transfer_picker_unit"] == true,
                            kind => record["kind"] == kind,
                        })
                        .filter(|record| {
                            field.value.is_empty()
                                || text(record, "uid") == field.value
                                || title(record)
                                    .to_lowercase()
                                    .contains(&field.value.to_lowercase())
                        })
                        .take(40)
                        .map(|record| (title(record), Command::Set(index, text(record, "uid"))))
                        .collect();
                    ui::picker(world, container, owner, "Choose by name", choices);
                    button(
                        world,
                        container,
                        owner,
                        "Find matching names",
                        Command::Find,
                    );
                }
                if matches!(field.kind, FieldKind::People) {
                    let existing: HashSet<_> = field.value.split(',').map(str::trim).collect();
                    let choices = records
                        .iter()
                        .filter(|record| {
                            record["kind"] == "person"
                                && !existing.contains(text(record, "uid").as_str())
                        })
                        .take(40)
                        .map(|record| {
                            let value = if field.value.trim().is_empty() {
                                text(record, "uid")
                            } else {
                                format!("{}, {}", field.value, text(record, "uid"))
                            };
                            (title(record), Command::Set(index, value))
                        })
                        .collect();
                    ui::picker(world, container, owner, "Add invited Person", choices);
                }
            }
        }
    }
    if let Some(step) = form.step {
        if step == 2 || step == 3 {
            let key = if step == 2 {
                "promises"
            } else {
                "dependencies"
            };
            let controls = row(world, container);
            button(
                world,
                controls,
                owner,
                if step == 2 {
                    "+ Promise"
                } else {
                    "+ Dependency"
                },
                Command::Add(key.into()),
            );
            for index in 0..array(&form.data, key).len() {
                button(
                    world,
                    controls,
                    owner,
                    &format!(
                        "Remove {} {}",
                        if step == 2 { "promise" } else { "dependency" },
                        index + 1
                    ),
                    Command::Remove(key.into(), index),
                );
            }
        }
        if step == 4 {
            review(world, container, &form.data, 0);
        }
        let controls = row(world, container);
        if step > 0 {
            button(world, controls, owner, "Back", Command::Step(step - 1));
        }
        if step < 4 {
            button(world, controls, owner, "Continue", Command::Step(step + 1));
        } else {
            button(
                world,
                controls,
                owner,
                "Sign and save transfer",
                Command::Submit,
            );
        }
    } else if form.mode != "reviewed"
        && matches!(
            text(&form.data, "action").as_str(),
            "settle-transfer-occurrence" | "complete-transfer-occurrence-claims-bulk"
        )
    {
        button(world, container, owner, "Refresh review", Command::Preview);
        render_preview(world, owner);
    } else {
        let review_data: Value = form
            .data
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter(|(key, _)| {
                        !["request_id", "action"].contains(&key.as_str())
                            && !form
                                .fields
                                .iter()
                                .any(|field| field.path == format!("/{key}"))
                    })
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<serde_json::Map<_, _>>()
            })
            .unwrap_or_default()
            .into();
        review(world, container, &review_data, 0);
        button(world, container, owner, "Confirm", Command::Submit);
    }
}

pub(super) fn review(world: &mut World, parent: Entity, value: &Value, depth: usize) {
    if depth > 6 {
        crate::edit_mode::label(world, parent, &display(value), 12.0);
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
                    continue;
                }
                match value {
                    Value::Object(_) | Value::Array(_) => {
                        crate::edit_mode::label(world, parent, &model::human(key), 14.0);
                        review(world, parent, value, depth + 1);
                    }
                    _ => {
                        crate::edit_mode::label(
                            world,
                            parent,
                            &format!("{}: {}", model::human(key), display(value)),
                            12.0,
                        );
                    }
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate().take(150) {
                if value.is_object() {
                    crate::edit_mode::label(world, parent, &format!("{}", index + 1), 12.0);
                }
                review(world, parent, value, depth + 1);
            }
        }
        _ => {
            crate::edit_mode::label(world, parent, &display(value), 12.0);
        }
    }
}

pub(super) fn request_preview(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let Some(mut form) = world.get::<TransferCastle>(owner).unwrap().form.clone() else {
        return;
    };
    if let Err(error) = form.commit_fields() {
        status(world, owner, error);
        return;
    }
    let bulk = form.data["action"] == "complete-transfer-occurrence-claims-bulk";
    let query = if bulk {
        let castle = world.get::<TransferCastle>(owner).unwrap();
        let occurrences: Vec<_> = world
            .get::<View>(owner)
            .unwrap()
            .selected_occurrences
            .iter()
            .cloned()
            .collect();
        json!({"source":"transfer_bulk_completion_preview", "where":[{"uid_eq":castle.selected}, {"occurrence_in":occurrences}]})
    } else {
        if !form.data["canonical_quantity"]
            .as_f64()
            .is_some_and(|v| v > 0.0 && v.is_finite())
        {
            status(world, owner, "Enter a positive settlement quantity");
            return;
        }
        json!({"source":"transfer_settlement_preview", "where":[{"uid_eq":form.data["occurrence"]},{"quantity_eq":form.data["canonical_quantity"]}]})
    };
    runtime::cancel(world, owner, "preview");
    world.get_mut::<View>(owner).unwrap().preview = None;
    match runtime::subscribe(world, owner, "preview", query) {
        Ok(id) => {
            world.get_mut::<View>(owner).unwrap().preview_request = Some(id);
            world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form);
            status(world, owner, "Loading exact review from the Cell…");
            render(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

#[derive(Component)]
struct Preview;

pub(super) fn render_preview(world: &mut World, owner: Entity) {
    let Some(form) = world.get::<TransferCastle>(owner).unwrap().form.clone() else {
        return;
    };
    let view = world.get::<View>(owner).unwrap();
    let container = view.form;
    let preview = view.preview.clone();
    let old: Vec<_> = world
        .query_filtered::<(Entity, &ChildOf), With<Preview>>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == container)
        .map(|(entity, _)| entity)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let panel = ui::stack(world, container);
    world.entity_mut(panel).insert(Preview);
    let Some(raw) = preview else {
        crate::edit_mode::label(
            world,
            panel,
            "Request a fresh review before confirming.",
            13.0,
        );
        return;
    };
    let preview = if raw["settlement_preview"].is_object() {
        &raw["settlement_preview"]
    } else {
        &raw
    };
    review(world, panel, preview, 0);
    if reviewed_payload(&form, preview, &person(world, owner)).is_ok() {
        button(
            world,
            panel,
            owner,
            "Use this review",
            Command::AcceptReview,
        );
    } else {
        crate::edit_mode::label(
            world,
            panel,
            "This review is blocked or does not match the selected Person and quantity.",
            13.0,
        );
    }
}

pub(super) fn reviewed_payload(form: &Form, preview: &Value, actor: &str) -> Result<Value, String> {
    let mut action = form.data.clone();
    if text(preview, "person") != actor || actor.is_empty() {
        return Err("This review belongs to a different Person. Choose the Person whose signer is installed.".into());
    }
    if action["action"] == "complete-transfer-occurrence-claims-bulk" {
        let items: Vec<_> = array(preview, "items")
            .iter()
            .filter_map(|item| {
                item["action"]
                    .as_object()
                    .map(|action| Value::Object(action.clone()))
            })
            .collect();
        if preview["eligible"] != true
            || items.is_empty()
            || items.len() != array(preview, "items").len()
            || text(preview, "review_token").is_empty()
        {
            return Err("The selection contains blocked or missing claims".into());
        }
        action["review_token"] = preview["review_token"].clone();
        action["items"] = json!(items);
    } else {
        if !capability(preview, "settle")
            || preview["canonical_quantity"].as_f64() != action["canonical_quantity"].as_f64()
            || preview["occurrence"] != action["occurrence"]
        {
            return Err("The quantity or occurrence changed. Request a fresh review.".into());
        }
        for (expected, actual) in [
            ("expected_remaining_quantity", "remaining_quantity"),
            ("expected_local_delta", "local_delta"),
            (
                "expected_application_formula_hash",
                "application_formula_hash",
            ),
            (
                "expected_application_formula_version",
                "application_formula_version",
            ),
            ("expected_remainder_policy", "remainder_policy"),
        ] {
            if preview[expected].is_null() || preview[expected] != preview[actual] {
                return Err("The Cell returned an incomplete settlement review".into());
            }
            action[expected] = preview[expected].clone();
        }
    }
    serde_json::from_value::<engine::actions::Action>(action.clone())
        .map_err(|error| format!("Incomplete review: {error}"))?;
    Ok(action)
}

pub(super) fn accept_review(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let Some(mut form) = world.get::<TransferCastle>(owner).unwrap().form.clone() else {
        return;
    };
    if let Err(error) = form.commit_fields() {
        status(world, owner, error);
        return;
    }
    let Some(raw) = world.get::<View>(owner).unwrap().preview.clone() else {
        status(world, owner, "Request a fresh review");
        return;
    };
    let preview = if raw["settlement_preview"].is_object() {
        &raw["settlement_preview"]
    } else {
        &raw
    };
    match reviewed_payload(&form, preview, &person(world, owner)) {
        Ok(payload) => {
            let mut reviewed = Form::action("Confirm reviewed changes", payload);
            reviewed.mode = "reviewed".into();
            world.get_mut::<TransferCastle>(owner).unwrap().form = Some(reviewed);
            world.get_mut::<View>(owner).unwrap().preview_request = None;
            runtime::cancel(world, owner, "preview");
            render(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

pub(super) fn bulk(world: &mut World, owner: Entity) {
    if world
        .get::<View>(owner)
        .unwrap()
        .selected_occurrences
        .is_empty()
    {
        status(world, owner, "Select occurrences in this branch first");
        return;
    }
    if world.get::<TransferCastle>(owner).unwrap().form.is_some() {
        status(world, owner, "Save or cancel the open form first");
        return;
    }
    let form = Form::action(
        "Review selected completion claims",
        json!({"action":"complete-transfer-occurrence-claims-bulk", "person":person(world, owner), "items":[], "review_token":""}),
    );
    world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form);
    request_preview(world, owner);
}
