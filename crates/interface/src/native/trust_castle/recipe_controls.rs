use super::*;
use nucleus::trust_analysis::claims::recipes::{
    self, Mode,
    commands::{Command as Request, Review},
};

#[derive(Clone)]
pub(super) struct Draft {
    pub recipe: String,
    pub revision: String,
    pub permit_days: String,
    pub refresh: String,
    pub enabled: bool,
    pub review: Option<(Review, nucleus::karma::CanonicalHash)>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            recipe: String::new(),
            revision: "0".into(),
            permit_days: "30".into(),
            refresh: "60".into(),
            enabled: false,
            review: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Recipe,
    Revision,
    PermitDays,
    Refresh,
}
#[derive(Clone)]
pub(super) enum UiCommand {
    Preview,
    Confirm,
    List(Option<String>),
    Inspect(String),
    Recompute(String),
    Control(String, u64, Mode),
    Enabled,
    Revise(serde_json::Value),
}

pub(super) fn capture(draft: &mut Draft, key: Field, value: String) {
    match key {
        Field::Recipe => draft.recipe = value,
        Field::Revision => draft.revision = value,
        Field::PermitDays => draft.permit_days = value,
        Field::Refresh => draft.refresh = value,
    }
    draft.review = None;
}

pub(super) fn draft(view: &View) -> Result<recipes::commands::Draft, String> {
    let days: u32 = view
        .recipes
        .permit_days
        .parse()
        .map_err(|_| "Choose a permit term of 1–366 days")?;
    if !(1..=366).contains(&days) {
        return Err("Choose a permit term of 1–366 days".into());
    }
    Ok(recipes::commands::Draft {
        claim: claims::draft(view)?,
        recipe: (!view.recipes.recipe.is_empty()).then(|| view.recipes.recipe.clone()),
        expected_revision: view
            .recipes
            .revision
            .parse()
            .map_err(|_| "Choose the current recipe revision")?,
        inputs: analysis::build(view)?,
        permit_seconds: days * 86400,
        refresh_seconds: view
            .recipes
            .refresh
            .parse()
            .map_err(|_| "Choose a local refresh interval of 30–86400 seconds")?,
        start_enabled: view.recipes.enabled,
    })
}

pub(super) fn apply(world: &mut World, owner: Entity, command: &UiCommand) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let request = (|| -> Result<Option<Request>, String> {
        Ok(match command {
            UiCommand::Preview => Some(Request::Preview {
                draft: draft(view)?,
            }),
            UiCommand::Confirm => {
                let (review, hash) = view
                    .recipes
                    .review
                    .clone()
                    .ok_or("Review this exact publication permit first")?;
                Some(Request::Save {
                    review,
                    expected_review: hash,
                })
            }
            UiCommand::List(before) => Some(Request::List {
                before: before.clone(),
            }),
            UiCommand::Inspect(record) => Some(Request::Inspect {
                record: record.clone(),
            }),
            UiCommand::Recompute(record) => Some(Request::Recompute {
                record: record.clone(),
            }),
            UiCommand::Control(record, revision, mode) => Some(Request::Control {
                record: record.clone(),
                expected_revision: *revision,
                mode: *mode,
            }),
            _ => None,
        })
    })();
    match request {
        Ok(Some(request)) => {
            if let Err(error) = super::request(
                world,
                owner,
                Backend::Claims {
                    request: nucleus::trust_analysis::claims::commands::Command::Recipes {
                        request,
                    },
                },
            ) {
                world.get_mut::<View>(owner).unwrap().notice = error;
            }
        }
        Err(error) => world.get_mut::<View>(owner).unwrap().notice = error,
        Ok(None) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.generation = view.generation.wrapping_add(1);
            view.recipes.review = None;
            view.claims.review = None;
            view.review = None;
            view.result = Value::Null;
            match command {
                UiCommand::Enabled => view.recipes.enabled = !view.recipes.enabled,
                UiCommand::Revise(value) => {
                    if let Ok(recipe) = serde_json::from_value::<recipes::Recipe>(value.clone()) {
                        let definition = recipe.definition;
                        analysis::load(&mut view, definition.inputs);
                        view.claims.title = definition.terms.title;
                        view.claims.claim = definition.terms.claim;
                        view.claims.metric = match definition.terms.descriptor.metric {
                            nucleus::trust_analysis::claims::Metric::Quantity => 0,
                            nucleus::trust_analysis::claims::Metric::ReportingOccurrences => 1,
                            nucleus::trust_analysis::claims::Metric::PositiveOccurrences => 2,
                        };
                        view.claims.exact = definition.terms.comparator
                            == nucleus::trust_analysis::claims::Comparator::Exact;
                        view.issuer_organ = matches!(definition.terms.issuer, Identity::Organ(_));
                        view.public = matches!(definition.terms.audience, Audience::Public);
                        view.readers = match definition.terms.audience {
                            Audience::Public => String::new(),
                            Audience::Selected { viewers } => viewers
                                .iter()
                                .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                                .collect::<Vec<_>>()
                                .join(", "),
                        };
                        view.term = (definition.terms.lifetime_seconds / 86400)
                            .max(1)
                            .to_string();
                        view.recipes.recipe = definition.recipe;
                        view.recipes.revision = definition.revision.to_string();
                        view.recipes.enabled = false;
                    }
                }
                _ => {}
            }
        }
    }
    super::render(world, owner);
}

fn button(world: &mut World, owner: Entity, label: &str, command: UiCommand) {
    panel::button(world, owner, owner, label, Command::Recipe(command));
}

fn source_conditions(conditions: &[nucleus::trust_analysis::Condition]) -> String {
    if conditions.is_empty() {
        return "none configured".into();
    }
    conditions
        .iter()
        .map(|condition| {
            if condition.identities.is_empty() {
                "any permitted identity".into()
            } else {
                condition
                    .identities
                    .iter()
                    .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                    .collect::<Vec<_>>()
                    .join(" or ")
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let draft = view.recipes.clone();
    let result = view.result.clone();
    label(world, owner, "Reviewed automatic local summaries");
    label(
        world,
        owner,
        "Reuse the claim's activity, issuer, audience and output term above. Only publication-approved receipt evidence enters this summary. Private Transfers, accounting, contact paths and known/proximity criteria are excluded. No remote Ask around runs automatically.",
    );
    for (key, caption, value) in [
        (
            Field::Recipe,
            "Recipe identity (empty for a new recipe)",
            draft.recipe.as_str(),
        ),
        (
            Field::Revision,
            "Current recipe revision",
            draft.revision.as_str(),
        ),
        (
            Field::PermitDays,
            "Signing permit lifetime in days",
            draft.permit_days.as_str(),
        ),
        (
            Field::Refresh,
            "Local refresh interval in seconds",
            draft.refresh.as_str(),
        ),
    ] {
        field(world, owner, owner, Key::Recipe(key), caption, value);
    }
    button(
        world,
        owner,
        if draft.enabled {
            "Start automatic updates after approval"
        } else {
            "Save paused after approval"
        },
        UiCommand::Enabled,
    );
    button(
        world,
        owner,
        "Review exact automatic-publication scope",
        UiCommand::Preview,
    );
    if let Some((review, _)) = &draft.review {
        let terms = &review.recipe.definition.terms;
        let inputs = &review.recipe.definition.inputs;
        label(
            world,
            owner,
            &format!(
                "Actual reviewed source policy: all-visible counting {} · included issuers {} · excluded issuers {} · included counterparties {} · excluded counterparties {} · exclude everywhere {}",
                inputs.rely_on_all_visible,
                source_conditions(&inputs.issuers.include),
                source_conditions(&inputs.issuers.exclude),
                source_conditions(&inputs.counterparties.include),
                source_conditions(&inputs.counterparties.exclude),
                inputs
                    .exclude_everywhere
                    .iter()
                    .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Output {:?} · role {:?} · evidence {:?} · {:?} comparator · counterparty basis {:?} · period {:?} to {:?}. Raw evidence is published; policy-weighted indices are not.",
                terms.descriptor.metric,
                terms.descriptor.role,
                terms.descriptor.level,
                terms.comparator,
                terms.descriptor.counterparty_basis,
                terms.descriptor.from,
                terms.descriptor.until
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Permit for {}:{} to update claim {} about {}:{} until {}. Each output lasts at most {} seconds. Starting claim revision {}.",
                terms.issuer.kind(),
                terms.issuer.uid(),
                terms.claim,
                terms.descriptor.subject.kind(),
                terms.descriptor.subject.uid(),
                terms.expires_at,
                terms.lifetime_seconds,
                review.recipe.definition.starting_claim_revision
            ),
        );
        label(
            world,
            owner,
            &analysis::measurement_caption(&terms.descriptor.measurement),
        );
        label(
            world,
            owner,
            &format!(
                "Approved audience: {}. Public signing permit identifies Cell {} of Organ {}, key {}. Private controller and input policy remain outside the generated document.",
                serde_json::to_string(&terms.audience).unwrap_or_default(),
                terms.delegate.cell,
                terms.delegate.organ,
                terms.delegate.public_key
            ),
        );
        label(
            world,
            owner,
            "The publisher signs a bounded calculation statement. Readers still choose their own sources and check original evidence. Unavailable inputs produce an explicit withdrawal/status rather than an invented zero.",
        );
        button(
            world,
            owner,
            "Sign and save this exact publication permit",
            UiCommand::Confirm,
        );
    }
    button(
        world,
        owner,
        "My private publication recipes",
        UiCommand::List(None),
    );
    if result["kind"] == "recipe_list" {
        for entry in result["entries"].as_array().into_iter().flatten() {
            if let Some(record) = entry["record"].as_str() {
                label(
                    world,
                    owner,
                    &format!(
                        "{} · {} · {}",
                        entry["recipe"]["definition"]["terms"]["title"]
                            .as_str()
                            .unwrap_or("Publication recipe"),
                        entry["state"]["mode"].as_str().unwrap_or_default(),
                        entry["state"]["status"].as_str().unwrap_or("pending")
                    ),
                );
                button(
                    world,
                    owner,
                    "Inspect recipe and update status",
                    UiCommand::Inspect(record.into()),
                );
            }
        }
        if let Some(next) = result["next"].as_str() {
            button(
                world,
                owner,
                "More private recipes",
                UiCommand::List(Some(next.into())),
            );
        }
    }
    if result["kind"] == "recipe_document"
        && let Some(record) = result["record"].as_str()
    {
        if result["authority_current"] == false {
            label(
                world,
                owner,
                "Current publication authority is withheld. The previous signed output remains historical evidence; a fresh authorized review is needed for publication.",
            );
        }
        if result["current"] == false {
            label(
                world,
                owner,
                "This recipe has been superseded and cannot publish new outputs.",
            );
        }
        button(
            world,
            owner,
            "Refresh local publication status",
            UiCommand::Inspect(record.into()),
        );
        label(
            world,
            owner,
            &format!(
                "Automatic summary: {} · {} · checked {} · next {}",
                result["state"]["mode"].as_str().unwrap_or_default(),
                result["state"]["status"].as_str().unwrap_or("pending"),
                result["state"]["checked_at"],
                result["state"]["next_at"]
            ),
        );
        button(
            world,
            owner,
            "Recompute from local permitted public evidence",
            UiCommand::Recompute(record.into()),
        );
        if let Some(revision) = result["state"]["revision"].as_u64() {
            if result["state"]["mode"] != "revoked" {
                button(
                    world,
                    owner,
                    "Pause automatic publication",
                    UiCommand::Control(record.into(), revision, Mode::Paused),
                );
                button(
                    world,
                    owner,
                    "Enable the reviewed automatic publication",
                    UiCommand::Control(record.into(), revision, Mode::Enabled),
                );
                button(
                    world,
                    owner,
                    "Revoke this recipe and withdraw its output",
                    UiCommand::Control(record.into(), revision, Mode::Revoked),
                );
            }
        }
        if let Some(output) = result["state"]["output"].as_str() {
            panel::button(
                world,
                owner,
                owner,
                "Review sharing this recipe's changing outputs",
                super::serving_controls::Open {
                    source: record.into(),
                    kind: nucleus::trust_analysis::serving::SourceKind::Claim,
                    scope: nucleus::trust_analysis::serving::Scope::Recipe,
                },
            );
            panel::button(
                world,
                owner,
                owner,
                "Inspect the last signed output",
                Command::Claim(claims::UiCommand::Inspect(output.into())),
            );
        }
        button(
            world,
            owner,
            "Prepare a fresh reviewed recipe revision",
            UiCommand::Revise(result["recipe"].clone()),
        );
        panel::button(
            world,
            owner,
            owner,
            "Open private recipe Record and visibility",
            Command::Record(record.into()),
        );
    }
}
