use crate::NucleusError;
use crate::karma::{Cadence, Carry, Condition, ConditionError, DecimalValue, ExactResolver, Gate};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

const VALUE_DEPTH_CAP: usize = 4;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now: DateTime<Utc>,
    pub quantities: HashMap<String, f64>,
    pub slugs: HashMap<String, String>,
    pub promises: Vec<ProjPromise>,
    pub rules: Vec<ProjRule>,
}

#[derive(Debug, Clone)]
pub struct ProjPromise {
    pub record_uid: String,
    pub delta: f64,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjMove {
    Add,
    Set,
}

#[derive(Debug, Clone)]
pub struct ProjRule {
    pub uid: String,
    pub slug: Option<String>,
    pub record_uid: String,
    pub cadence: Cadence,
    pub anchor: DateTime<Utc>,
    pub condition: Option<ProjCondition>,
    pub movement: Option<(ProjMove, f64)>,
}

#[derive(Debug, Clone)]
pub struct ProjCondition {
    pub condition: Condition,
    pub gate: Gate,
    pub carry: Carry,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelinePoint {
    pub at: DateTime<Utc>,
    pub record_uid: String,
    pub quantity: f64,
    pub cause: String,
}

#[derive(Debug, Clone)]
pub struct Timeline {
    pub points: Vec<TimelinePoint>,
    pub final_state: HashMap<String, f64>,
}

impl Timeline {
    pub fn crossing_below(&self, record_uid: &str, threshold: f64) -> Option<&TimelinePoint> {
        self.points
            .iter()
            .find(|p| p.record_uid == record_uid && p.quantity <= threshold)
    }

    pub fn projected(&self, record_uid: &str) -> Option<f64> {
        self.final_state.get(record_uid).copied()
    }
}

struct VirtualResolver<'a> {
    snapshot: &'a Snapshot,
    quantities: &'a HashMap<String, f64>,
    since: DateTime<Utc>,
    at: DateTime<Utc>,
    depth: usize,
}

impl VirtualResolver<'_> {
    fn uid_of(&self, token: &str) -> Option<String> {
        let token = token.trim_start_matches('@');
        self.snapshot.slugs.get(token).cloned().or_else(|| {
            self.quantities
                .contains_key(token)
                .then(|| token.to_string())
        })
    }

    fn rule_on(&self, record_uid: &str) -> Option<&ProjRule> {
        self.snapshot
            .rules
            .iter()
            .find(|rule| rule.record_uid == record_uid)
    }
}

fn exact(value: f64) -> Result<DecimalValue, ConditionError> {
    DecimalValue::from_f64_lossy(value)
        .map_err(|_| ConditionError::UnknownReference("a level too large to read".into()))
}

impl ExactResolver for VirtualResolver<'_> {
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        _window_secs: Option<i64>,
    ) -> Result<DecimalValue, ConditionError> {
        match func {
            "quantity" | "signal" => {
                let uid = self
                    .uid_of(slug)
                    .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))?;
                exact(self.quantities.get(&uid).copied().unwrap_or(0.0))
            }
            "freq" => {
                let uid = self
                    .uid_of(slug)
                    .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))?;
                let Some(rhythm) = self.rule_on(&uid) else {
                    return exact(0.0);
                };
                let tick = chrono::Duration::milliseconds(1);
                let (Some(from), Some(to)) = (
                    self.since.checked_add_signed(tick),
                    self.at.checked_add_signed(tick),
                ) else {
                    return exact(0.0);
                };
                let landed = rhythm
                    .cadence
                    .between(rhythm.anchor, from, to)
                    .map(|derived| derived.len())
                    .unwrap_or(0);
                exact(landed as f64)
            }
            "value" => {
                if self.depth >= VALUE_DEPTH_CAP {
                    return Err(ConditionError::UnknownReference(
                        "these rules read each other in a circle".into(),
                    ));
                }
                let uid = self
                    .uid_of(slug)
                    .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))?;
                let rule = self
                    .rule_on(&uid)
                    .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))?;
                let asked = rule
                    .condition
                    .as_ref()
                    .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))?;
                let mut inner = VirtualResolver {
                    depth: self.depth + 1,
                    ..*self
                };
                asked.condition.evaluate(&mut inner)
            }
            other => Err(ConditionError::UnknownReference(format!(
                "{other}() in projection"
            ))),
        }
    }
}

pub fn project(snapshot: &Snapshot, until: DateTime<Utc>) -> Timeline {
    let mut state = snapshot.quantities.clone();
    let mut points = Vec::new();

    enum Ev {
        Promise(usize),
        Rule(usize, DateTime<Utc>),
    }

    let mut events: Vec<(DateTime<Utc>, Ev)> = Vec::new();
    for (index, promise) in snapshot.promises.iter().enumerate() {
        if promise.at > snapshot.now && promise.at <= until {
            events.push((promise.at, Ev::Promise(index)));
        }
    }
    for (index, rule) in snapshot.rules.iter().enumerate() {
        let Ok(derived) = rule.cadence.between(rule.anchor, snapshot.now, until) else {
            continue;
        };
        let mut previous = rule
            .cadence
            .preceding(rule.anchor, snapshot.now)
            .ok()
            .flatten()
            .unwrap_or(rule.anchor);
        for at in derived {
            events.push((at, Ev::Rule(index, previous)));
            previous = at;
        }
    }
    events.sort_by_key(|(at, event)| {
        (
            *at,
            match event {
                Ev::Promise(_) => 0usize,
                Ev::Rule(..) => 1,
            },
        )
    });

    for (at, event) in events {
        match event {
            Ev::Promise(index) => {
                let promise = &snapshot.promises[index];
                let level = state.entry(promise.record_uid.clone()).or_insert(0.0);
                *level += promise.delta;
                points.push(TimelinePoint {
                    at,
                    record_uid: promise.record_uid.clone(),
                    quantity: *level,
                    cause: "promise".into(),
                });
            }
            Ev::Rule(index, since) => {
                let rule = &snapshot.rules[index];
                let Some((movement, declared)) = rule.movement else {
                    continue;
                };
                let mut number = declared;
                if let Some(asked) = rule.condition.as_ref() {
                    let mut resolver = VirtualResolver {
                        snapshot,
                        quantities: &state,
                        since,
                        at,
                        depth: 0,
                    };
                    match crate::karma::decide(
                        &asked.condition,
                        &asked.gate,
                        &asked.carry,
                        &mut resolver,
                    ) {
                        Ok(None) | Err(_) => continue,
                        Ok(Some(carried)) => number = carried.to_f64(),
                    }
                }
                let current = state.get(&rule.record_uid).copied().unwrap_or(0.0);
                let next = match movement {
                    ProjMove::Add => current + number,
                    ProjMove::Set => number,
                };
                if next != current {
                    state.insert(rule.record_uid.clone(), next);
                    points.push(TimelinePoint {
                        at,
                        record_uid: rule.record_uid.clone(),
                        quantity: next,
                        cause: format!("rule:{}", rule.uid),
                    });
                }
            }
        }
    }

    Timeline {
        points,
        final_state: state,
    }
}

pub fn proj_condition(
    source: &str,
    gate: Gate,
    carry: Carry,
) -> Result<ProjCondition, NucleusError> {
    let condition =
        Condition::parse(source).map_err(|error| NucleusError::Parse(error.to_string()))?;
    Ok(ProjCondition {
        condition,
        gate,
        carry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn promises_and_rules_fold_forward() {
        let now = at("2026-07-05T00:00:00Z");
        let snapshot = Snapshot {
            now,
            quantities: HashMap::from([("r_APPLES".into(), 8.0)]),
            slugs: HashMap::from([("apples.stock".into(), "r_APPLES".into())]),
            promises: vec![ProjPromise {
                record_uid: "r_APPLES".into(),
                delta: 5.0,
                at: at("2026-07-08T12:00:00Z"),
            }],
            rules: vec![ProjRule {
                uid: "r_RULE".into(),
                slug: Some("rules.eat".into()),
                record_uid: "r_APPLES".into(),
                cadence: Cadence::every_days(1),
                anchor: at("2026-07-05T06:00:00Z"),
                condition: None,
                movement: Some((ProjMove::Add, -1.0)),
            }],
        };
        let timeline = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(timeline.projected("r_APPLES"), Some(3.0));
        let crossing = timeline.crossing_below("r_APPLES", 4.0).unwrap();
        assert!(crossing.at < at("2026-07-08T12:00:00Z"));
        assert!(crossing.cause.starts_with("rule:"));
        let again = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(again.points, timeline.points);
    }

    #[test]
    fn a_projected_rhythm_is_counted_the_same_way_the_wheel_counts_it() {
        let now = at("2026-03-01T00:00:00Z");
        let snapshot = Snapshot {
            now,
            quantities: HashMap::from([("r_HABIT".into(), 0.0), ("r_PAYDAY".into(), 0.0)]),
            slugs: HashMap::from([
                ("habit".into(), "r_HABIT".into()),
                ("payday".into(), "r_PAYDAY".into()),
            ]),
            promises: Vec::new(),
            rules: vec![
                ProjRule {
                    uid: "r_PAY".into(),
                    slug: Some("payday".into()),
                    record_uid: "r_PAYDAY".into(),
                    cadence: Cadence::every_days(7),
                    anchor: at("2026-03-02T00:00:00Z"),
                    condition: None,
                    movement: None,
                },
                ProjRule {
                    uid: "r_HAB".into(),
                    slug: Some("habit".into()),
                    record_uid: "r_HABIT".into(),
                    cadence: Cadence::every_days(1),
                    anchor: at("2026-03-01T00:00:00Z"),
                    condition: Some(
                        proj_condition("-1 * freq(@payday)", Gate::NonZero, Carry::Value)
                            .expect("a readable condition"),
                    ),
                    movement: Some((ProjMove::Add, 0.0)),
                },
            ],
        };
        let timeline = project(&snapshot, at("2026-03-22T00:00:00Z"));
        assert_eq!(timeline.projected("r_HABIT"), Some(-3.0));
    }

    #[test]
    fn a_reading_the_projection_will_not_invent_stops_that_rule_only() {
        let now = at("2026-07-05T00:00:00Z");
        let snapshot = Snapshot {
            now,
            quantities: HashMap::from([("r_A".into(), 10.0), ("r_B".into(), 10.0)]),
            slugs: HashMap::from([("a".into(), "r_A".into()), ("b".into(), "r_B".into())]),
            promises: Vec::new(),
            rules: vec![
                ProjRule {
                    uid: "r_UNSIMULABLE".into(),
                    slug: None,
                    record_uid: "r_A".into(),
                    cadence: Cadence::every_days(1),
                    anchor: now,
                    condition: Some(
                        proj_condition("sum(@a, 30d)", Gate::NonZero, Carry::Value)
                            .expect("readable"),
                    ),
                    movement: Some((ProjMove::Add, -1.0)),
                },
                ProjRule {
                    uid: "r_PLAIN".into(),
                    slug: None,
                    record_uid: "r_B".into(),
                    cadence: Cadence::every_days(1),
                    anchor: now,
                    condition: None,
                    movement: Some((ProjMove::Add, -1.0)),
                },
            ],
        };
        let timeline = project(&snapshot, now + TimeDelta::days(3));
        assert_eq!(
            timeline.projected("r_A"),
            Some(10.0),
            "a rule that cannot be simulated must not be guessed at"
        );
        assert_eq!(
            timeline.projected("r_B"),
            Some(7.0),
            "and must not take the other rules down with it"
        );
    }
}
