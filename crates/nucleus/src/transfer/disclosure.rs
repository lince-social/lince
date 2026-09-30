use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudienceScope {
    #[default]
    Everyone,
    Participants,
    Owner,
    Selected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FieldAudience {
    pub scope: AudienceScope,
    pub people: Vec<String>,
}

impl FieldAudience {
    pub fn allows(&self, viewer: Option<&str>, owner: Option<&str>, participant: bool) -> bool {
        if viewer.is_some() && viewer == owner {
            return true;
        }
        match self.scope {
            AudienceScope::Everyone => true,
            AudienceScope::Participants => participant,
            AudienceScope::Owner => false,
            AudienceScope::Selected => {
                viewer.is_some_and(|uid| self.people.iter().any(|p| p == uid))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ItemDisclosure {
    pub title: FieldAudience,
    pub description: FieldAudience,
    pub source: FieldAudience,
    pub parties: FieldAudience,
    pub quantity: FieldAudience,
    pub location: FieldAudience,
}

impl Default for ItemDisclosure {
    fn default() -> Self {
        Self {
            title: FieldAudience::default(),
            description: FieldAudience::default(),
            source: FieldAudience {
                scope: AudienceScope::Owner,
                people: Vec::new(),
            },
            parties: FieldAudience::default(),
            quantity: FieldAudience::default(),
            location: FieldAudience::default(),
        }
    }
}

impl ItemDisclosure {
    pub fn fields_mut(&mut self) -> [&mut FieldAudience; 6] {
        [
            &mut self.title,
            &mut self.description,
            &mut self.source,
            &mut self.parties,
            &mut self.quantity,
            &mut self.location,
        ]
    }

    pub fn fields(&self) -> [(&'static str, &FieldAudience); 6] {
        [
            ("title", &self.title),
            ("description", &self.description),
            ("source", &self.source),
            ("parties", &self.parties),
            ("quantity", &self.quantity),
            ("location", &self.location),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TransferItem {
    pub title: String,
    pub description: String,
    pub disclosure: ItemDisclosure,
    pub exchange: Option<super::exchange::ExchangeRoute>,
    pub loan: Option<super::loans::Interval>,
    pub return_of: Option<super::loans::Reference>,
    pub future_need_for: Option<super::loans::Reference>,
}

impl TransferItem {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.title.trim().is_empty() || self.title.chars().count() > 500 {
            return Err("item title must contain 1 to 500 characters");
        }
        if self.description.chars().count() > 20_000 {
            return Err("item description must contain at most 20000 characters");
        }
        if let Some(exchange) = &self.exchange {
            exchange.validate()?;
        }
        if [self.loan.is_some(), self.return_of.is_some(), self.future_need_for.is_some()].into_iter().filter(|present| *present).count() > 1 {
            return Err("choose a loan, a return or a future Need for this item");
        }
        if let Some(loan) = &self.loan {
            loan.bounds()?;
            if self.exchange.is_none() { return Err("a loan needs a lender and borrower"); }
        }
        for link in [&self.return_of, &self.future_need_for].into_iter().flatten() { link.validate()?; }
        for (_, field) in self.disclosure.fields() {
            if field.people.len() > 100
                || field.people.iter().any(|p| p.is_empty() || p.len() > 512)
            {
                return Err("item field audience is invalid");
            }
            if field.scope != AudienceScope::Selected && !field.people.is_empty() {
                return Err("only a selected audience can list People");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ItemAccess {
    pub title: bool,
    pub description: bool,
    pub source: bool,
    pub parties: bool,
    pub quantity: bool,
    pub location: bool,
}

impl ItemAccess {
    pub fn for_item(
        item: Option<&TransferItem>,
        viewer: Option<&str>,
        owner: Option<&str>,
        participant: bool,
        privileged: bool,
    ) -> Self {
        let obligation_owner = viewer
            .filter(|viewer| {
                item.and_then(|item| item.exchange.as_ref())
                    .is_some_and(|exchange| exchange.involves(viewer))
            })
            .or(owner);
        let allows = |field: &FieldAudience| {
            privileged || field.allows(viewer, obligation_owner, participant)
        };
        match item {
            Some(item) => Self {
                title: allows(&item.disclosure.title),
                description: allows(&item.disclosure.description),
                source: privileged || item.disclosure.source.allows(viewer, None, participant),
                parties: allows(&item.disclosure.parties),
                quantity: allows(&item.disclosure.quantity),
                location: allows(&item.disclosure.location),
            },
            None => Self {
                title: true,
                description: true,
                source: true,
                parties: true,
                quantity: true,
                location: true,
            },
        }
    }

    pub fn complete(self) -> bool {
        self.title
            && self.description
            && self.source
            && self.parties
            && self.quantity
            && self.location
    }
}

fn hide(row: &mut Value, fields: &[&str]) {
    if let Some(object) = row.as_object_mut() {
        for field in fields {
            object.insert((*field).into(), Value::Null);
        }
    }
}

fn hide_blocker_details(value: &mut Value) {
    if let Some(blockers) = value.as_array_mut() {
        for blocker in blockers {
            if blocker.is_object() {
                *blocker = json!({"code":blocker.get("code")});
            }
        }
    }
}

fn hide_private_accounting(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in [
                "record_quantity",
                "availability",
                "application",
                "settlement_preview",
                "private_applications",
                "recorded_effects",
                "private_compensated_slices",
                "private_settlement_slices",
                "compensation",
                "formula",
            ] {
                object.remove(key);
            }
            for child in object.values_mut() {
                hide_private_accounting(child);
            }
        }
        Value::Array(values) => {
            for value in values {
                hide_private_accounting(value);
            }
        }
        _ => {}
    }
}

fn cancellation_action(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| {
            [
                "action",
                "transfer",
                "occurrence",
                "cancellation",
                "expected_revision",
                "expected_remaining_quantity",
                "person",
                "request_id",
            ]
            .contains(&key.as_str())
        });
    }
    value
}

fn redact_item(row: &mut Value, access: ItemAccess) {
    if !access.parties || !access.quantity {
        hide(row, &["loan", "accepted_loan", "return_of", "future_need_for"]);
    }
    let cancellation_action_payload = (access.quantity && access.parties)
        .then(|| cancellation_action(&row["action_payloads"]["propose_cancellation"]))
        .filter(Value::is_object);
    let cancellations = access.quantity.then(|| row["cancellations"].as_array().map(|rows| rows.iter().map(|entry| {
        let mut result = json!({"uid":entry["uid"],"revision":entry["revision"],"quantity":entry["quantity"],"status":entry["status"],"proposal_fact":entry["proposal_fact"],"applied_fact":entry["applied_fact"],"all_agreed":entry["all_agreed"]});
        if access.parties {
            result["capabilities"] = json!({"apply_cancellation":entry["capabilities"]["apply_cancellation"]});
            result["action_payloads"] = json!({"apply_cancellation":cancellation_action(&entry["action_payloads"]["apply_cancellation"])});
        }
        result
    }).collect::<Vec<_>>())).flatten();
    if !access.title {
        hide(row, &["title"]);
    }
    if !access.description {
        hide(row, &["description"]);
    }
    if !access.source {
        hide(
            row,
            &[
                "record",
                "record_head",
                "record_slug",
                "record_quantity",
                "availability",
                "concept",
                "concept_name",
                "application",
                "settlement_preview",
                "reservation",
                "private_applications",
            ],
        );
    }
    if !access.parties {
        hide_blocker_details(&mut row["agreement_blockers"]);
        hide(
            row,
            &[
                "party",
                "proposer",
                "proposer_head",
                "proposer_slug",
                "giver",
                "receiver",
            ],
        );
    }
    if !access.quantity {
        hide(
            row,
            &[
                "delta",
                "quantity",
                "proposer_delta",
                "remaining_quantity",
                "direction",
                "unit",
                "unit_name",
                "availability",
                "settlement_progress",
                "cancellations",
                "settlements",
                "settlement_preview",
                "remote_settlement_preview",
            ],
        );
    }
    if !access.location {
        hide(row, &["place"]);
    }
    if !access.complete() {
        hide(
            row,
            &[
                "condition",
                "claim_pairs",
                "predecessor",
                "successor",
                "source_promise",
                "action_payloads",
                "activation",
                "dispute",
                "delivery",
                "receipt",
                "system_dispute",
                "claim_history",
            ],
        );
        if let Some(progress) = row
            .get_mut("settlement_progress")
            .and_then(Value::as_object_mut)
        {
            progress.remove("slices");
        }
        if let Some(object) = row.as_object_mut() {
            object.retain(|key, _| {
                [
                    "uid",
                    "revision",
                    "title",
                    "description",
                    "record",
                    "record_head",
                    "record_slug",
                    "record_quantity",
                    "concept",
                    "concept_name",
                    "unit",
                    "unit_name",
                    "delta",
                    "quantity",
                    "direction",
                    "state",
                    "status",
                    "party",
                    "open",
                    "proposer",
                    "proposer_head",
                    "proposer_slug",
                    "withdrawn",
                    "window_start",
                    "window_end",
                    "place",
                    "reuse_policy",
                    "condition",
                    "reserve_from",
                    "agreement_ready",
                    "agreement_eligible",
                    "agreement_blockers",
                    "capabilities",
                    "blocking_reasons",
                    "promise",
                    "opposite_promise",
                    "exchange_path",
                    "exchange",
                    "loan",
                    "accepted_loan",
                    "return_of",
                    "future_need_for",
                    "giver",
                    "receiver",
                    "delivery_claimed",
                    "receipt_claimed",
                    "confirmed_conclusion",
                    "disputed",
                    "settlement_progress",
                ]
                .contains(&key.as_str())
            });
        }
    }
    if let Some(cancellations) = cancellations {
        row["cancellations"] = json!(cancellations);
    }
    if let Some(action) = cancellation_action_payload {
        if !row["action_payloads"].is_object() {
            row["action_payloads"] = json!({});
        }
        row["action_payloads"]["propose_cancellation"] = action;
    }
    row["disclosed"] = json!({"title":access.title,"description":access.description,"source":access.source,"parties":access.parties,"quantity":access.quantity,"location":access.location});
}

pub fn project_transfer(row: &mut Value, viewer: Option<&str>, privileged: bool) {
    if let Some(object) = row.as_object_mut() {
        object.remove("private_applications");
    }
    let has_items = row["promises"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|promise| promise.get("item").is_some_and(|item| !item.is_null()));
    if row["promises"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|promise| {
            promise.get("item").is_some_and(|item| {
                !item.is_null() && serde_json::from_value::<TransferItem>(item.clone()).is_err()
            })
        })
    {
        *row = json!({"kind":"transfer","uid":row["uid"],"disclosure_error":"invalid_item"});
        return;
    }
    let participant = row["parties"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|party| viewer.is_some() && party["actor"].as_str() == viewer)
        || row["invitations"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|invitation| {
                viewer.is_some() && invitation["addressed_person"].as_str() == viewer
            });
    let mut access_by_promise = BTreeMap::new();
    let mut visible_people = BTreeSet::new();
    if let Some(viewer) = viewer {
        visible_people.insert(viewer.to_string());
    }
    if let Some(promises) = row["promises"].as_array_mut() {
        for promise in promises {
            let item = promise
                .get("item")
                .filter(|item| !item.is_null())
                .map(|item| serde_json::from_value::<TransferItem>(item.clone()));
            let item = match item {
                Some(Ok(item)) => Some(item),
                Some(Err(_)) => {
                    unreachable!("item shape validated before projection");
                }
                None => None,
            };
            let access = ItemAccess::for_item(
                item.as_ref(),
                viewer,
                promise["party"].as_str(),
                participant,
                privileged,
            );
            if access.parties
                && let Some(person) = promise["party"].as_str()
            {
                visible_people.insert(person.into());
            }
            if let Some(item) = item {
                if let Some(exchange) = &item.exchange {
                    promise["exchange"] = json!(exchange.uid);
                    promise["giver"] = json!(exchange.giver);
                    promise["receiver"] = json!(exchange.receiver);
                    if access.parties {
                        visible_people.insert(exchange.giver.clone());
                        visible_people.insert(exchange.receiver.clone());
                    }
                }
                promise["title"] = json!(item.title);
                promise["description"] = json!(item.description);
                promise["loan"] = json!(item.loan);
                promise["return_of"] = json!(item.return_of);
                promise["future_need_for"] = json!(item.future_need_for);
            }
            if !privileged && !participant {
                hide(promise, &["return_of", "future_need_for", "accepted_loan"]);
            }
            if let Some(uid) = promise["uid"].as_str() {
                access_by_promise.insert(uid.to_string(), access);
            }
            if !privileged && let Some(object) = promise.as_object_mut() {
                object.remove("item");
            }
            redact_item(promise, access);
        }
    }
    let restricted = access_by_promise.values().any(|access| !access.complete());
    let hidden_people = access_by_promise.values().any(|access| !access.parties);
    let hidden_source = access_by_promise.values().any(|access| !access.source);
    let hidden_quantity = access_by_promise.values().any(|access| !access.quantity);
    let hidden_location = access_by_promise.values().any(|access| !access.location);
    if has_items && !privileged {
        hide_private_accounting(row);
    }
    if let Some(occurrences) = row["occurrences"].as_array_mut() {
        for occurrence in occurrences {
            if let Some(access) = occurrence["promise"]
                .as_str()
                .and_then(|uid| access_by_promise.get(uid))
                .copied()
            {
                redact_item(occurrence, access);
            }
        }
    }
    if restricted || (has_items && !privileged) {
        let shared_threads: Vec<_> = row["threads"].as_array().into_iter().flatten().filter_map(|thread| {
            let messages: Vec<_> = thread["messages"].as_array().into_iter().flatten().filter_map(|message| {
                let shared = crate::simulation::sharing::Shared::parse(message["body"].as_str()?)?;
                shared.disclosed(row["uid"].as_str()?, row["revision"].as_u64()?, &access_by_promise).then(|| {
                    json!({"uid":message["uid"], "created_at":message["created_at"], "body":serde_json::to_string(&shared).unwrap(),
                        "sender":message.get("sender").filter(|sender| !hidden_people || sender.as_str().is_some_and(|person| visible_people.contains(person)))})
                })
            }).collect();
            (!messages.is_empty()).then(|| json!({"uid":thread["uid"],"head":"Shared simulations","messages":messages}))
        }).collect();
        let authorship = |proof: &Value| {
            json!({
                "fact": proof.get("fact"),
                "actor": proof.get("actor").filter(|actor| !hidden_people || actor.as_str().is_some_and(|person| visible_people.contains(person))),
                "at": proof.get("at"),
                "state": proof.get("state"),
                "authoritative": proof.get("authoritative"),
                "terms_disclosed": false,
            })
        };
        let history = row["timeline"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|event| event.is_object())
            .map(|event| {
                json!({
                    "uid":event.get("uid"), "kind":event.get("kind"), "at":event.get("at"),
                    "fact":event.get("fact"), "proof":authorship(&event["proof"]),
                })
            })
            .collect::<Vec<_>>();
        let proof = row
            .get("proof")
            .filter(|proof| proof.is_object())
            .map(authorship);
        hide(
            row,
            &[
                "revision_evidence",
                "proof",
                "timeline",
                "threads",
                "correction_lineage",
                "first_completes_evidence",
                "confirmations",
                "social_delivery",
                "dependencies",
            ],
        );
        row["timeline"] = json!(history);
        row["threads"] = json!(shared_threads);
        row["proof"] = json!(proof);
        row["history_disclosure"] =
            json!("Full signed history contains fields unavailable to this viewer");
    }
    if hidden_source {
        hide(
            row,
            &[
                "source",
                "source_head",
                "source_slug",
                "balance",
                "balance_detail",
            ],
        );
        row["settlement_progress"]["by_resource"] = Value::Null;
    }
    if hidden_quantity {
        hide(
            row,
            &[
                "balance",
                "balance_detail",
                "balanced",
                "settlement_progress",
            ],
        );
    }
    if hidden_location {
        hide(row, &["default_place"]);
    }
    if hidden_people {
        hide_blocker_details(&mut row["readiness"]["blockers"]);
        if let Some(parties) = row["parties"].as_array_mut() {
            parties.retain(|party| {
                party["actor"]
                    .as_str()
                    .is_some_and(|person| visible_people.contains(person))
            });
            for party in parties {
                hide_blocker_details(&mut party["readiness"]["blockers"]);
            }
        }
        hide(row, &["invitations", "agreement", "viewer_party"]);
    }
    if restricted || (has_items && !privileged) {
        if let Some(object) = row.as_object_mut() {
            object.retain(|key, _| {
                [
                    "kind",
                    "uid",
                    "slug",
                    "head",
                    "revision",
                    "status",
                    "primary_status",
                    "operational_status",
                    "inbox_facets",
                    "active",
                    "agreement_type",
                    "agreement_pct",
                    "settlement",
                    "visibility",
                    "max_proximity",
                    "satiation",
                    "reserve_default",
                    "require_confirmation",
                    "default_place",
                    "viewer_party",
                    "viewer_roles",
                    "timeline",
                    "proof",
                    "history_disclosure",
                    "capabilities",
                    "blocking_reasons",
                    "agreement",
                    "readiness",
                    "outcome",
                    "progress",
                    "settlement_progress",
                    "correction_status",
                    "parties",
                    "promises",
                    "occurrences",
                    "invitations",
                    "threads",
                ]
                .contains(&key.as_str())
            });
        }
        row["visibility_projection"] = json!({"policy":row["visibility"],"scope":"item_fields","disclosure":{"field_overrides_supported":true}});
    } else if row["visibility_projection"].is_object() {
        row["visibility_projection"]["disclosure"]["field_overrides_supported"] = json!(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_simulations_keep_only_typed_authorized_information() {
        let body = json!({"kind":"transfer-simulation", "id":"share-1", "transfer":"transfer", "revision":1, "source_at_ms":10, "duration_ms":100,
            "assumptions":[{"source":{"transfer":"transfer","revision":1,"promise":"promise","exchange":"route","occurrence":null},"after_ms":50,"quantity":{"scale":0,"value":"4"}}],"result":null});
        let mut item = TransferItem {
            title: "Apples".into(),
            ..Default::default()
        };
        item.disclosure.quantity.scope = AudienceScope::Selected;
        item.disclosure.quantity.people = vec!["beto".into()];
        let row = json!({"uid":"transfer","revision":1,"promises":[{"uid":"promise","party":"ana","item":item}],
            "threads":[{"uid":"thread","head":"secret-title","messages":[
                {"uid":"message","body":body.to_string(),"references":["secret-record"]},
                {"uid":"other","body":"secret-text"}]}]});
        let mut beto = row.clone();
        project_transfer(&mut beto, Some("beto"), false);
        assert_eq!(beto["threads"][0]["messages"].as_array().unwrap().len(), 1);
        assert!(!beto.to_string().contains("secret-"));
        assert!(
            crate::simulation::sharing::Shared::parse(
                beto["threads"][0]["messages"][0]["body"].as_str().unwrap()
            )
            .is_some()
        );
        let mut stranger = row;
        project_transfer(&mut stranger, Some("dora"), false);
        assert_eq!(stranger["threads"], json!([]));
        let mut arbitrary = body;
        arbitrary["database"] = json!("secret-db");
        assert!(crate::simulation::sharing::Shared::parse(&arbitrary.to_string()).is_none());
    }

    #[test]
    fn private_source_and_selected_fields_have_distinct_audiences() {
        let mut item = TransferItem {
            title: "City bike".into(),
            ..Default::default()
        };
        assert!(item.disclosure.title.allows(None, Some("ana"), false));
        assert!(
            !item
                .disclosure
                .source
                .allows(Some("beto"), Some("ana"), true)
        );
        assert!(
            item.disclosure
                .source
                .allows(Some("ana"), Some("ana"), false)
        );
        assert!(!item.disclosure.source.allows(None, None, false));
        item.disclosure.location = FieldAudience {
            scope: AudienceScope::Selected,
            people: vec!["courier".into()],
        };
        assert!(
            item.disclosure
                .location
                .allows(Some("courier"), Some("ana"), false)
        );
        assert!(
            !item
                .disclosure
                .location
                .allows(Some("beto"), Some("ana"), true)
        );
        assert!(item.validate().is_ok());
        item.disclosure.location.scope = AudienceScope::Everyone;
        assert!(item.validate().is_err());
    }

    #[test]
    fn sharing_source_identity_never_shares_private_accounting() {
        let mut item = TransferItem {
            title: "Apples".into(),
            ..Default::default()
        };
        item.disclosure.source.scope = AudienceScope::Everyone;
        let mut row = json!({"promises":[{"uid":"p","item":item,"party":"beto",
            "record":"inventory","record_quantity":99,"availability":{"reserved":20},
            "application":{"formula":"private formula"}}]});
        project_transfer(&mut row, Some("beto"), false);
        assert_eq!(row["promises"][0]["record"], "inventory");
        assert!(row["promises"][0]["record_quantity"].is_null());
        assert!(row["promises"][0]["availability"].is_null());
        assert!(!row.to_string().contains("private formula"));
    }

    #[test]
    fn redaction_removes_nested_history_and_keeps_independent_item_text() {
        let mut item = TransferItem {
            title: "City bike".into(),
            description: "Blue frame".into(),
            ..Default::default()
        };
        item.disclosure.parties.scope = AudienceScope::Owner;
        item.disclosure.quantity.scope = AudienceScope::Owner;
        item.disclosure.location.scope = AudienceScope::Selected;
        item.disclosure.location.people.push("courier".into());
        let original = json!({
            "uid":"transfer", "head":"A bike", "visibility":"public",
            "parties":[{"actor":"secret-person","actor_head":"secret-name"}],
            "promises":[{"uid":"promise","item":item,"party":"secret-person","record":"secret-record","record_head":"secret-head","record_quantity":99,"place":{"address":"Collection point"},"delta":17,"unit":"secret-unit","condition":"secret-condition","claim_pairs":["secret-pair"],"availability":{"quantity":99}}],
            "occurrences":[{"uid":"occurrence","promise":"promise","giver":"secret-person","receiver":"secret-receiver","record":"secret-record","quantity":17,"claim_history":["secret-person"],"settlement_progress":{"slices":["secret-record"],"remaining_quantity":17}}],
            "revision_evidence":{"terms":"secret-record"},"timeline":["secret-person"],"proof":{"action_intent":"secret-encoded-action"},
            "source":"secret-record", "balance":{"secret-unit":17}, "invitations":["secret-person"], "threads":["secret-thread"],
            "agreement":{"history":["secret-person"]}, "settlement_progress":{"by_resource":["secret-record"]}, "default_place":{"address":"Collection point"}
        });
        let mut public = original.clone();
        project_transfer(&mut public, None, false);
        assert_eq!(public["promises"][0]["title"], "City bike");
        assert_eq!(public["promises"][0]["description"], "Blue frame");
        assert!(!public.to_string().contains("secret-"), "{public}");
        assert!(!public.to_string().contains("Collection point"));
        assert!(public["promises"][0]["delta"].is_null());
        let mut courier = original.clone();
        project_transfer(&mut courier, Some("courier"), false);
        assert_eq!(
            courier["promises"][0]["place"]["address"],
            "Collection point"
        );
        assert!(!courier.to_string().contains("secret-"), "{courier}");
        let mut owner = original;
        project_transfer(&mut owner, Some("secret-person"), true);
        assert_eq!(owner["promises"][0]["record"], "secret-record");
    }
    #[test]
    fn cancellation_terms_stay_reviewable_without_private_sources() {
        let mut item = TransferItem {
            title: "Apples".into(),
            ..Default::default()
        };
        let original = json!({"uid":"transfer","revision":3,"parties":[{"actor":"ana"},{"actor":"beto"}],
            "promises":[{"uid":"promise","party":"ana","record":"secret-stock","item":item}],
            "occurrences":[{"uid":"occurrence","promise":"promise","record":"secret-stock",
                "cancellations":[{"uid":"cancel","revision":3,"quantity":{"scale":0,"value":"6"},"status":"pending","private":"secret-stock","capabilities":{"apply_cancellation":true},
                    "action_payloads":{"apply_cancellation":{"action":"apply-transfer-cancellation","person":"beto","transfer":"transfer","cancellation":"cancel","expected_revision":3,"record":"secret-stock"}}}],
                "action_payloads":{"propose_cancellation":{"action":"propose-transfer-cancellation","transfer":"transfer","occurrence":"occurrence","person":"beto","expected_revision":3,"expected_remaining_quantity":{"scale":0,"value":"6"},"record":"secret-stock"}}}]
        });
        let mut recipient = original.clone();
        project_transfer(&mut recipient, Some("beto"), false);
        assert_eq!(
            recipient["occurrences"][0]["cancellations"][0]["quantity"]["value"],
            "6"
        );
        assert_eq!(
            recipient["occurrences"][0]["cancellations"][0]["action_payloads"]["apply_cancellation"]
                ["person"],
            "beto"
        );
        assert_eq!(
            recipient["occurrences"][0]["action_payloads"]["propose_cancellation"]["person"],
            "beto"
        );
        assert!(!recipient.to_string().contains("secret-stock"));
        item.disclosure.quantity.scope = AudienceScope::Owner;
        let mut public = original;
        public["promises"][0]["item"] = json!(item);
        project_transfer(&mut public, None, false);
        assert!(public["occurrences"][0]["cancellations"].is_null());
        assert!(public["occurrences"][0]["action_payloads"].is_null());
    }
}
