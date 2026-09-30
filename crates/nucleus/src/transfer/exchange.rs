use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{TransferRevisionPromise, TransferRevisionSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExchangeRoute {
    pub uid: String,
    pub giver: String,
    pub receiver: String,
}

impl ExchangeRoute {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.uid.trim().is_empty() || self.uid.len() > 200 {
            return Err("exchange identity must contain 1 to 200 characters");
        }
        if self.giver.is_empty() || self.receiver.is_empty() || self.giver == self.receiver {
            return Err("an exchange requires different giver and receiver People");
        }
        Ok(())
    }

    pub fn involves(&self, person: &str) -> bool {
        self.giver == person || self.receiver == person
    }

    pub fn owner(&self, delta: f64) -> &str {
        if delta < 0.0 {
            &self.giver
        } else {
            &self.receiver
        }
    }
}

pub fn validate_exchanges(snapshot: &TransferRevisionSnapshot) -> Result<(), &'static str> {
    let people = snapshot
        .parties
        .iter()
        .map(|party| party.person_uid.as_str())
        .chain(
            snapshot
                .invitations
                .iter()
                .map(|invitation| invitation.addressed_person_uid.as_str()),
        )
        .collect::<BTreeSet<_>>();
    let mut routes: BTreeMap<&str, Vec<&TransferRevisionPromise>> = BTreeMap::new();
    for promise in snapshot
        .promises
        .iter()
        .filter(|promise| promise.state != "withdrawn")
    {
        let Some(route) = promise
            .item
            .as_ref()
            .and_then(|item| item.exchange.as_ref())
        else {
            continue;
        };
        route.validate()?;
        if promise.state == "open"
            || promise.person_uid.as_deref() != Some(route.owner(promise.delta))
        {
            return Err(
                "the exchange route must match the responsible Person and quantity direction",
            );
        }
        if !people.contains(route.giver.as_str()) || !people.contains(route.receiver.as_str()) {
            return Err("exchange People must be participants or addressed invitees");
        }
        routes.entry(&route.uid).or_default().push(promise);
    }
    for promises in routes.values() {
        if promises.len() > 2 {
            return Err("split contributions require separate exchange identities");
        }
        if let [left, right] = promises.as_slice() {
            let left_item = left.item.as_ref().unwrap();
            let right_item = right.item.as_ref().unwrap();
            if left_item.exchange != right_item.exchange
                || left_item.loan != right_item.loan
                || left_item.return_of != right_item.return_of
                || left_item.future_need_for != right_item.future_need_for
                || left_item.title != right_item.title
                || left_item.description != right_item.description
                || left.person_uid == right.person_uid
                || left.delta != -right.delta
                || left.unit_uid != right.unit_uid
                || left.window_start != right.window_start
                || left.window_end != right.window_end
                || left.location != right.location
            {
                return Err(
                    "both sides of an exchange must describe the same item, route, amount, unit and timing",
                );
            }
        }
    }
    Ok(())
}
