use super::*;
use nucleus::trust_analysis::claims::{self, Comparator, Metric, commands::Command as Request};

#[derive(Clone)]
pub(super) struct Draft {
    pub category: claims::Category,
    pub title: String,
    pub amount: String,
    pub claim: String,
    pub revision: String,
    pub as_of: String,
    pub exact: bool,
    pub metric: usize,
    pub withdrawn: bool,
    pub import: String,
    pub review: Option<(claims::commands::Review, nucleus::karma::CanonicalHash)>,
    pub snapshot: Option<claims::Document>,
    pub checks: Vec<Value>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            category: Default::default(),
            title: "Contribution history claim".into(),
            amount: String::new(),
            claim: String::new(),
            revision: "0".into(),
            as_of: String::new(),
            exact: false,
            metric: 0,
            withdrawn: false,
            import: String::new(),
            review: None,
            snapshot: None,
            checks: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Title,
    Amount,
    Claim,
    Revision,
    AsOf,
    Import,
}

#[derive(Clone)]
pub(super) enum UiCommand {
    Preview,
    Confirm,
    Import,
    List(Option<String>),
    Inspect(String),
    Export(String),
    Check(String),
    CheckSnapshot,
    InspectSnapshot,
    Exact,
    Metric,
    Category,
    Withdrawn,
    Revise(Value, bool),
}

pub(super) fn capture(draft: &mut Draft, key: Field, value: String) {
    match key {
        Field::Title => draft.title = value,
        Field::Amount => draft.amount = value,
        Field::Claim => draft.claim = value,
        Field::Revision => draft.revision = value,
        Field::AsOf => draft.as_of = value,
        Field::Import => draft.import = value,
    }
    draft.review = None;
}

fn descriptor(view: &View) -> Result<claims::Descriptor, String> {
    let query = analysis::build(view)?;
    if query.measurements.len() != 1 || query.levels.len() != 1 {
        return Err("Select one exact activity definition and one evidence class in the analysis controls first".into());
    }
    Ok(claims::Descriptor {
        subject: query.subject,
        role: query.role,
        measurement: query.measurements[0].clone(),
        level: query.levels[0],
        metric: match view.claims.metric {
            1 => Metric::ReportingOccurrences,
            2 => Metric::PositiveOccurrences,
            _ => Metric::Quantity,
        },
        counterparty_basis: query.counterparty_basis,
        from: query.from,
        until: query.until,
    })
}

pub(super) fn draft(view: &View) -> Result<claims::commands::Draft, String> {
    let source = &view.claims;
    let mut audience = identities(&view.readers)?;
    audience.push(Identity::Person(view.person.clone()));
    audience.sort();
    audience.dedup();
    let days: u32 = view
        .term
        .parse()
        .map_err(|_| "Choose a claim term of 1–366 days")?;
    if days == 0 || days > 366 {
        return Err("Choose a claim term of 1–366 days".into());
    }
    let as_of = if source.as_of.trim().is_empty() {
        None
    } else {
        Some(
            chrono::DateTime::parse_from_rfc3339(&source.as_of)
                .map_err(|_| "Use a date and timezone such as 2026-10-01T00:00:00Z")?
                .timestamp(),
        )
    };
    Ok(claims::commands::Draft {
        category: source.category,
        issuer: if view.issuer_organ {
            Identity::Organ(view.context["organ"].as_str().unwrap_or_default().into())
        } else {
            Identity::Person(view.person.clone())
        },
        claim: (!source.claim.trim().is_empty()).then(|| source.claim.clone()),
        title: source.title.clone(),
        descriptor: descriptor(view)?,
        comparator: if source.exact {
            Comparator::Exact
        } else {
            Comparator::AtLeast
        },
        amount: source.amount.clone(),
        as_of,
        lifetime_seconds: days * 86400,
        expected_revision: source
            .revision
            .parse()
            .map_err(|_| "Choose the current claim revision")?,
        withdrawn: source.withdrawn,
        audience: if view.public {
            Audience::Public
        } else {
            Audience::Selected { viewers: audience }
        },
        key_id: String::new(),
        public_key: String::new(),
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
                let (review, expected_review) = view
                    .claims
                    .review
                    .clone()
                    .ok_or("Preview the exact claim before signing")?;
                Some(Request::Publish {
                    review,
                    expected_review,
                })
            }
            UiCommand::Import => Some(Request::Import {
                document: view.claims.import.clone(),
                as_organ: view.as_organ,
            }),
            UiCommand::List(before) => {
                let query = analysis::build(view)?;
                Some(Request::List {
                    subject: query.subject,
                    before: before.clone(),
                    as_organ: view.as_organ,
                })
            }
            UiCommand::Inspect(record) => Some(Request::Inspect {
                record: record.clone(),
                as_organ: view.as_organ,
            }),
            UiCommand::Export(record) => Some(Request::Export {
                record: record.clone(),
                as_organ: view.as_organ,
            }),
            UiCommand::Check(record) => Some(Request::Check {
                record: record.clone(),
                sources: analysis::build(view)?,
                as_organ: view.as_organ,
            }),
            UiCommand::InspectSnapshot => Some(Request::InspectDocument {
                document: serde_json::to_string(
                    view.claims
                        .snapshot
                        .as_ref()
                        .ok_or("Select a profile claim first")?,
                )
                .map_err(|error| error.to_string())?,
                as_organ: view.as_organ,
            }),
            UiCommand::CheckSnapshot => Some(Request::CheckDocument {
                document: serde_json::to_string(
                    view.claims
                        .snapshot
                        .as_ref()
                        .ok_or("Select a profile claim first")?,
                )
                .map_err(|error| error.to_string())?,
                sources: analysis::build(view)?,
                as_organ: view.as_organ,
            }),
            _ => None,
        })
    })();
    match request {
        Ok(Some(command)) => {
            if let Err(error) = super::request(world, owner, Backend::Claims { request: command }) {
                world.get_mut::<View>(owner).unwrap().notice = error;
            }
        }
        Err(error) => world.get_mut::<View>(owner).unwrap().notice = error,
        Ok(None) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.generation = view.generation.wrapping_add(1);
            view.claims.review = None;
            view.recipes.review = None;
            view.review = None;
            view.result = Value::Null;
            match command {
                UiCommand::Exact => view.claims.exact = !view.claims.exact,
                UiCommand::Category => {
                    view.claims.category = match view.claims.category {
                        claims::Category::Experience => claims::Category::Credential,
                        claims::Category::Credential => claims::Category::Recommendation,
                        claims::Category::Recommendation => claims::Category::Experience,
                    }
                }
                UiCommand::Metric => view.claims.metric = (view.claims.metric + 1) % 3,
                UiCommand::Withdrawn => {
                    view.claims.withdrawn = !view.claims.withdrawn;
                    if view.claims.withdrawn {
                        view.claims.amount = "0".into();
                    }
                }
                UiCommand::Revise(value, withdrawn) => {
                    if let Ok(document) = serde_json::from_value::<claims::Document>(value.clone())
                    {
                        let statement = document.statement;
                        view.claims.category = statement.category;
                        let mut query = nucleus::trust_analysis::query::Query::for_subject(
                            statement.descriptor.subject.clone(),
                        );
                        query.role = statement.descriptor.role;
                        query.measurements = vec![statement.descriptor.measurement];
                        query.levels = vec![statement.descriptor.level];
                        query.counterparty_basis = statement.descriptor.counterparty_basis;
                        query.from = statement.descriptor.from;
                        query.until = statement.descriptor.until;
                        analysis::load(&mut view, query);
                        view.issuer_organ = matches!(statement.issuer, Identity::Organ(_));
                        view.public = matches!(statement.audience, Audience::Public);
                        view.readers = match statement.audience {
                            Audience::Selected { viewers } => viewers
                                .iter()
                                .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                                .collect::<Vec<_>>()
                                .join(", "),
                            Audience::Public => String::new(),
                        };
                        view.claims.claim = statement.claim;
                        view.claims.revision = statement.revision.to_string();
                        view.claims.title = statement.title;
                        view.claims.amount = if *withdrawn {
                            "0".into()
                        } else {
                            statement.amount.canonical()
                        };
                        view.claims.exact = statement.comparator == Comparator::Exact;
                        view.claims.metric = match statement.descriptor.metric {
                            Metric::Quantity => 0,
                            Metric::ReportingOccurrences => 1,
                            Metric::PositiveOccurrences => 2,
                        };
                        view.claims.withdrawn = *withdrawn;
                        view.claims.as_of = String::new();
                    }
                }
                _ => {}
            }
        }
    }
    super::render(world, owner);
}

fn input(world: &mut World, owner: Entity, key: Field, caption: &str, value: &str) {
    field(world, owner, owner, Key::Claim(key), caption, value);
}

pub(super) fn provenance(authorship: &claims::Authorship) -> String {
    match authorship {
        claims::Authorship::Manual => "Publisher declares a manual claim".into(),
        claims::Authorship::Generated {
            recipe,
            revision,
            partial,
            state,
            ..
        } => format!(
            "Publisher declares a generated claim from recipe {} revision {} · state {:?} · partial inputs {}. This signature does not verify recipe execution.",
            recipe.as_str(),
            revision,
            state,
            partial,
        ),
    }
}
fn button(world: &mut World, owner: Entity, caption: &str, command: UiCommand) {
    panel::button(world, owner, owner, caption, Command::Claim(command));
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let draft = view.claims.clone();
    let result = view.result.clone();
    let person = view.person.clone();
    let organ = view.context["organ"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let admin = view.context["organ_admin"] == true;
    let page = view.analysis.page;
    label(world, owner, "Profile claims and independent checking");
    label(
        world,
        owner,
        "A signature identifies the publisher. It does not verify the amount. Claims use one exact activity definition, role, evidence class and period from the analysis controls above. The reader keeps their own source choices.",
    );
    label(
        world,
        owner,
        "Issuer, private/public audience, extra viewers and term use the disclosure controls above. A public signed payload does not widen its private holder Record.",
    );
    button(world, owner, draft.category.label(), UiCommand::Category);
    input(world, owner, Field::Title, "Claim title", &draft.title);
    input(
        world,
        owner,
        Field::Amount,
        "Declared amount or count",
        &draft.amount,
    );
    button(
        world,
        owner,
        if draft.exact {
            "Comparator: exact"
        } else {
            "Comparator: at least"
        },
        UiCommand::Exact,
    );
    button(
        world,
        owner,
        match draft.metric {
            1 => "Metric: reporting occurrences (including zero reports)",
            2 => "Metric: occurrences with positive reported quantity",
            _ => "Metric: raw activity quantity",
        },
        UiCommand::Metric,
    );
    input(
        world,
        owner,
        Field::AsOf,
        "As of (optional RFC3339; empty means now)",
        &draft.as_of,
    );
    input(
        world,
        owner,
        Field::Claim,
        "Public claim identifier (empty starts a new claim)",
        &draft.claim,
    );
    input(
        world,
        owner,
        Field::Revision,
        "Expected current claim revision",
        &draft.revision,
    );
    button(
        world,
        owner,
        if draft.withdrawn {
            "This revision withdraws the claim"
        } else {
            "This revision states a claim"
        },
        UiCommand::Withdrawn,
    );
    button(
        world,
        owner,
        "Preview exact manual claim",
        UiCommand::Preview,
    );
    if let Some((review, _)) = &draft.review {
        let statement = &review.document.statement;
        label(world, owner, &provenance(&statement.authorship));
        label(
            world,
            owner,
            &analysis::measurement_caption(&statement.descriptor.measurement),
        );
        label(
            world,
            owner,
            &format!(
                "Publisher {}:{} · subject {}:{} · {:?} {} · {:?} · {:?} · audience {} · expires {}",
                statement.issuer.kind(),
                statement.issuer.uid(),
                statement.descriptor.subject.kind(),
                statement.descriptor.subject.uid(),
                statement.comparator,
                statement.amount,
                statement.descriptor.metric,
                statement.descriptor.level,
                serde_json::to_string(&statement.audience).unwrap_or_default(),
                statement.expires_at
            ),
        );
        button(
            world,
            owner,
            "Sign and retain the reviewed claim",
            UiCommand::Confirm,
        );
    }
    input(
        world,
        owner,
        Field::Import,
        "Original signed claim to retain",
        &draft.import,
    );
    button(world, owner, "Verify and retain claim", UiCommand::Import);
    button(
        world,
        owner,
        "My retained claims about the selected profile",
        UiCommand::List(None),
    );
    if result["kind"] == "claim_list" {
        for entry in result["entries"].as_array().into_iter().flatten() {
            let statement = &entry["document"]["statement"];
            label(
                world,
                owner,
                &format!(
                    "Publisher claims: {} · amount {} · authority at check {} · current retained {}",
                    statement["title"].as_str().unwrap_or_default(),
                    statement["amount"]["value"],
                    entry["authority_current"],
                    entry["current_retained"]
                ),
            );
            if let Some(record) = entry["record"].as_str() {
                button(
                    world,
                    owner,
                    "Inspect publisher statement",
                    UiCommand::Inspect(record.into()),
                );
            }
        }
        if let Some(next) = result["next"].as_str() {
            button(
                world,
                owner,
                "More retained claims",
                UiCommand::List(Some(next.into())),
            );
        }
    }
    if matches!(
        result["kind"].as_str(),
        Some("claim_document" | "claim_check")
    ) {
        if let Some(record) = result["record"].as_str() {
            evidence_serving_controls(
                world,
                owner,
                record,
                nucleus::trust_analysis::serving::SourceKind::Claim,
            );
        }
        if let Ok(document) = serde_json::from_value::<claims::Document>(result["document"].clone())
        {
            let statement = &document.statement;
            label(world, owner, &provenance(&statement.authorship));
            label(
                world,
                owner,
                &analysis::measurement_caption(&statement.descriptor.measurement),
            );
            if statement.withdrawn {
                label(
                    world,
                    owner,
                    "No current numerical claim: this signed revision withdraws the statement. An unavailable generated summary does not assert zero contribution.",
                );
            } else {
                label(
                    world,
                    owner,
                    &format!(
                        "Publisher {}:{} claims {:?} {} · {:?} · {:?}. As of {}; expires {}. Authority at last check: {}",
                        statement.issuer.kind(),
                        statement.issuer.uid(),
                        statement.comparator,
                        statement.amount,
                        statement.descriptor.metric,
                        statement.descriptor.level,
                        statement.as_of,
                        statement.expires_at,
                        result["authority_current"]
                    ),
                );
            }
            if let Some(record) = result["record"].as_str() {
                button(
                    world,
                    owner,
                    "Check locally using my source policy",
                    UiCommand::Check(record.into()),
                );
                button(
                    world,
                    owner,
                    "Export original signed claim",
                    UiCommand::Export(record.into()),
                );
                panel::button(
                    world,
                    owner,
                    owner,
                    "Open claim Record and visibility",
                    Command::Record(record.into()),
                );
            }
            if statement.issuer == Identity::Person(person)
                || (admin && statement.issuer == Identity::Organ(organ))
            {
                button(
                    world,
                    owner,
                    "Prepare a reviewed claim revision",
                    UiCommand::Revise(result["document"].clone(), false),
                );
                button(
                    world,
                    owner,
                    "Prepare a reviewed withdrawal",
                    UiCommand::Revise(result["document"].clone(), true),
                );
            }
            if result["export"].is_string() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Copy original claim document",
                    Command::Copy,
                );
            }
        }
    }
    render_check(world, owner, &result, page);
}

pub(super) fn render_check(world: &mut World, owner: Entity, result: &Value, page: usize) {
    if result["kind"] == "credential_check" {
        label(
            world,
            owner,
            &format!(
                "Issuer trusted for this exact activity: {}. Delivery quantity added: zero.",
                result["trusted_issuer_for_activity"]
            ),
        );
        label(
            world,
            owner,
            "This checks current issuer authority, signed scope, expiry, withdrawal and your source filters. It does not verify physical delivery.",
        );
    }
    if result["kind"] == "claim_check" {
        let state = match result["support"]["state"].as_str() {
            Some("partial_support") => "Partial support in visible evidence",
            Some("matches_visible_scope") => "Matches visible evidence",
            Some("at_least_supported_in_visible_scope") => {
                "At least this much is supported by visible evidence"
            }
            Some("visible_evidence_exceeds_claim") => "Visible evidence exceeds the stated amount",
            Some("arithmetic_unavailable") => "The total could not be calculated safely",
            _ => "Unknown: no counted evidence in this view",
        };
        label(
            world,
            owner,
            &format!(
                "Publisher claims {}; this reader sees support {} · {} · partial {}",
                result["support"]["claimed"]["value"]
                    .as_str()
                    .unwrap_or("unavailable"),
                result["support"]["observed"]["value"]
                    .as_str()
                    .unwrap_or("unknown"),
                state,
                result["support"]["partial"]
            ),
        );
        label(
            world,
            owner,
            "Missing evidence is not proof of missing activity. This check uses the permitted evidence already on this Lince, under your source choices.",
        );
        analysis::render_result(world, owner, &result, page);
    }
}
