use super::*;
use model::FieldKind;
use ui::{Command, button, input, row};

pub(super) use crate::transfers::fields::{audience_fields, composer_fields};
use crate::transfers::fields::{choice, reference};

mod fields;
mod presentation;
mod summary;

pub(super) use summary::render as review;

fn set_display(world: &mut World, entity: Entity, value: Display) {
    if let Some(mut node) = world.get_mut::<Node>(entity)
        && node.display != value
    {
        node.display = value;
    }
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let (Some(view), Some(castle)) = (world.get::<View>(owner), world.get::<TransferCastle>(owner))
    else {
        return;
    };
    let (container, transport) = (view.form, view.transport);
    let Some(body) = world.get::<ChildOf>(view.list).map(ChildOf::parent) else {
        return;
    };
    if world.get_entity(container).is_err() {
        return;
    }
    let has_form = castle.form.is_some();
    let in_transport = castle.workflow.workflow == crate::transfers::workspace::Workflow::Transport;
    if let Some(transport) = transport {
        set_display(
            world,
            transport,
            if !has_form && in_transport {
                Display::Flex
            } else {
                Display::None
            },
        );
    }
    set_display(
        world,
        body,
        if has_form || in_transport {
            Display::None
        } else {
            Display::Flex
        },
    );
    set_display(
        world,
        container,
        if has_form {
            Display::Flex
        } else {
            Display::None
        },
    );
    if let Some(mut node) = world.get_mut::<Node>(container)
        && node.flex_grow != 1.0
    {
        node.flex_grow = 1.0;
    }
    ui::clear(world, container);
    if !has_form {
        return;
    }
    let error = {
        let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
            return;
        };
        let Some(form) = castle.form.as_mut() else {
            return;
        };
        match form.commit_fields() {
            Ok(()) => {
                if form.step.is_some() {
                    composer_fields(form);
                } else if form.data["action"] == "set-transfer-private-application-policy" {
                    effect_fields(form);
                } else if form.data["action"] == "transfer-receipt"
                    && form.data["request"]["kind"] == "prepare"
                {
                    form.fields
                        .retain(|field| !field.path.starts_with("/request/scope/audience/"));
                    audience_fields(form, "/request/scope/audience");
                }
                None
            }
            Err(error) => Some(error),
        }
    };
    if let Some(error) = error {
        status(world, owner, error);
    }
    let (Some(form), Some(view)) = (
        world
            .get::<TransferCastle>(owner)
            .and_then(|castle| castle.form.as_ref()),
        world.get::<View>(owner),
    ) else {
        return;
    };
    let prepared = presentation::prepare(form, view);
    presentation::render(world, owner, container, prepared);
}

pub(super) fn disclosure_preview(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    data: &Value,
    draft: bool,
) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let prepared = presentation::Disclosure::prepare(data, view.disclosure_preview.as_ref(), draft);
    prepared.render(world, owner, parent);
}

pub(super) fn request_preview(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let (Some(castle), Some(view)) = (world.get::<TransferCastle>(owner), world.get::<View>(owner))
    else {
        return;
    };
    let Some(mut form) = castle.form.clone() else {
        return;
    };
    if let Err(error) = form.commit_fields() {
        status(world, owner, error);
        return;
    }
    let bulk = form.data["action"] == "complete-transfer-occurrence-claims-bulk";
    let query = if bulk {
        let occurrences: Vec<_> = view.selected_occurrences.iter().cloned().collect();
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
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    view.preview = None;
    match runtime::subscribe(world, owner, "preview", query) {
        Ok(id) => {
            let Ok((mut view, mut castle)) = world
                .query::<(&mut View, &mut TransferCastle)>()
                .get_mut(world, owner)
            else {
                runtime::cancel(world, owner, "preview");
                return;
            };
            view.preview_request = Some(id);
            castle.form = Some(form);
            status(world, owner, "Loading exact review from the Cell…");
            render(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

#[derive(Component)]
struct Preview;

pub(super) fn render_preview(world: &mut World, owner: Entity) {
    let (container, prepared) = {
        let _span = nucleus::performance::Span::new("transfer-preview-view-preparation");
        let (Some(form), Some(view)) = (
            world
                .get::<TransferCastle>(owner)
                .and_then(|castle| castle.form.as_ref()),
            world.get::<View>(owner),
        ) else {
            return;
        };
        let prepared = view.preview.as_ref().map(|raw| {
            let preview = if raw["settlement_preview"].is_object() {
                &raw["settlement_preview"]
            } else {
                raw
            };
            (
                summary::Summary::new(preview, 0),
                reviewed_payload(form, preview, &person(world, owner)).is_ok(),
            )
        });
        (view.form, prepared)
    };
    if world.get_entity(container).is_err() {
        return;
    }
    let old: Vec<_> = world
        .get::<Children>(container)
        .into_iter()
        .flat_map(|children| children.iter())
        .filter(|entity| world.get::<Preview>(*entity).is_some())
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let panel = ui::stack(world, container);
    world.entity_mut(panel).insert(Preview);
    let Some((review, acceptable)) = prepared else {
        crate::native::edit_mode::label(
            world,
            panel,
            "Request a fresh review before confirming.",
            13.0,
        );
        return;
    };
    review.render(world, panel);
    if acceptable {
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
    engine::actions::Action::deserialize(&action)
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
    let Some(mut form) = world
        .get::<TransferCastle>(owner)
        .and_then(|castle| castle.form.clone())
    else {
        return;
    };
    if let Err(error) = form.commit_fields() {
        status(world, owner, error);
        return;
    }
    let Some(raw) = world
        .get::<View>(owner)
        .and_then(|view| view.preview.as_ref())
    else {
        status(world, owner, "Request a fresh review");
        return;
    };
    let preview = if raw["settlement_preview"].is_object() {
        &raw["settlement_preview"]
    } else {
        raw
    };
    let result = reviewed_payload(&form, preview, &person(world, owner));
    match result {
        Ok(payload) => {
            let mut reviewed = Form::action("Confirm reviewed changes", payload);
            reviewed.mode = "reviewed".into();
            let Ok((mut view, mut castle)) = world
                .query::<(&mut View, &mut TransferCastle)>()
                .get_mut(world, owner)
            else {
                return;
            };
            castle.form = Some(reviewed);
            view.preview_request = None;
            runtime::cancel(world, owner, "preview");
            render(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

pub(super) fn bulk(world: &mut World, owner: Entity) {
    let (Some(castle), Some(view)) = (world.get::<TransferCastle>(owner), world.get::<View>(owner))
    else {
        return;
    };
    if view.selected_occurrences.is_empty() {
        status(world, owner, "Select occurrences in this branch first");
        return;
    }
    if castle.form.is_some() {
        status(world, owner, "Save or cancel the open form first");
        return;
    }
    let form = Form::action(
        "Review selected completion claims",
        json!({"action":"complete-transfer-occurrence-claims-bulk", "person":person(world, owner), "items":[], "review_token":""}),
    );
    let Some(mut castle) = world.get_mut::<TransferCastle>(owner) else {
        return;
    };
    castle.form = Some(form);
    request_preview(world, owner);
}

#[cfg(test)]
#[test]
fn open_draft_survives_removal_of_its_view() {
    let mut world = World::new();
    let form = model::composer("ana", "Donation");
    let owner = world
        .spawn(TransferCastle {
            form: Some(form.clone()),
            ..Default::default()
        })
        .id();

    render(&mut world, owner);
    request_preview(&mut world, owner);
    accept_review(&mut world, owner);
    bulk(&mut world, owner);
    runtime::submit(&mut world, owner);

    let castle = world.get::<TransferCastle>(owner).unwrap();
    assert_eq!(castle.form.as_ref(), Some(&form));
}
