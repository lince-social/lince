use super::*;
use nucleus::trust_analysis::{
    Filter,
    custody::{Command as Request, Policy},
};

#[derive(Clone)]
pub(super) struct Draft {
    enabled: bool,
    forwarding: bool,
    storage: String,
    items: String,
    term: String,
    version: Option<u64>,
    page: usize,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            enabled: false,
            forwarding: false,
            storage: (8_u64 * 1024 * 1024).to_string(),
            items: "256".into(),
            term: (365_u32 * 86400).to_string(),
            version: None,
            page: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Storage,
    Items,
    Term,
}

#[derive(Clone)]
pub(super) enum UiCommand {
    Load,
    Toggle,
    Forwarding,
    Save,
    Inventory(Option<i64>),
    Cleanup,
    Outbox(Option<i64>),
    Cancel(String, nucleus::karma::CanonicalHash),
    Inspect(String),
    Page(usize),
    Copy,
}

pub(super) fn capture(draft: &mut Draft, field: Field, value: String) {
    match field {
        Field::Storage => draft.storage = value,
        Field::Items => draft.items = value,
        Field::Term => draft.term = value,
    }
}

fn filter(view: &View, scope: RuleScope) -> Result<Filter, String> {
    let mut filter = Filter::default();
    for rule in view.rules.iter().filter(|rule| rule.scope == scope) {
        let condition = analysis::condition(rule)?;
        if rule.exclusion {
            filter.exclude.push(condition);
        } else {
            filter.include.push(condition);
        }
    }
    Ok(filter)
}

pub(super) fn request(view: &View, command: &UiCommand) -> Result<Request, String> {
    Ok(match command {
        UiCommand::Load => Request::InspectPolicy,
        UiCommand::Inspect(record) => Request::InspectOriginal {
            record: record.clone(),
        },
        UiCommand::Save => {
            let expected_version = view
                .custody
                .version
                .ok_or("Load current custody settings before saving")?;
            let policy = Policy {
                enabled: view.custody.enabled,
                forwarding: nucleus::trust_analysis::custody::forwarding::Policy {
                    enabled: view.custody.forwarding,
                    peers: filter(view, RuleScope::CustodyForwardPeer)?,
                },
                senders: filter(view, RuleScope::CustodySender)?,
                readers: filter(view, RuleScope::CustodyReader)?,
                storage_bytes: view
                    .custody
                    .storage
                    .parse()
                    .map_err(|_| "Choose storage quota in bytes")?,
                items: view
                    .custody
                    .items
                    .parse()
                    .map_err(|_| "Choose a positive retained-item limit")?,
                retention_seconds: view
                    .custody
                    .term
                    .parse()
                    .map_err(|_| "Choose retention in seconds up to one year")?,
            };
            policy.validate()?;
            Request::SetPolicy {
                policy,
                expected_version,
            }
        }
        UiCommand::Inventory(before) => Request::Inventory { before: *before },
        UiCommand::Outbox(before) => Request::Outbox { before: *before },
        UiCommand::Cancel(record, expected_offer) => Request::CancelDelivery {
            record: record.clone(),
            expected_offer: expected_offer.clone(),
        },
        UiCommand::Cleanup => Request::Cleanup,
        UiCommand::Toggle | UiCommand::Forwarding | UiCommand::Page(_) | UiCommand::Copy => {
            return Err("This control edits the local review".into());
        }
    })
}

pub(super) fn apply(world: &mut World, owner: Entity, command: &UiCommand) {
    if let UiCommand::Page(page) = command {
        world.get_mut::<View>(owner).unwrap().custody.page = *page;
        return;
    }
    if matches!(command, UiCommand::Copy) {
        let view = world.get::<View>(owner).unwrap();
        let copied = if view.result["kind"] == "custody_original" {
            serde_json::from_value::<nucleus::trust_analysis::serving::Artifact>(
                view.result["artifact"].clone(),
            )
            .map_err(|error| error.to_string())
            .and_then(|artifact| artifact.bytes())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
        } else {
            Err("Inspect a currently permitted original first".into())
        };
        let copied = copied.and_then(|text| {
            world
                .get_resource_mut::<bevy::clipboard::Clipboard>()
                .ok_or("Clipboard unavailable".to_owned())?
                .set_text(&text)
                .map_err(|error| error.to_string())
        });
        world.get_mut::<View>(owner).unwrap().notice = copied.map_or_else(
            |error| error,
            |()| "Canonical signed original copied".into(),
        );
        return;
    }
    if matches!(command, UiCommand::Inspect(_)) {
        world.get_mut::<View>(owner).unwrap().custody.page = 0;
    }
    if matches!(command, UiCommand::Toggle | UiCommand::Forwarding) {
        let mut view = world.get_mut::<View>(owner).unwrap();
        if view.pending.is_none() {
            if matches!(command, UiCommand::Forwarding) {
                view.custody.forwarding = !view.custody.forwarding;
            } else {
                view.custody.enabled = !view.custody.enabled;
            }
            view.generation = view.generation.wrapping_add(1);
            view.result = Value::Null;
        }
        return;
    }
    let result = request(world.get::<View>(owner).unwrap(), command)
        .and_then(|request| super::request(world, owner, Backend::Custody { request }));
    if let Err(error) = result {
        world.get_mut::<View>(owner).unwrap().notice = error;
    }
}

pub(super) fn receive(view: &mut View, result: &Value) {
    if result["kind"] != "custody_policy" {
        return;
    }
    let Ok(policy) = serde_json::from_value::<Policy>(result["state"]["policy"].clone()) else {
        return;
    };
    view.custody = Draft {
        enabled: policy.enabled,
        forwarding: policy.forwarding.enabled,
        storage: policy.storage_bytes.to_string(),
        items: policy.items.to_string(),
        term: policy.retention_seconds.to_string(),
        version: result["state"]["version"].as_u64(),
        page: 0,
    };
    view.rules.retain(|rule| {
        !matches!(
            rule.scope,
            RuleScope::CustodySender | RuleScope::CustodyReader | RuleScope::CustodyForwardPeer
        )
    });
    for (scope, filter) in [
        (RuleScope::CustodySender, policy.senders),
        (RuleScope::CustodyReader, policy.readers),
        (RuleScope::CustodyForwardPeer, policy.forwarding.peers),
    ] {
        for (exclusion, conditions) in [(false, filter.include), (true, filter.exclude)] {
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
    }
}

fn button(world: &mut World, owner: Entity, title: &str, command: UiCommand) {
    panel::button(world, owner, owner, title, Command::Custody(command));
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let draft = view.custody.clone();
    let result = view.result.clone();
    let admin = view.context["organ_admin"] == true;
    label(world, owner, "Retain public signed evidence for others");
    label(
        world,
        owner,
        "Custody is optional and off by default. A signed acknowledgment promises retained bytes until expiry; it is not a contribution or confirmation of physical delivery. Full storage refuses new promises. Disabling serving does not erase an existing retention commitment.",
    );
    button(
        world,
        owner,
        "Load current custody settings",
        UiCommand::Load,
    );
    if admin {
        button(
            world,
            owner,
            if draft.enabled {
                "Proposed custody: enabled"
            } else {
                "Proposed custody: disabled"
            },
            UiCommand::Toggle,
        );
        button(
            world,
            owner,
            if draft.forwarding {
                "Proposed evidence forwarding: enabled"
            } else {
                "Proposed evidence forwarding: disabled"
            },
            UiCommand::Forwarding,
        );
        label(
            world,
            owner,
            "Forwarding covers only my existing Public evidence-serving approvals and accepted Public custody copies. Public gossip and Send consent for the chosen contacts must also be enabled. Each displayed policy authorizes at most three attempts per original and peer; reapplying it explicitly authorizes a new attempt set for failed deliveries.",
        );
        field(
            world,
            owner,
            owner,
            Key::Custody(Field::Storage),
            "Retained payload quota (bytes, maximum 1 GiB)",
            &draft.storage,
        );
        field(
            world,
            owner,
            owner,
            Key::Custody(Field::Items),
            "Retained originals (maximum 4096)",
            &draft.items,
        );
        field(
            world,
            owner,
            owner,
            Key::Custody(Field::Term),
            "Maximum retention (seconds, maximum one year)",
            &draft.term,
        );
        for (title, scope, exclusion) in [
            ("Include custody sender", RuleScope::CustodySender, false),
            ("Exclude custody sender", RuleScope::CustodySender, true),
            ("Include custody reader", RuleScope::CustodyReader, false),
            ("Exclude custody reader", RuleScope::CustodyReader, true),
            (
                "Include forwarding peer",
                RuleScope::CustodyForwardPeer,
                false,
            ),
            (
                "Exclude forwarding peer",
                RuleScope::CustodyForwardPeer,
                true,
            ),
        ] {
            panel::button(
                world,
                owner,
                owner,
                title,
                Command::AddRule(scope, exclusion),
            );
        }
        button(
            world,
            owner,
            "Apply the displayed custody policy",
            UiCommand::Save,
        );
        button(
            world,
            owner,
            "Clean expired custody payloads",
            UiCommand::Cleanup,
        );
    }
    button(
        world,
        owner,
        "List my retained evidence",
        UiCommand::Inventory(None),
    );
    button(
        world,
        owner,
        "List evidence forwarding jobs",
        UiCommand::Outbox(None),
    );
    if matches!(
        result["kind"].as_str(),
        Some("custody_outbox" | "custody_delivery")
    ) {
        let rows = if result["kind"] == "custody_delivery" {
            vec![&result["job"]]
        } else {
            result["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        };
        for row in rows {
            let state = match row["state"].as_str() {
                Some("pending") => "Queued",
                Some("sending") => "Sending",
                Some("stored") => "Custodian acknowledged retention",
                Some("held") => "Waiting for permission",
                Some("failed") => "Failed after bounded attempts",
                Some("cancelled") => "Further sending cancelled",
                _ => "Expired",
            };
            label(
                world,
                owner,
                &format!(
                    "{} · {} · {} bytes sent · {} attempts · {}",
                    row["offer"]["artifact"].as_str().unwrap_or_default(),
                    state,
                    row["sent_bytes"],
                    row["attempts"],
                    row["error"].as_str().unwrap_or_default()
                ),
            );
            if matches!(
                row["state"].as_str(),
                Some("pending" | "sending" | "held" | "failed")
            ) {
                if let (Some(record), Ok(offer)) = (
                    row["record"].as_str(),
                    serde_json::from_value::<nucleus::trust_analysis::custody::Offer>(
                        row["offer"].clone(),
                    ),
                ) {
                    if let Ok(hash) = offer.hash() {
                        button(
                            world,
                            owner,
                            "Cancel this reviewed delivery",
                            UiCommand::Cancel(record.into(), hash),
                        );
                    }
                }
            }
        }
        if let Some(next) = result["next"].as_i64() {
            button(
                world,
                owner,
                "Next forwarding jobs page",
                UiCommand::Outbox(Some(next)),
            );
        }
    }
    if result["kind"] == "custody_inventory" {
        for row in result["rows"].as_array().into_iter().flatten() {
            if let Some(record) = row["record"].as_str() {
                button(
                    world,
                    owner,
                    "Inspect retained signed original",
                    UiCommand::Inspect(record.into()),
                );
            }
            label(
                world,
                owner,
                &format!(
                    "{} · {} bytes · retained until {}",
                    row["hash"].as_str().unwrap_or_default(),
                    row["bytes"],
                    row["retained_until"]
                ),
            );
        }
        if let Some(next) = result["next"].as_i64() {
            button(
                world,
                owner,
                "Next retained-evidence page",
                UiCommand::Inventory(Some(next)),
            );
        }
    }
    if result["kind"] == "custody_original" {
        label(
            world,
            owner,
            &format!(
                "Original {} · signature status {}",
                result["hash"], result["signature"]
            ),
        );
        let text = serde_json::to_string_pretty(&result["artifact"]).unwrap_or_default();
        let pages = text.chars().count().div_ceil(4096).max(1);
        let page = draft.page.min(pages - 1);
        label(
            world,
            owner,
            &text
                .chars()
                .skip(page * 4096)
                .take(4096)
                .collect::<String>(),
        );
        if page > 0 {
            button(
                world,
                owner,
                "Previous original page",
                UiCommand::Page(page - 1),
            );
        }
        if page + 1 < pages {
            button(
                world,
                owner,
                "Next original page",
                UiCommand::Page(page + 1),
            );
        }
        button(
            world,
            owner,
            "Copy canonical signed original",
            UiCommand::Copy,
        );
    }
}
