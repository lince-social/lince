use nucleus::karma::rule_field::RuleFieldKind;
use nucleus::transfer::karma::Snapshot;
use serde_json::Value;

pub fn snapshot(row: &Value) -> Option<Snapshot> {
    let state: Snapshot = serde_json::from_value(row.get("karma_state")?.clone()).ok()?;
    state.validate().ok()?;
    (row["uid"].as_str() == Some(state.transfer.as_str())).then_some(state)
}

pub fn retreated(before: &Snapshot, after: &Snapshot, person: &str) -> bool {
    before.transfer == after.transfer
        && before.revision == after.revision
        && before
            .participants
            .get(person)
            .zip(after.participants.get(person))
            .is_some_and(|(before, after)| after.guard.level < before.guard.level)
}

pub fn label(element: &str, rows: &[Value]) -> String {
    let mut label = element.to_owned();
    for row in rows {
        if let Some(uid) = row["uid"].as_str() {
            if let Some(name) = row["head"].as_str().filter(|name| !name.is_empty()) {
                label = label.replace(&format!("@{uid}"), name);
            }
        }
        for party in row["parties"].as_array().into_iter().flatten() {
            if let Some(uid) = party["actor"].as_str() {
                if let Some(name) = party["actor_head"].as_str().filter(|name| !name.is_empty()) {
                    label = label.replace(&format!("@{uid}"), name);
                }
            }
        }
    }
    label
}

pub fn elements(kind: RuleFieldKind, rows: &[Value], acting: Option<&str>) -> Vec<String> {
    let mut values = Vec::new();
    for row in rows {
        let Some(state) = snapshot(row) else {
            continue;
        };
        match kind {
            RuleFieldKind::Condition => {
                for reading in [
                    "transfer_revision",
                    "transfer_active",
                    "transfer_published",
                    "transfer_ready",
                ] {
                    values.push(format!("{reading}(@{})", state.transfer));
                }
                for person in state.participants.keys() {
                    for reading in ["agreement_level", "agreement_changed_at", "agreement_age"] {
                        values.push(format!("{reading}(@{}, @{person})", state.transfer));
                    }
                }
            }
            RuleFieldKind::Consequence => {
                let Some(person) = acting.filter(|person| state.participants.contains_key(*person))
                else {
                    continue;
                };
                let target = format!("@{}", state.transfer);
                values.push(format!("{target}: agreement(@{person})"));
                for level in 0..=2 {
                    values.push(format!("{target}: agreement(@{person}, {level})"));
                }
                values.push(format!("{target}: agreement(@{person}, 2, 3d)"));
                values.push(format!("{target}: publish(@{person})"));
                for promise in row["promises"].as_array().into_iter().flatten() {
                    if let Some(promise) = promise["uid"].as_str() {
                        values.push(format!(
                            "{target}: activate(@{person}, @{promise}, \"once\")"
                        ));
                    }
                }
            }
            RuleFieldKind::Threshold => {}
        }
    }
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use nucleus::transfer::{AgreementGuard, karma::Participant};

    use super::*;

    fn state(level: u8) -> Snapshot {
        Snapshot {
            transfer: "r_K7T3ZG08G5EBWRSTF8NBWMTZYV".into(),
            revision: 1,
            active: false,
            published: false,
            ready: false,
            participants: BTreeMap::from([(
                "r_K7T3ZG08G5EBWRSTF8NBWMTZYW".into(),
                Participant {
                    guard: AgreementGuard {
                        level,
                        change_uid: Some(format!("change-{level}")),
                    },
                    changed_at_ms: Some(0),
                },
            )]),
        }
    }

    #[test]
    fn retreat_requires_an_actual_decrease_on_the_same_transfer_terms() {
        let before = state(2);
        let mut after = state(1);
        let person = before.participants.keys().next().unwrap();
        assert!(retreated(&before, &after, person));
        assert!(!retreated(&after, &before, person));
        assert!(!retreated(&before, &before, person));
        after.revision += 1;
        assert!(!retreated(&before, &after, person));
    }

    #[test]
    fn transfer_templates_use_stable_ids_and_an_explicit_fulfillment_key() {
        let state = state(0);
        let person = state.participants.keys().next().unwrap();
        let row = serde_json::json!({"uid":state.transfer,"karma_state":state,"promises":[{"uid":"promise-a"}]});
        let readings = elements(RuleFieldKind::Condition, &[row.clone()], Some(person));
        assert_eq!(readings.len(), 7);
        for source in readings {
            nucleus::karma::Condition::parse(&source).unwrap();
        }
        let effects = elements(RuleFieldKind::Consequence, &[row.clone()], Some(person));
        assert_eq!(effects.len(), 7);
        for source in &effects {
            nucleus::karma::rule_field::RuleConsequence::parse(source).unwrap();
        }
        assert!(effects.iter().any(|source| source.ends_with("\"once\")")));
        assert!(elements(RuleFieldKind::Consequence, &[row], None).is_empty());
    }
}
