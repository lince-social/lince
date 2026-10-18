use super::*;
use model::FieldKind;
use nucleus::transfer::surplus::{Command as Request, Definition, Mode, Recipe, Review, Saved};

#[path = "stock_tests.rs"]
pub(super) mod tests;

pub(super) fn current(world: &World, owner: Entity, form: &Form) -> bool {
    let Some(castle) = world.get::<TransferCastle>(owner) else {
        return false;
    };
    form.actor.as_deref() == Some(person(world, owner).as_str())
        && form.transfer.as_deref() == Some(castle.selected.as_str())
        && castle.form.as_ref() == Some(form)
}

pub(super) fn clear_form(world: &mut World, owner: Entity) {
    if world
        .get::<TransferCastle>(owner)
        .and_then(|castle| castle.form.as_ref())
        .is_some_and(|form| form.data["action"] == "surplus")
    {
        world.get_mut::<TransferCastle>(owner).unwrap().form = None;
        world.get_mut::<View>(owner).unwrap().notice = None;
        forms::render(world, owner);
    }
}

fn same_target(left: &Definition, right: &Definition) -> bool {
    left.controller_uid == right.controller_uid
        && left.stock_uid == right.stock_uid
        && match (&left.recipe, &right.recipe) {
            (
                Recipe::Offer {
                    transfer_uid: a,
                    promise_uid: b,
                    ..
                },
                Recipe::Offer {
                    transfer_uid: c,
                    promise_uid: d,
                    ..
                },
            ) => a == c && b == d,
            (Recipe::Replenish { .. }, Recipe::Replenish { .. }) => true,
            _ => false,
        }
}

#[derive(Clone)]
struct OpenRule {
    person: String,
    transfer: String,
    rule: String,
}

#[derive(Clone)]
struct PauseReview {
    expected: Form,
    confirmation: Form,
}

impl Action for PauseReview {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        ui::capture(world, owner);
        if !current(world, owner, &self.expected)
            || world
                .get::<View>(owner)
                .is_none_or(|view| view.pending.is_some())
        {
            return;
        }
        world.get_mut::<TransferCastle>(owner).unwrap().form = Some(self.confirmation.clone());
        forms::render(world, owner);
    }
}

impl Action for OpenRule {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner)
            || person(world, owner) != self.person
            || world
                .get::<TransferCastle>(owner)
                .is_none_or(|castle| castle.selected != self.transfer)
            || world
                .get::<View>(owner)
                .is_none_or(|view| view.pending.is_some())
        {
            return;
        }
        crate::native::karma_castle::open_rule(world, owner, &self.rule);
    }
}

pub(super) fn extra_controls(world: &mut World, owner: Entity, parent: Entity, editing: &Form) {
    let Ok(saved) = serde_json::from_value::<Saved>(editing.review["Saved stock policy"].clone())
    else {
        return;
    };
    if saved.review.definition.controller_uid != person(world, owner) {
        return;
    }
    if let Some(rule) = saved.rule_uid.clone() {
        crate::native::castle_feed::button(
            world,
            parent,
            owner,
            "Open stock planning rule",
            OpenRule {
                person: person(world, owner),
                transfer: editing.transfer.clone().unwrap_or_default(),
                rule,
            },
        );
    }
    if saved.enabled {
        let mut pause = form(
            world,
            owner,
            "Pause stock planning",
            Request::Pause {
                policy_uid: saved.review.definition.uid,
                controller_uid: saved.review.definition.controller_uid,
                expected_revision: saved.revision,
            },
        );
        pause.review = json!({"Pause":"Stop automatic stock planning. Existing Transfer terms, reservations and deliveries remain governed by their normal controls."});
        crate::native::castle_feed::button(
            world,
            parent,
            owner,
            "Pause stock policy",
            PauseReview {
                expected: editing.clone(),
                confirmation: pause,
            },
        );
    }
}

fn form(world: &World, owner: Entity, title: &str, request: Request) -> Form {
    let mut form = Form::action(title, json!({"action":"surplus","request":request}));
    form.actor = Some(person(world, owner));
    form.transfer = Some(world.get::<TransferCastle>(owner).unwrap().selected.clone());
    form.revision = world
        .get::<View>(owner)
        .and_then(|view| {
            view.rows
                .iter()
                .find(|row| row["uid"].as_str() == form.transfer.as_deref())
        })
        .and_then(|row| row["revision"].as_u64());
    form
}

fn definition(world: &World, owner: Entity, stock: &str, recipe: Recipe) -> Definition {
    Definition {
        uid: nucleus::new_uid("r"),
        controller_uid: person(world, owner),
        stock_uid: stock.into(),
        recipe,
    }
}

pub(super) fn controls(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    let person = person(world, owner);
    if person.is_empty() {
        return;
    }
    let mut replenish = form(
        world,
        owner,
        "Choose stock for replenishment planning",
        Request::Inspect {
            definition: definition(
                world,
                owner,
                "",
                Recipe::Replenish {
                    below: "7".into(),
                    target: "14".into(),
                    upstream_transfer_uid: text(transfer, "uid"),
                },
            ),
        },
    );
    replenish.field(
        "/request/definition/stock_uid",
        "My stock Record",
        FieldKind::Reference("record".into()),
    );
    replenish.review = json!({"Planning":"Choose the stock to inspect. This creates no Need and changes no inventory."});
    ui::button(
        world,
        parent,
        owner,
        "Plan stock replenishment",
        ui::Command::Open(replenish),
    );
    if !capability(transfer, "edit_terms") {
        return;
    }
    for promise in array(transfer, "promises") {
        let stock = text(promise, "record");
        if text(promise, "party") != person
            || !nucleus::valid_uid(&stock, "r")
            || promise["delta"].as_f64().is_none_or(|amount| amount >= 0.0)
        {
            continue;
        }
        let request = Request::Inspect {
            definition: definition(
                world,
                owner,
                &stock,
                Recipe::Offer {
                    transfer_uid: text(transfer, "uid"),
                    promise_uid: text(promise, "uid"),
                    retain: "0".into(),
                    mode: Mode::RecomputeOffer,
                },
            ),
        };
        let offer = form(world, owner, "Inspect stock and surplus", request);
        ui::button(
            world,
            parent,
            owner,
            &format!("Stock policy · {}", title(promise)),
            ui::Command::Open(offer),
        );
    }
}

fn summary(preview: &Value) -> Value {
    json!({"Stock":preview["actual"],"Available before reservations":preview["available"],"Already reserved":preview["reserved"],"This offer's reservation":preview["reserved_offer"],"Current offer":preview["offered"],"Proposed quantity":preview["proposed"],"Planned for myself":preview["personal"],"Stock shortage":preview["shortfall"],"State":preview["reason"]})
}

fn configuration(
    world: &World,
    owner: Entity,
    definition: Definition,
    preview: &Value,
    saved: &Value,
) -> Form {
    let mut form = form(
        world,
        owner,
        "Review stock planning",
        Request::Review { definition },
    );
    match form.data["request"]["definition"]["recipe"]["kind"].as_str() {
        Some("offer") => {
            form.field(
                "/request/definition/recipe/retain",
                "Personal planning target",
                FieldKind::Text,
            );
            form.field(
                "/request/definition/recipe/mode",
                "When stock changes",
                FieldKind::OptionalChoice(vec![
                    (
                        "recompute_offer".into(),
                        "Keep personal target and recompute offer".into(),
                    ),
                    (
                        "preserve_offer".into(),
                        "Keep offer and reduce personal availability".into(),
                    ),
                ]),
            );
        }
        Some("replenish") => {
            form.field(
                "/request/definition/recipe/below",
                "Propose below this stock level",
                FieldKind::Text,
            );
            form.field(
                "/request/definition/recipe/target",
                "Replenish up to",
                FieldKind::Text,
            );
            form.field(
                "/request/definition/recipe/upstream_transfer_uid",
                "Upstream farmer/supplier Transfer",
                FieldKind::Reference("transfer".into()),
            );
        }
        _ => {}
    }
    form.review = summary(preview);
    form.review["Existing policy"] = json!({"enabled":saved["enabled"],"reason":saved["reason"]});
    form.review["Saved stock policy"] = saved.clone();
    form.review["Review"] = json!(
        "Review recalculates stock and reservations before Enable. Planning creates no delivery or experience credit."
    );
    form
}

pub(super) fn complete(world: &mut World, owner: Entity, pending: &Form, data: Option<&Value>) {
    let Some(stock) = data.and_then(|value| value.get("stock")) else {
        return;
    };
    let request = pending.data["request"]["kind"].as_str().unwrap_or_default();
    let result = (|| -> Result<Option<Form>, String> {
        match request {
            "inspect" => {
                let definition: Definition = serde_json::from_value(stock["definition"].clone())
                    .map_err(|error| error.to_string())?;
                let expected: Definition =
                    serde_json::from_value(pending.data["request"]["definition"].clone())
                        .map_err(|error| error.to_string())?;
                if Some(definition.controller_uid.as_str()) != pending.actor.as_deref()
                    || !same_target(&expected, &definition)
                {
                    return Err(
                        "Stock response belongs to another Person or stock selection".into(),
                    );
                }
                definition.validate()?;
                Ok(Some(configuration(
                    world,
                    owner,
                    definition,
                    &stock["preview"],
                    &stock["saved"],
                )))
            }
            "review" => {
                let review: Review = serde_json::from_value(stock["review"].clone())
                    .map_err(|error| error.to_string())?;
                let commitment = stock["commitment"]
                    .as_str()
                    .ok_or("Stock review is missing its commitment")?
                    .to_owned();
                let expected: Definition =
                    serde_json::from_value(pending.data["request"]["definition"].clone())
                        .map_err(|error| error.to_string())?;
                let mut normalized = expected.clone();
                normalized.uid = review.definition.uid.clone();
                if normalized != review.definition
                    || Some(review.definition.controller_uid.as_str()) != pending.actor.as_deref()
                    || review.commitment()? != commitment
                {
                    return Err("Stock inputs changed; review the current configuration".into());
                }
                let mut enabled = form(
                    world,
                    owner,
                    "Enable reviewed stock policy",
                    Request::Enable {
                        review: Box::new(review.clone()),
                        commitment,
                    },
                );
                enabled.review = summary(&json!(review.preview));
                enabled.review["Stock Record"] = json!(review.definition.stock_uid);
                enabled.review["Stock unit"] = json!(review.unit_uid);
                enabled.review["Agreement"] = json!(
                    "Approved amounts remain frozen. Unaccepted changes use ordinary signed revisions."
                );
                enabled.review["If surplus becomes zero"] = json!(
                    "Recompute mode withdraws the unaccepted bundle, including its return promises."
                );
                Ok(Some(enabled))
            }
            "enable" | "pause" => {
                world.get_mut::<View>(owner).unwrap().notice=Some("Stock policy saved. Use its stock controls to inspect current availability and its normal Karma rule.".into());
                Ok(None)
            }
            _ => Ok(None),
        }
    })();
    match result {
        Ok(Some(form)) => world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form),
        Ok(None) => {}
        Err(error) => {
            world.get_mut::<View>(owner).unwrap().notice = Some(error.clone());
            status(world, owner, error);
        }
    }
}
