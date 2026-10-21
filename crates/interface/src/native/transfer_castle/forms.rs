use super::*;
use model::FieldKind;
use ui::{Command, button, input, row};

pub(super) use crate::transfers::fields::{audience_fields, composer_fields};
use crate::transfers::fields::{choice, reference};

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
            } else if form.data["action"] == "set-transfer-private-application-policy" {
                effect_fields(&mut form);
            } else if form.data["action"] == "transfer-receipt"
                && form.data["request"]["kind"] == "prepare"
            {
                form.fields
                    .retain(|field| !field.path.starts_with("/request/scope/audience/"));
                audience_fields(&mut form, "/request/scope/audience");
            }
        }
        Err(error) => status(world, owner, error),
    }
    world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form.clone());
    let heading = row(world, container);
    crate::native::edit_mode::label(world, heading, &form.title, 20.0);
    if super::workflows::ACTIVITIES.contains(&form.preset.as_str()) {
        crate::native::edit_mode::label(
            world,
            container,
            "Choose the contribution, its unit and who provides it. A Need may request attention or participation so you can contribute. Private Record effects are reviewed separately; this draft grants no agreement, delivery or sharing.",
            13.0,
        );
    }
    if form.mode == "counteroffer-transfer" {
        crate::native::edit_mode::label(
            world,
            container,
            "Unavailable fields and private source settings stay unchanged.",
            13.0,
        );
    }
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
    if form.step == Some(3) {
        crate::native::edit_mode::label(
            world,
            container,
            "Public-history terms are optional and require agreement. They cover receipt fields and retention; live location and private bookkeeping stay separate.",
            13.0,
        );
        if form.data["public_history"].is_object() {
            button(
                world,
                container,
                owner,
                "Remove proposed public-history terms",
                Command::Remove("public_history".into(), 0),
            );
        } else {
            button(
                world,
                container,
                owner,
                "Add optional public-history terms",
                Command::Add("public_history".into()),
            );
        }
    }
    if form.data["action"] == "set-transfer-private-application-policy" {
        crate::native::edit_mode::label(world, container, "This outcome meets…", 16.0);
        crate::native::edit_mode::label(
            world,
            container,
            "Quantity changes add or deduct stock. Fulfilment meets an outstanding Need up to zero. These choices are private.",
            13.0,
        );
        let controls = row(world, container);
        button(
            world,
            controls,
            owner,
            "+ Record or Need",
            Command::Add("effects".into()),
        );
        for index in 0..array(&form.data, "effects").len() {
            button(
                world,
                controls,
                owner,
                &format!("Remove effect {}", index + 1),
                Command::Remove("effects".into(), index),
            );
        }
    }
    let organ = world.get::<View>(owner).unwrap().context["organ"]
        .as_str()
        .map(str::to_owned);
    for (index, field) in form.fields.iter().enumerate() {
        match &field.kind {
            FieldKind::OptionalChoice(choices) => {
                crate::native::edit_mode::label(world, container, &field.label, 12.0);
                let selected = choices
                    .iter()
                    .find(|(value, _)| value == &field.value)
                    .map(|(_, label)| label.as_str())
                    .unwrap_or("None");
                let mut options = vec![("None".into(), Command::Set(index, String::new()))];
                options.extend(
                    choices
                        .iter()
                        .map(|(value, label)| (label.clone(), Command::Set(index, value.clone()))),
                );
                ui::picker(world, container, owner, selected, options);
            }
            FieldKind::DataChoice(choices) => {
                crate::native::edit_mode::label(world, container, &field.label, 12.0);
                let selected = choices
                    .iter()
                    .find(|(value, _)| value.to_string() == field.value)
                    .map(|(_, label)| label.as_str())
                    .unwrap_or("Choose a Need");
                ui::picker(
                    world,
                    container,
                    owner,
                    selected,
                    choices
                        .iter()
                        .map(|(value, label)| {
                            (label.clone(), Command::Set(index, value.to_string()))
                        })
                        .collect(),
                );
            }
            FieldKind::Choice(choices) => {
                crate::native::edit_mode::label(world, container, &field.label, 12.0);
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
                    matches!(field.path.as_str(), "/body" | "/formula")
                        || field.path.ends_with("/item/description"),
                );
                if let FieldKind::Reference(kind) = &field.kind {
                    let choices: Vec<_> = records
                        .iter()
                        .filter(|record| match kind.as_str() {
                            "record" => {
                                record["transfer_picker_unit"] != true
                                    && (!matches!(field.path.as_str(), "/local_record" | "/record")
                                        || organ
                                            .as_deref()
                                            .is_some_and(|organ| record["organ"] == organ))
                            }
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
                if let FieldKind::Selections(options) = &field.kind {
                    let selected: Vec<_> = field
                        .value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .collect();
                    let choices = options
                        .iter()
                        .map(|(uid, label)| {
                            let included = selected.contains(&uid.as_str());
                            let mut values: Vec<_> = selected
                                .iter()
                                .copied()
                                .filter(|value| *value != uid)
                                .collect();
                            if !included {
                                values.push(uid);
                            }
                            (
                                format!("{} {label}", if included { "●" } else { "○" }),
                                Command::Set(index, values.join(", ")),
                            )
                        })
                        .collect();
                    ui::picker(world, container, owner, "Choose included items", choices);
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
        if step == 4 && form.mode != "counteroffer-transfer" {
            disclosure_preview(world, owner, container, &form.data, true);
        }
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
            review(world, container, &model::review_draft(&form), 0);
            for promise in array(&form.data, "promises") {
                button(
                    world,
                    container,
                    owner,
                    &format!("Simulate {}", text(&promise["item"], "title")),
                    Command::Simulate(text(promise, "uid")),
                );
            }
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
    } else if form.data["action"] == "apply-transfer-application" {
        if form.review.is_array() {
            review(world, container, &form.review, 0);
        }
        review(
            world,
            container,
            &json!({
                "Changes to my Record": form.data["expected_local_delta"],
                "Earlier changes for this delivery": form.data["expected_local_cumulative_before"],
                "Person": form.data["person"],
            }),
            0,
        );
        button(
            world,
            container,
            owner,
            "Apply all reviewed changes",
            Command::Submit,
        );
    } else if form.data["action"] == "record-journey-progress" {
        review(world, container, &form.review, 0);
        button(world, container, owner, &form.title, Command::Submit);
    } else if form.data["action"] == "transfer-receipt" {
        let mut visible_review = form.review.clone();
        if let Some(object) = visible_review.as_object_mut() {
            object.remove("Reviewed request");
        }
        review(world, container, &visible_review, 0);
        if form.data["request"]["kind"] == "authorize_participation" {
            button(
                world,
                container,
                owner,
                "Review current request",
                Command::ReviewParticipation,
            );
            if form.mode == "participation-reviewed" {
                button(
                    world,
                    container,
                    owner,
                    "Approve this participation",
                    Command::Submit,
                );
            }
        } else {
            button(world, container, owner, "Confirm", Command::Submit);
        }
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

pub(super) fn disclosure_preview(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    data: &Value,
    draft: bool,
) {
    crate::native::edit_mode::label(
        world,
        parent,
        "Field preview for people allowed to open this Transfer",
        13.0,
    );
    let controls = row(world, parent);
    button(
        world,
        controls,
        owner,
        "View as public",
        Command::ViewAs(None),
    );
    let mut people: std::collections::BTreeSet<String> = if draft {
        std::iter::once(text(data, "creator"))
            .chain(array(data, "invitees").iter().map(model::display))
            .collect()
    } else {
        array(data, "parties")
            .iter()
            .map(|party| text(party, "actor"))
            .collect()
    };
    for promise in array(data, "promises") {
        if let Ok(item) = serde_json::from_value::<nucleus::transfer::disclosure::TransferItem>(
            promise["item"].clone(),
        ) {
            for (_, audience) in item.disclosure.fields() {
                people.extend(audience.people.iter().cloned());
            }
        }
    }
    for person in people.into_iter().filter(|person| !person.is_empty()) {
        button(
            world,
            controls,
            owner,
            &format!("View as {person}"),
            Command::ViewAs(Some(person.clone())),
        );
    }
    if let Some(viewer) = world.get::<View>(owner).unwrap().disclosure_preview.clone() {
        let preview = model::disclosure_preview(data, viewer.as_deref(), draft);
        review(world, parent, &preview, 0);
    }
}

pub(super) fn review(world: &mut World, parent: Entity, value: &Value, depth: usize) {
    if depth > 6 {
        crate::native::edit_mode::label(world, parent, &display(value), 12.0);
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
                        crate::native::edit_mode::label(world, parent, &model::human(key), 14.0);
                        review(world, parent, value, depth + 1);
                    }
                    _ => {
                        crate::native::edit_mode::label(
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
            if values.len() > 150 {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    &format!(
                        "Showing the first 150 of {} entries in this summary. The complete artifact remains unchanged.",
                        values.len()
                    ),
                    12.0,
                );
            }
            for (index, value) in values.iter().enumerate().take(150) {
                if value.is_object() {
                    crate::native::edit_mode::label(world, parent, &format!("{}", index + 1), 12.0);
                }
                review(world, parent, value, depth + 1);
            }
        }
        _ => {
            crate::native::edit_mode::label(world, parent, &display(value), 12.0);
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
        crate::native::edit_mode::label(
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
        crate::native::edit_mode::label(
            world,
            panel,
            "This review is blocked or does not match the selected Person and quantity.",
            13.0,
        );
    }
}

pub(super) fn reviewed_payload(form: &Form, preview: &Value, actor: &str) -> Result<Value, String> {
    let action = crate::transfers::actions::reviewed_payload(form, preview, actor)?;
    serde_json::from_value::<engine::actions::Action>(action.clone())
        .map_err(|error| format!("Incomplete review: {error}"))?;
    Ok(action)
}

pub(super) fn effect_fields(form: &mut Form) {
    form.fields.clear();
    for index in 0..array(&form.data, "effects").len() {
        let prefix = format!("/effects/{index}");
        if form.data["effects"][index].get("private_ratio").is_none() {
            form.data["effects"][index]["private_ratio"] = Value::Null;
        }
        form.field(
            &format!("{prefix}/record"),
            &format!("Effect {} · My Record or Need", index + 1),
            reference("record"),
        );
        form.field(
            &format!("{prefix}/mode"),
            "How it changes",
            choice(&["quantity", "fulfilment"]),
        );
        form.field(
            &format!("{prefix}/private_ratio"),
            "Change per agreed unit (optional)",
            FieldKind::Optional,
        );
        form.field(
            &format!("{prefix}/formula"),
            "Formula (when the ratio is blank)",
            FieldKind::Text,
        );
    }
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
