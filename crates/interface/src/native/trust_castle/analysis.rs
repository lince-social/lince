use super::*;
use nucleus::{
    DecimalValue,
    transfer::{outcome::Role, receipt::Measurement},
    trust_analysis::{
        Condition, Filter,
        query::{CounterpartyBasis, EvidenceLevel, Query, Weights},
    },
};

#[derive(Clone)]
pub(super) struct Draft {
    pub subject: String,
    pub organ: bool,
    pub receiving: bool,
    pub people: bool,
    pub all_visible: bool,
    pub paths: bool,
    pub identity_budget: String,
    pub path_exclusions: Vec<Identity>,
    pub levels: [bool; 3],
    pub weights: [String; 4],
    pub caps: [String; 2],
    pub from: String,
    pub until: String,
    pub name: String,
    pub measurements: Vec<Measurement>,
    pub concept: String,
    pub unit: String,
    pub page: usize,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            subject: String::new(),
            organ: false,
            receiving: false,
            people: false,
            all_visible: false,
            paths: false,
            identity_budget: "1024".into(),
            path_exclusions: Vec::new(),
            levels: [true, false, false],
            weights: ["1".into(), "0.5".into(), "0.1".into(), "0.5".into()],
            caps: Default::default(),
            from: String::new(),
            until: String::new(),
            name: "My evidence analysis".into(),
            measurements: Vec::new(),
            concept: String::new(),
            unit: String::new(),
            page: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Subject,
    Name,
    From,
    Until,
    Weight(usize),
    Cap(usize),
    IdentityBudget,
    Concept,
    Unit,
}
#[derive(Clone, Copy)]
pub(super) enum Toggle {
    Organ,
    Receiving,
    People,
    Paths,
    Level(usize),
    ClearPathExclusions,
    AllVisible,
}

pub(super) fn capture(draft: &mut Draft, key: Field, value: String) {
    match key {
        Field::Subject => draft.subject = value,
        Field::Concept => draft.concept = value,
        Field::Unit => draft.unit = value,
        Field::Name => draft.name = value,
        Field::From => draft.from = value,
        Field::Until => draft.until = value,
        Field::IdentityBudget => draft.identity_budget = value,
        Field::Weight(index) => {
            if let Some(field) = draft.weights.get_mut(index) {
                *field = value;
            }
        }
        Field::Cap(index) => {
            if let Some(field) = draft.caps.get_mut(index) {
                *field = value;
            }
        }
    }
    draft.page = 0;
}

pub(super) fn toggle(draft: &mut Draft, key: Toggle) {
    match key {
        Toggle::Organ => draft.organ = !draft.organ,
        Toggle::Receiving => draft.receiving = !draft.receiving,
        Toggle::People => draft.people = !draft.people,
        Toggle::Paths => draft.paths = !draft.paths,
        Toggle::Level(index) => {
            if let Some(value) = draft.levels.get_mut(index) {
                *value = !*value;
            }
        }
        Toggle::ClearPathExclusions => draft.path_exclusions.clear(),
        Toggle::AllVisible => draft.all_visible = !draft.all_visible,
    }
    draft.page = 0;
}

fn number(value: &str) -> Result<DecimalValue, String> {
    if value.len() > 128 {
        return Err("Choose a bounded decimal coefficient".into());
    }
    DecimalValue::parse_inferred(value).map_err(|error| error.to_string())
}

fn time(value: &str) -> Result<Option<i64>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| Some(time.timestamp()))
        .map_err(|_| "Use a date and timezone such as 2026-10-01T00:00:00Z".into())
}

pub(super) fn condition(rule: &Rule) -> Result<Condition, String> {
    let bound = |value: &str, inclusive| -> Result<Option<nucleus::visibility::Bound>, String> {
        if value.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(nucleus::visibility::Bound {
                value: value
                    .parse()
                    .map_err(|_| "Choose a nonnegative Organ proximity")?,
                inclusive,
            }))
        }
    };
    Ok(Condition {
        identities: identities(&rule.identities)?,
        known: match rule.known {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        },
        lower: bound(&rule.lower, rule.lower_inclusive)?,
        upper: bound(&rule.upper, rule.upper_inclusive)?,
    })
}

pub(super) fn build(view: &View) -> Result<Query, String> {
    let draft = &view.analysis;
    let subject = if draft.organ {
        Identity::Organ(draft.subject.clone())
    } else {
        Identity::Person(draft.subject.clone())
    };
    let mut query = Query::for_subject(subject);
    query.role = if draft.receiving {
        Role::Receiver
    } else {
        Role::Giver
    };
    query.rely_on_all_visible = draft.all_visible;
    query.counterparty_basis = if draft.people {
        CounterpartyBasis::AttributedPerson
    } else {
        CounterpartyBasis::Organ
    };
    query.from = time(&draft.from)?;
    query.until = time(&draft.until)?;
    query.levels = [
        EvidenceLevel::NeedLinkedReceiver,
        EvidenceLevel::Receiver,
        EvidenceLevel::Giver,
    ]
    .into_iter()
    .enumerate()
    .filter(|(index, _)| draft.levels[*index])
    .map(|(_, level)| level)
    .collect();
    query.weights = Weights {
        need_linked_receiver: number(&draft.weights[0])?,
        receiver: number(&draft.weights[1])?,
        giver: number(&draft.weights[2])?,
        distance_decay: number(&draft.weights[3])?,
    };
    let cap = |value: &str| {
        if value.trim().is_empty() {
            Ok(None)
        } else {
            number(value).map(Some)
        }
    };
    query.issuer_cap = cap(&draft.caps[0])?;
    query.first_contact_cap = cap(&draft.caps[1])?;
    query.measurements = draft.measurements.clone();
    query.exclude_everywhere = identities(&view.excluded)?;
    query.contacts = if draft.paths {
        let mut policy = path_policy(view)?;
        policy.max_identities = draft
            .identity_budget
            .parse()
            .map_err(|_| "Choose an identity budget from 1 to 1024")?;
        policy.exclude_everywhere = draft.path_exclusions.clone();
        Some(policy)
    } else {
        None
    };
    for rule in &view.rules {
        let filter = match rule.scope {
            RuleScope::Issuer => &mut query.issuers,
            RuleScope::Counterparty => &mut query.counterparties,
            RuleScope::Endpoint => &mut query.endpoints,
            _ => continue,
        };
        let condition = condition(rule)?;
        if rule.exclusion {
            filter.exclude.push(condition)
        } else {
            filter.include.push(condition)
        }
    }
    query.validate()?;
    Ok(query)
}

pub(super) fn load(view: &mut View, query: Query) {
    let display_time = |time: Option<i64>| {
        time.and_then(|time| chrono::DateTime::from_timestamp(time, 0))
            .map(|time| time.to_rfc3339())
            .unwrap_or_default()
    };
    let draft = Draft {
        subject: query.subject.uid().into(),
        organ: matches!(query.subject, Identity::Organ(_)),
        receiving: query.role == Role::Receiver,
        people: query.counterparty_basis == CounterpartyBasis::AttributedPerson,
        all_visible: query.rely_on_all_visible,
        paths: query.contacts.is_some(),
        identity_budget: query
            .contacts
            .as_ref()
            .map_or(1024, |policy| policy.max_identities)
            .to_string(),
        path_exclusions: query
            .contacts
            .as_ref()
            .map(|policy| policy.exclude_everywhere.clone())
            .unwrap_or_default(),
        levels: [
            EvidenceLevel::NeedLinkedReceiver,
            EvidenceLevel::Receiver,
            EvidenceLevel::Giver,
        ]
        .map(|level| query.levels.contains(&level)),
        weights: [
            query.weights.need_linked_receiver,
            query.weights.receiver,
            query.weights.giver,
            query.weights.distance_decay,
        ]
        .map(|value| value.canonical()),
        caps: [query.issuer_cap, query.first_contact_cap]
            .map(|value| value.map(|value| value.canonical()).unwrap_or_default()),
        from: display_time(query.from),
        until: display_time(query.until),
        concept: query
            .measurements
            .first()
            .map(|measurement| measurement.meaning.concept_uid.clone())
            .unwrap_or_default(),
        unit: query
            .measurements
            .first()
            .and_then(|measurement| measurement.unit.as_ref())
            .map(|unit| unit.concept_uid.clone())
            .unwrap_or_default(),
        measurements: query.measurements,
        name: view.analysis.name.clone(),
        page: 0,
    };
    view.analysis = draft;
    view.excluded = query
        .exclude_everywhere
        .iter()
        .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
        .collect::<Vec<_>>()
        .join(", ");
    view.rules.clear();
    let mut add = |scope, filter: &Filter| {
        for (exclusion, conditions) in [(false, &filter.include), (true, &filter.exclude)] {
            for condition in conditions {
                view.rules.push(Rule {
                    scope,
                    exclusion,
                    identities: condition
                        .identities
                        .iter()
                        .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                        .collect::<Vec<_>>()
                        .join(", "),
                    known: match condition.known {
                        Some(true) => 1,
                        Some(false) => 2,
                        None => 0,
                    },
                    lower: condition
                        .lower
                        .map(|bound| bound.value.to_string())
                        .unwrap_or_default(),
                    upper: condition
                        .upper
                        .map(|bound| bound.value.to_string())
                        .unwrap_or_default(),
                    lower_inclusive: condition.lower.is_none_or(|bound| bound.inclusive),
                    upper_inclusive: condition.upper.is_none_or(|bound| bound.inclusive),
                });
            }
        }
    };
    add(RuleScope::Issuer, &query.issuers);
    add(RuleScope::Counterparty, &query.counterparties);
    add(RuleScope::Endpoint, &query.endpoints);
    if let Some(policy) = query.contacts {
        add(RuleScope::Target, &policy.targets);
        add(RuleScope::Intermediary, &policy.intermediaries);
        view.anchor = policy.anchor.uid().into();
        view.anchor_organ = matches!(policy.anchor, Identity::Organ(_));
        view.depth = policy.max_depth.to_string();
        view.unilateral = policy.allow_unilateral;
    }
}

fn input(world: &mut World, owner: Entity, key: Field, caption: &str, value: &str) {
    field(world, owner, owner, Key::Query(key), caption, value);
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let draft = view.analysis.clone();
    let result = view.result.clone();
    label(world, owner, "Analyze permitted Transfer evidence");
    label(
        world,
        owner,
        "This profile is separate from your reader identity and the contact-path anchor. Person history requires explicit attribution; Organ membership does not inherit it.",
    );
    input(
        world,
        owner,
        Field::Subject,
        "Profile Record being analyzed",
        &draft.subject,
    );
    for (caption, key) in [
        (
            if draft.organ {
                "Subject: Organ"
            } else {
                "Subject: Person"
            },
            Toggle::Organ,
        ),
        (
            if draft.receiving {
                "Role: receiving"
            } else {
                "Role: giving"
            },
            Toggle::Receiving,
        ),
        (
            if draft.people {
                "Counterparty: explicitly attributed Person"
            } else {
                "Counterparty: Organ"
            },
            Toggle::People,
        ),
        (
            if draft.paths {
                "Require the configured contact paths"
            } else {
                "Use visible local evidence without a path requirement"
            },
            Toggle::Paths,
        ),
    ] {
        panel::button(world, owner, owner, caption, Command::QueryToggle(key));
    }
    panel::button(
        world,
        owner,
        owner,
        if draft.all_visible {
            "My policy counts all permitted issuers"
        } else {
            "Count only selected issuers or path-qualified counterparties"
        },
        Command::QueryToggle(Toggle::AllVisible),
    );
    label(
        world,
        owner,
        "Select included issuers or require contact paths before counting contributions. Recognizing a signing key alone does not make its reports a source you rely on. Counting all visible issuers is an explicit local choice.",
    );
    if draft.paths {
        input(
            world,
            owner,
            Field::IdentityBudget,
            "Maximum identities in contact paths (1–1024)",
            &draft.identity_budget,
        );
        if !draft.path_exclusions.is_empty() {
            label(
                world,
                owner,
                &format!(
                    "Additional saved path-only exclusions: {}",
                    draft
                        .path_exclusions
                        .iter()
                        .map(|identity| format!("{}:{}", identity.kind(), identity.uid()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            panel::button(
                world,
                owner,
                owner,
                "Clear additional path-only exclusions",
                Command::QueryToggle(Toggle::ClearPathExclusions),
            );
        }
    }
    for (index, caption) in [
        "Need-linked receiver confirmation with settlement",
        "Receiver confirmation",
        "Giver claim",
    ]
    .iter()
    .enumerate()
    {
        panel::button(
            world,
            owner,
            owner,
            &format!(
                "{} {caption}",
                if draft.levels[index] {
                    "Included:"
                } else {
                    "Excluded:"
                }
            ),
            Command::QueryToggle(Toggle::Level(index)),
        );
    }
    for (index, caption) in [
        "Need-linked receiver weight (0–100)",
        "Receiver weight (0–100)",
        "Giver weight (0–100)",
        "Weight retained for each additional contact hop (0–1)",
    ]
    .iter()
    .enumerate()
    {
        input(
            world,
            owner,
            Field::Weight(index),
            caption,
            &draft.weights[index],
        );
    }
    for (index, caption) in [
        "Index cap per issuer and measurement (optional)",
        "Index cap per first contact and measurement (optional; requires paths)",
    ]
    .iter()
    .enumerate()
    {
        input(world, owner, Field::Cap(index), caption, &draft.caps[index]);
    }
    input(
        world,
        owner,
        Field::From,
        "Reported from (e.g. 2026-10-01T00:00:00Z)",
        &draft.from,
    );
    input(
        world,
        owner,
        Field::Until,
        "Reported before (optional, same date format)",
        &draft.until,
    );
    label(
        world,
        owner,
        &format!(
            "Activity definitions selected: {}",
            if draft.measurements.is_empty() {
                "all permitted meanings, kept in separate groups".into()
            } else {
                draft.measurements.len().to_string()
            }
        ),
    );
    input(
        world,
        owner,
        Field::Concept,
        "Activity Concept identity",
        &draft.concept,
    );
    input(
        world,
        owner,
        Field::Unit,
        "Unit Concept identity (optional)",
        &draft.unit,
    );
    panel::button(
        world,
        owner,
        owner,
        "Select exact activity and unit versions",
        Command::ResolveMeasurement,
    );
    if !draft.measurements.is_empty() {
        for measurement in &draft.measurements {
            label(world, owner, &measurement_caption(measurement));
        }
        panel::button(
            world,
            owner,
            owner,
            "Use all permitted activity definitions",
            Command::QueryMeasurement(None),
        );
    }
    label(
        world,
        owner,
        "Issuer and counterparty conditions above apply to this analysis. Endpoint conditions are saved for deliberate remote searches; this local search contacts no remote node.",
    );
    panel::button(
        world,
        owner,
        owner,
        "Analyze local evidence",
        Command::Analyze,
    );
    input(
        world,
        owner,
        Field::Name,
        "Private saved query name",
        &draft.name,
    );
    panel::button(
        world,
        owner,
        owner,
        "Save this query privately",
        Command::SaveQuery,
    );
    panel::button(
        world,
        owner,
        owner,
        "My saved queries",
        Command::ListQueries(None),
    );
    match result["kind"].as_str() {
        Some("query_list") => {
            for entry in result["entries"].as_array().into_iter().flatten() {
                panel::button(
                    world,
                    owner,
                    owner,
                    entry["name"].as_str().unwrap_or("Saved query"),
                    Command::LoadQuery(entry["record"].as_str().unwrap_or_default().into()),
                );
            }
            if let Some(next) = result["next"].as_str() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "More saved queries",
                    Command::ListQueries(Some(next.into())),
                );
            }
        }
        Some("saved_query") => {
            label(
                world,
                owner,
                "Private reader policy loaded. It does not publish a claim or endorse any identity.",
            );
            if let Some(record) = result["record"].as_str() {
                panel::button(
                    world,
                    owner,
                    owner,
                    "Open saved query Record",
                    Command::Record(record.into()),
                );
            }
        }
        Some("query_analysis") => render_result(world, owner, &result, draft.page),
        _ => {}
    }
}

pub(super) fn measurement_caption(measurement: &Measurement) -> String {
    format!(
        "Activity {} · Vocabulary {} · definition v{} / {} · unit {} · object {}",
        measurement.meaning.concept_uid,
        measurement.meaning.vocabulary_uid,
        measurement.meaning.revision,
        measurement.meaning.definition_hash,
        measurement
            .unit
            .as_ref()
            .map(|unit| format!(
                "{} v{} / {}",
                unit.concept_uid, unit.revision, unit.definition_hash
            ))
            .unwrap_or_else(|| "none".into()),
        measurement
            .object_commitment
            .as_ref()
            .map(|hash| hash.as_str())
            .unwrap_or("none"),
    )
}

fn artifact_controls(world: &mut World, owner: Entity, row: &Value) {
    if world.get::<View>(owner).is_none_or(|view| view.remote.job.is_none()) { return; }
    for original in row["originals"].as_array().into_iter().flatten().take(32) {
        if original["provenance"] == "artifact" {
            if let Some(hash) = original["hash"].as_str().and_then(|hash| nucleus::karma::CanonicalHash::parse(hash.to_owned()).ok()) {
                panel::button(world, owner, owner, "Inspect collected original proof", Command::Remote(super::remote_controls::UiCommand::Inspect(hash)));
            }
        }
    }
}

pub(super) fn render_result(world: &mut World, owner: Entity, result: &Value, page: usize) {
    let analysis = &result["analysis"];
    let checked = result["checked_at"]
        .as_i64()
        .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| "unavailable".into());
    label(
        world,
        owner,
        &format!("Checked {checked}. Analyze again to refresh this local snapshot."),
    );
    label(
        world,
        owner,
        "Raw quantities remain separate from policy-derived indices. Groups use identical meaning/unit versions, object and evidence class; there is no universal total.",
    );
    label(
        world,
        owner,
        &format!(
            "Coverage: {} · policy {}",
            if analysis["partial"] == true {
                "partial; some evidence or paths are unavailable"
            } else {
                "permitted local evidence only"
            },
            analysis["policy"].as_str().unwrap_or_default()
        ),
    );
    let start = page.saturating_mul(64);
    let groups = analysis["groups"].as_array();
    let rows = analysis["rows"].as_array();
    let protocol = analysis["protocol"].as_array();
    let withheld = analysis["withheld"].as_array();
    if groups.is_none_or(Vec::is_empty) {
        label(
            world,
            owner,
            "No counted outcomes match this view. Inspect the visible reports and source choices below; missing or withheld evidence does not prove zero contribution.",
        );
    }
    let quantity = |value: &Value| value["value"].as_str().unwrap_or("unavailable").to_owned();
    for group in groups.into_iter().flatten().skip(start).take(64) {
        label(
            world,
            owner,
            &format!(
                "{} · raw {} · weighted index {} · reporting occurrences {} · rounded {} · capped {} · arithmetic unavailable {}",
                group["level"].as_str().unwrap_or_default(),
                quantity(&group["raw_quantity"]),
                quantity(&group["weighted_index"]),
                group["occurrences"],
                group["index_rounded"],
                group["capped"],
                group["arithmetic_error"]
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Meaning {} revision {} · unit {}",
                group["measurement"]["meaning"]["concept_uid"]
                    .as_str()
                    .unwrap_or_default(),
                group["measurement"]["meaning"]["revision"],
                group["measurement"]["unit"]["concept_uid"]
                    .as_str()
                    .unwrap_or("unspecified")
            ),
        );
        if let Ok(measurement) = serde_json::from_value(group["measurement"].clone()) {
            panel::button(
                world,
                owner,
                owner,
                "Analyze only this exact activity definition",
                Command::QueryMeasurement(Some(measurement)),
            );
        }
    }
    for row in rows.into_iter().flatten().skip(start).take(64) {
        label(
            world,
            owner,
            &format!(
                "{} · reported {} · selected {} · disputed {} · withdrawn {} · contacts away {}",
                row["level"].as_str().unwrap_or_default(),
                quantity(&row["quantity"]),
                row["selected"],
                row["disputed"],
                row["withdrawn"],
                row["distance"]
            ),
        );
        label(
            world,
            owner,
            &format!(
                "Original issuer {} · counterparty {} · occurrence evidence {}",
                row["issuer"]["uid"].as_str().unwrap_or_default(),
                row["counterparty"]["uid"].as_str().unwrap_or_default(),
                row["source"].as_str().unwrap_or_default()
            ),
        );
        if let Ok(agreed) = serde_json::from_value::<nucleus::transfer::receipt::RecordedFloat>(
            row["agreed_quantity"].clone(),
        ) {
            if let Ok(value) = agreed.value() {
                label(
                    world,
                    owner,
                    &format!(
                        "Separately disclosed agreed canonical quantity: {value} (binary64 {}) · canonical unit {}",
                        agreed.ieee754_binary64,
                        row["canonical_unit"]["concept_uid"]
                            .as_str()
                            .unwrap_or("not disclosed")
                    ),
                );
            }
        }
        for record in row["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .take(1)
        {
            panel::button(
                world,
                owner,
                owner,
                "Open original evidence Record",
                Command::Record(record.into()),
            );
        }
        artifact_controls(world, owner, row);
    }
    if !protocol.is_none_or(Vec::is_empty) {
        label(
            world,
            owner,
            "Original Need and protocol context. Canonical quantities keep their recorded binary64 precision and are not added to activity amounts. Private accounting effects do not establish physical delivery or non-delivery.",
        );
    }
    for row in protocol.into_iter().flatten().skip(start).take(64) {
        let content = &row["content"];
        let recorded = |value: &Value| {
            serde_json::from_value::<nucleus::transfer::receipt::RecordedFloat>(value.clone())
                .ok()
                .and_then(|value| {
                    value
                        .value()
                        .ok()
                        .map(|number| format!("{number} (binary64 {})", value.ieee754_binary64))
                })
                .unwrap_or_else(|| "unavailable".into())
        };
        let description = match content["kind"].as_str() {
            Some("need") => format!("Receiver-authored Need: {}",quantity(&content["requested"])),
            Some("settled") => format!("Canonical settled quantity: {}",recorded(&content["quantity"])),
            Some("settlement_status") => format!("At that canonical settlement: settled {}; remaining then {}",recorded(&content["quantity"]),recorded(&content["remaining_after"])),
            Some("cancelled") => format!("Cancelled canonical quantity: {}",quantity(&content["quantity"])),
            Some("compensated") => "Private accounting compensation was reported; this does not establish physical non-delivery".into(),
            Some("dispute") => format!("Dispute active: {}",content["active"]),
            _ => "Protocol context unavailable".into(),
        };
        label(
            world,
            owner,
            &format!(
                "{description} · {} · occurrence evidence {}",
                if row["current"] == true {
                    "current observed statement"
                } else {
                    "historical Need referenced by a report"
                },
                row["source"].as_str().unwrap_or_default()
            ),
        );
        if row["canonical_unit"].is_null()
            && matches!(
                content["kind"].as_str(),
                Some("settled" | "settlement_status" | "cancelled")
            )
        {
            label(
                world,
                owner,
                "No immutable canonical unit definition was disclosed for this protocol quantity.",
            );
        }
        if let Some(record) = row["records"]
            .as_array()
            .and_then(|records| records.first())
            .and_then(Value::as_str)
        {
            panel::button(
                world,
                owner,
                owner,
                "Open original protocol evidence",
                Command::Record(record.into()),
            );
        }
        artifact_controls(world, owner, row);
    }
    if !analysis["withheld"].as_array().is_none_or(Vec::is_empty) {
        label(
            world,
            owner,
            "Some source histories contain conflicts and are withheld. Inspect the original evidence; copies do not resolve conflicts by voting.",
        );
    }
    for row in withheld.into_iter().flatten().skip(start).take(64) {
        label(
            world,
            owner,
            &format!(
                "Withheld: {}",
                row["reason"].as_str().unwrap_or("incomplete evidence")
            ),
        );
        if let Some(record) = row["records"]
            .as_array()
            .and_then(|records| records.first())
            .and_then(Value::as_str)
        {
            panel::button(
                world,
                owner,
                owner,
                "Inspect withheld source evidence",
                Command::Record(record.into()),
            );
        }
        artifact_controls(world, owner, row);
    }
    if page > 0 {
        panel::button(
            world,
            owner,
            owner,
            "Previous evidence page",
            Command::QueryPage(page - 1),
        );
    }
    if [groups, rows, protocol, withheld]
        .into_iter()
        .map(|items| items.map_or(0, Vec::len))
        .max()
        .unwrap_or(0)
        > start + 64
    {
        panel::button(
            world,
            owner,
            owner,
            "Next evidence page",
            Command::QueryPage(page + 1),
        );
    }
}
