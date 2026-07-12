//! Imagination (blueprint XII): fold promises and rules forward.
//! `state(t) = facts ≤ now + promises kept by t`, with pure rules simulated on
//! a virtual clock. No IO: the engine builds the Snapshot; this walks it.
//!
//! v1 scope: promises apply at `window_end`; frequencies fire on schedule and
//! feed rules via `freq()`; rules simulate `quantity`/`freq`/`value` tokens
//! (rules needing signals/sums are skipped — signals are frozen world state);
//! only quantity-shaped consequences (set/add/activate/deactivate) simulate.

use crate::NucleusError;
use crate::expr::{Resolver, Value};
use crate::frequency::FrequencySpec;
use crate::rule::{ConsequenceKind, RuleDef};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now: DateTime<Utc>,
    /// record uid -> current level (the quantity cache, copied).
    pub quantities: HashMap<String, f64>,
    /// slug -> record uid, for token resolution inside rule conditions.
    pub slugs: HashMap<String, String>,
    pub promises: Vec<ProjPromise>,
    pub frequencies: Vec<ProjFrequency>,
    pub rules: Vec<RuleDef>,
}

#[derive(Debug, Clone)]
pub struct ProjPromise {
    pub record_uid: String,
    pub delta: f64,
    /// expected keep time (window_end in v1).
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ProjFrequency {
    pub record_uid: String,
    pub spec: FrequencySpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelinePoint {
    pub at: DateTime<Utc>,
    pub record_uid: String,
    pub quantity: f64,
    pub cause: String, // "promise" | "rule:<uid>"
}

#[derive(Debug, Clone)]
pub struct Timeline {
    pub points: Vec<TimelinePoint>,
    pub final_state: HashMap<String, f64>,
}

impl Timeline {
    /// First projected moment `record` crosses at-or-below `threshold`.
    pub fn crossing_below(&self, record_uid: &str, threshold: f64) -> Option<&TimelinePoint> {
        self.points
            .iter()
            .find(|p| p.record_uid == record_uid && p.quantity <= threshold)
    }

    pub fn projected(&self, record_uid: &str) -> Option<f64> {
        self.final_state.get(record_uid).copied()
    }
}

#[derive(Clone, Copy)]
struct VirtualResolver<'a> {
    quantities: &'a HashMap<String, f64>,
    slugs: &'a HashMap<String, String>,
    rules_by_token: &'a HashMap<String, RuleDef>, // slug/uid -> derived rule
    freq_fired: &'a HashMap<String, f64>,         // freq record uid -> periods
    depth: usize,
}

impl VirtualResolver<'_> {
    fn uid_of(&self, token: &str) -> Option<String> {
        self.slugs.get(token).cloned().or_else(|| {
            self.quantities
                .contains_key(token)
                .then(|| token.to_string())
        })
    }
}

impl Resolver for VirtualResolver<'_> {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, NucleusError> {
        let slug = args
            .iter()
            .find_map(|a| match a {
                Value::Ref(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| NucleusError::Eval(format!("{name} needs a @ref")))?;
        match name {
            "quantity" | "signal" => {
                let uid = self.uid_of(&slug).ok_or(NucleusError::UnknownToken(slug))?;
                Ok(Value::Num(
                    self.quantities.get(&uid).copied().unwrap_or(0.0),
                ))
            }
            "freq" => {
                let uid = self.uid_of(&slug).ok_or(NucleusError::UnknownToken(slug))?;
                Ok(Value::Num(
                    self.freq_fired.get(&uid).copied().unwrap_or(0.0),
                ))
            }
            "value" => {
                if self.depth > 4 {
                    return Err(NucleusError::Eval("value() too deep in projection".into()));
                }
                let rule = self
                    .rules_by_token
                    .get(&slug)
                    .ok_or(NucleusError::UnknownToken(slug))?;
                let mut inner = VirtualResolver {
                    depth: self.depth + 1,
                    ..*self
                };
                Ok(Value::Num(rule.condition.eval(&mut inner)?))
            }
            // signals stay frozen at snapshot values via `signal` above; sums,
            // confidence, place functions are not simulated in v1
            _ => Err(NucleusError::UnknownToken(format!(
                "{name}() in projection"
            ))),
        }
    }
}

/// The fold. Deterministic: same snapshot, same timeline.
pub fn project(snapshot: &Snapshot, until: DateTime<Utc>) -> Timeline {
    let mut state = snapshot.quantities.clone();
    let mut points = Vec::new();

    // one merged, time-ordered event stream: promise keeps + frequency fires
    #[derive(Clone)]
    enum Ev {
        Promise(usize),
        Freq(usize, f64), // index, periods (1 per fire)
    }
    let mut events: Vec<(DateTime<Utc>, Ev)> = Vec::new();
    for (i, p) in snapshot.promises.iter().enumerate() {
        if p.at > snapshot.now && p.at <= until {
            events.push((p.at, Ev::Promise(i)));
        }
    }
    for (i, f) in snapshot.frequencies.iter().enumerate() {
        for boundary in f.spec.boundaries(snapshot.now, until) {
            events.push((boundary, Ev::Freq(i, 1.0)));
        }
    }
    events.sort_by_key(|(at, _)| *at);

    let rules_by_token: HashMap<String, RuleDef> = snapshot
        .rules
        .iter()
        .flat_map(|r| {
            let mut keys = vec![(r.uid.clone(), r.clone())];
            if let Some(slug) = &r.slug {
                keys.push((slug.clone(), r.clone()));
            }
            keys
        })
        .collect();

    for (at, ev) in events {
        let fired: HashMap<String, f64> = match &ev {
            Ev::Freq(i, periods) => {
                HashMap::from([(snapshot.frequencies[*i].record_uid.clone(), *periods)])
            }
            Ev::Promise(i) => {
                let p = &snapshot.promises[*i];
                let q = state.entry(p.record_uid.clone()).or_insert(0.0);
                *q += p.delta;
                points.push(TimelinePoint {
                    at,
                    record_uid: p.record_uid.clone(),
                    quantity: *q,
                    cause: "promise".into(),
                });
                HashMap::new()
            }
        };
        // evaluate every rule against the virtual state (small N; correctness
        // over cleverness — the reactive graph is an engine concern)
        for rule in &snapshot.rules {
            if rule.is_derived_value() {
                continue;
            }
            let mut resolver = VirtualResolver {
                quantities: &state,
                slugs: &snapshot.slugs,
                rules_by_token: &rules_by_token,
                freq_fired: &fired,
                depth: 0,
            };
            let Ok(Some(firing)) = rule.evaluate(&mut resolver) else {
                continue;
            };
            for c in &rule.consequences {
                let Some(target) = c.target.as_deref() else {
                    continue;
                };
                let token = target.trim_start_matches('@');
                let Some(uid) = snapshot
                    .slugs
                    .get(token)
                    .cloned()
                    .or_else(|| state.contains_key(token).then(|| token.to_string()))
                else {
                    continue;
                };
                let current = state.get(&uid).copied().unwrap_or(0.0);
                let next = match c.kind {
                    ConsequenceKind::SetQuantity => firing.carried,
                    ConsequenceKind::AddQuantity => current + firing.carried,
                    ConsequenceKind::Activate => 1.0,
                    ConsequenceKind::Deactivate => 0.0,
                    _ => continue, // effects/promises/asks don't simulate in v1
                };
                if next != current {
                    state.insert(uid.clone(), next);
                    points.push(TimelinePoint {
                        at,
                        record_uid: uid,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::ConsequenceSpec;
    use chrono::TimeDelta;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn promises_and_rules_fold_forward() {
        let now = at("2026-07-05T00:00:00Z");
        // apples 8; a daily rule eats one; Maria delivers 5 on the 8th
        let snapshot = Snapshot {
            now,
            quantities: HashMap::from([("r_APPLES".into(), 8.0)]),
            slugs: HashMap::from([
                ("apples.stock".into(), "r_APPLES".into()),
                ("freq.daily".into(), "r_FREQ".into()),
            ]),
            promises: vec![ProjPromise {
                record_uid: "r_APPLES".into(),
                delta: 5.0,
                at: at("2026-07-08T12:00:00Z"),
            }],
            frequencies: vec![ProjFrequency {
                record_uid: "r_FREQ".into(),
                spec: FrequencySpec {
                    seconds: 0,
                    days: 1,
                    months: 0,
                    day_of_week: None,
                    next_at: at("2026-07-05T06:00:00Z"),
                    finish_at: None,
                    catch_up: false,
                },
            }],
            rules: vec![
                RuleDef::parse(
                    "r_RULE",
                    Some("rules.eat".into()),
                    "-1 * freq(@freq.daily)",
                    "!=0",
                    "value",
                    None,
                    vec![ConsequenceSpec {
                        kind: ConsequenceKind::AddQuantity,
                        target: Some("@apples.stock".into()),
                        params: None,
                        position: 0,
                    }],
                )
                .unwrap(),
            ],
        };
        // ten days out: 8 - 10 (rule) + 5 (promise) = 3
        let timeline = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(timeline.projected("r_APPLES"), Some(3.0));
        // the low point (4) is reached by the rule BEFORE the delivery lifts it —
        // exactly the threshold-crossing Attention watches for
        let crossing = timeline.crossing_below("r_APPLES", 4.0).unwrap();
        assert!(crossing.at < at("2026-07-08T12:00:00Z"));
        assert!(crossing.cause.starts_with("rule:"));
        // determinism: same snapshot, same timeline
        let again = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(again.points, timeline.points);
    }
}
