//! Shared indexes and row details used by Transfer projections.

use super::viewer::{AgreementTransition, TransferViewer};
use nucleus::performance::{Span, add};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use store::{
    Store, StoreError, records::RecordRow, transfer_agreement::AgreementReadinessProjection,
    transfer_loans::accepted, transfers::TransferAgreementReadinessInput,
};

pub(super) type RecordIndex<'a> = HashMap<&'a str, &'a RecordRow<Option<String>>>;

pub(super) fn attach_action_access(row: &mut Value, actions: &[(&str, &[&str])]) {
    let mut capabilities = Map::new();
    let mut reasons = Map::new();
    for &(action, blockers) in actions {
        capabilities.insert(action.into(), blockers.is_empty().into());
        reasons.insert(action.into(), blockers.into());
    }
    row["capabilities"] = Value::Object(capabilities);
    row["blocking_reasons"] = Value::Object(reasons);
}

pub(super) struct AgreementOptions {
    pub review: Vec<&'static str>,
    pub commit: Vec<&'static str>,
    pub back: Vec<&'static str>,
}

#[derive(Default)]
struct OptionSummary(Option<Vec<&'static str>>);

impl OptionSummary {
    fn allowed(&self) -> bool {
        self.0.as_ref().is_some_and(Vec::is_empty)
    }

    fn consider(&mut self, attempt: impl FnOnce() -> Vec<&'static str>) {
        if self.allowed() {
            return;
        }
        let blockers = attempt();
        if self.0.is_none() || blockers.is_empty() {
            self.0 = Some(blockers);
        }
    }

    fn finish(self, absent: &'static str) -> Vec<&'static str> {
        self.0.unwrap_or_else(|| vec![absent])
    }
}

pub(super) fn agreement_options(
    input: &TransferAgreementReadinessInput,
    viewer: &TransferViewer,
    revision_signed: bool,
    coalition_members: Option<&HashSet<&str>>,
    installed_signer: Option<&str>,
) -> AgreementOptions {
    let _options = Span::new("protein-transfer-agreement-options");
    let mut review = OptionSummary::default();
    let mut commit = OptionSummary::default();
    let mut back = OptionSummary::default();
    for party in input
        .parties
        .iter()
        .filter(|party| viewer.local || viewer.person.as_deref() == Some(party.person_uid.as_str()))
    {
        let transition = AgreementTransition {
            person: &party.person_uid,
            party: &party.party_uid,
            level: party.level,
            revision: input.revision,
            revision_signed,
            coalition: coalition_members,
            has_signer: installed_signer == Some(party.person_uid.as_str()),
        };
        let blockers = |target| {
            add("protein-transfer-agreement-option-checks", 1);
            viewer.agreement_transition_blockers(&transition, target)
        };
        match party.level {
            0 => review.consider(|| blockers(1)),
            1 => commit.consider(|| blockers(2)),
            _ => {}
        }
        if party.level > 0 {
            back.consider(|| blockers(party.level - 1));
        }
        if review.allowed() && commit.allowed() && back.allowed() {
            break;
        }
    }
    AgreementOptions {
        review: review.finish("no_party_can_enter_checked_level"),
        commit: commit.finish("no_party_can_enter_agreed_level"),
        back: back.finish("no_party_can_move_agreement_back"),
    }
}

pub(super) fn first_by_key<'a, T>(
    rows: &'a [T],
    key: impl Fn(&'a T) -> &'a str,
) -> HashMap<&'a str, &'a T> {
    let _index = Span::new("protein-transfer-projection-index");
    let mut index = HashMap::new();
    for row in rows {
        index.entry(key(row)).or_insert(row);
    }
    add("protein-transfer-index-source-rows", rows.len() as u64);
    index
}

pub(super) struct Blocker<'a> {
    pub promise: &'a str,
    pub value: &'a Value,
}

pub(super) fn party_blockers<'a>(
    parties: &'a [(String, String, i64)],
    input: &'a TransferAgreementReadinessInput,
    readiness: &'a AgreementReadinessProjection,
    recipient_details_authorized: bool,
    viewer: Option<&str>,
) -> HashMap<&'a str, Vec<Blocker<'a>>> {
    let _groups = Span::new("protein-transfer-blocker-groups");
    let mut groups: HashMap<_, Vec<_>> = parties
        .iter()
        .filter(|(_, actor, _)| recipient_details_authorized || viewer == Some(actor.as_str()))
        .map(|(_, actor, _)| (actor.as_str(), Vec::new()))
        .collect();
    if groups.is_empty() {
        return groups;
    }
    let mut references = 0;
    for promise in &input.promises {
        let Some(group) = promise
            .person_uid
            .as_deref()
            .and_then(|person| groups.get_mut(person))
        else {
            continue;
        };
        let Some(readiness) = readiness
            .promises
            .get(&promise.uid)
            .filter(|readiness| readiness.eligible && !readiness.ready)
        else {
            continue;
        };
        references += readiness.blockers.len() as u64;
        group.extend(readiness.blockers.iter().map(|value| Blocker {
            promise: promise.uid.as_str(),
            value,
        }));
    }
    add("protein-transfer-blocker-references", references);
    groups
}

fn exchange_uid(promise: &Value) -> Option<&str> {
    promise["item"]["exchange"]["uid"]
        .as_str()
        .or(promise["exchange"].as_str())
}

pub(super) async fn attach_loans(
    store: &Store,
    transfer: &str,
    value: &mut Value,
) -> Result<(), StoreError> {
    let _projection = Span::new("protein-transfer-loan-projection-await");
    let Some(promises) = value.get_mut("promises").and_then(Value::as_array_mut) else {
        add("protein-transfer-loan-read-skipped", 1);
        return Ok(());
    };
    let loans = if promises
        .iter()
        .any(|promise| exchange_uid(promise).is_some())
    {
        accepted(&store.pool, transfer).await?
    } else {
        add("protein-transfer-loan-read-skipped", 1);
        Vec::new()
    };
    let by_exchange = first_by_key(&loans, |loan| loan.exchange.as_str());
    for promise in promises {
        let loan = exchange_uid(promise).and_then(|exchange| by_exchange.get(exchange));
        if let Some(promise) = promise.as_object_mut() {
            promise.insert("accepted_loan".into(), json!(loan));
        }
    }
    Ok(())
}
