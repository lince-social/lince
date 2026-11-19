use nucleus::karma::rule_field::RuleFieldKind;
use nucleus::transfer::karma::Snapshot;
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;

pub fn snapshot(row: &Value) -> Option<Snapshot> {
    let state: Snapshot = Snapshot::deserialize(row.get("karma_state")?).ok()?;
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

fn names(rows: &[Value]) -> impl Iterator<Item = (&str, &str)> {
    rows.iter().flat_map(|row| {
        row["uid"]
            .as_str()
            .zip(row["head"].as_str().filter(|name| !name.is_empty()))
            .into_iter()
            .chain(
                row["parties"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|party| {
                        party["actor"]
                            .as_str()
                            .zip(party["actor_head"].as_str().filter(|name| !name.is_empty()))
                    }),
            )
    })
}

fn replace<'a>(element: &str, names: impl Iterator<Item = (Cow<'a, str>, &'a str)>) -> String {
    let mut label = element.to_owned();
    for (needle, name) in names {
        if needle.as_ref() != name && label.contains(needle.as_ref()) {
            label = label.replace(needle.as_ref(), name);
        }
    }
    label
}

pub fn label(element: &str, rows: &[Value]) -> String {
    replace(
        element,
        names(rows).map(|(uid, name)| (format!("@{uid}").into(), name)),
    )
}

#[derive(Default)]
pub struct Labels(Vec<(String, String)>);

impl Labels {
    pub fn new(rows: &[Value]) -> Self {
        Self(
            names(rows)
                .map(|(uid, name)| (format!("@{uid}"), name.to_owned()))
                .collect(),
        )
    }

    pub fn label(&self, element: &str) -> String {
        replace(
            element,
            self.0
                .iter()
                .map(|(needle, name)| (needle.as_str().into(), name.as_str())),
        )
    }
}

pub fn elements(kind: RuleFieldKind, rows: &[Value], acting: Option<&str>) -> Vec<String> {
    if kind == RuleFieldKind::Threshold || kind == RuleFieldKind::Consequence && acting.is_none() {
        return Vec::new();
    }
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
                values.push(format!("{target}: repeat(@{person})"));
                values.push(format!("{target}: repeat(@{person}, \"publish\")"));
                values.push(format!("{target}: repeat(@{person}, \"commit\")"));
                values.push(format!("{target}: repeat(@{person}, \"activate\")"));
                values.push(format!("{target}: reopen(@{person})"));
                values.push(format!("{target}: reopen(@{person}, 14)"));
                for promise in row["promises"].as_array().into_iter().flatten() {
                    if let Some(promise) = promise["uid"].as_str() {
                        values.push(format!(
                            "{target}: activate(@{person}, @{promise}, \"$occurrence\")"
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

#[derive(Clone, Copy)]
pub enum Automation {
    Reopen,
    Repeat,
}

pub fn automation(row: &Value, person: &str, mode: Automation) -> Result<super::Draft, String> {
    let state = snapshot(row).ok_or("Refresh the Transfer before configuring automation")?;
    if !state.participants.contains_key(person) {
        return Err("Choose a participating Person".into());
    }
    let (name, condition, threshold, consequence) = match mode {
        Automation::Reopen => (
            "Reopen Need",
            "1".to_owned(),
            "> 0",
            format!("@{}: reopen(@{person})", state.transfer),
        ),
        Automation::Repeat => (
            "Repeat Transfer",
            String::new(),
            "> 0",
            format!("@{}: repeat(@{person})", state.transfer),
        ),
    };
    Ok(super::Draft {
        name: name.into(),
        fields: [condition, threshold.into(), consequence]
            .map(|text| super::FieldDraft { text, linked: None }),
        ..Default::default()
    })
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
    fn automation_drafts_share_normal_karma_fields_and_require_frequency_for_repeat() {
        let state = state(0);
        let person = state.participants.keys().next().unwrap();
        let row = serde_json::json!({"uid":state.transfer,"karma_state":state});
        let reopen = automation(&row, person, Automation::Reopen).unwrap();
        assert!(reopen.fields[2].text.contains("reopen("));
        assert!(reopen.valid());
        let mut repeat = automation(&row, person, Automation::Repeat).unwrap();
        assert!(repeat.fields[0].text.is_empty());
        repeat.fields[0].text = "freq(@weekly)".into();
        assert!(repeat.valid());
        assert!(repeat.rule.is_none());
        assert!(automation(&row, "another-person", Automation::Repeat).is_err());
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
        assert_eq!(effects.len(), 13);
        for source in &effects {
            nucleus::karma::rule_field::RuleConsequence::parse(source).unwrap();
        }
        assert!(
            effects
                .iter()
                .any(|source| source.ends_with("\"$occurrence\")"))
        );
        assert!(elements(RuleFieldKind::Consequence, &[row], None).is_empty());
    }
}
