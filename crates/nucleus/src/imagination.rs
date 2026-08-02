//! Imagination (blueprint XII): fold promises and rules forward.
//!
//! `state(t) = facts ≤ now + promises kept by t`, with rules simulated on a
//! virtual clock. No IO: the engine builds the [`Snapshot`]; this walks it.
//!
//! # One rule shape, here too
//!
//! There used to be three objects in this file: a rule, a frequency it read
//! through `freq()`, and a separate notion of a derived value. There is one now.
//! A rule carries its own cadence, so projecting it forward is just deriving the
//! instants that cadence produces between now and the horizon — the same
//! arithmetic the heartbeat runs, on a clock that has not happened yet.
//!
//! That identity is the point. A projection that used a second implementation
//! of "when does this repeat" would drift from the wheel, and the drift would
//! show up as a threshold crossing that never arrives or an alarm for one that
//! does not exist.
//!
//! # What does not simulate
//!
//! Signals stay frozen at their snapshot values: they are world state, and
//! guessing at them would be inventing readings. Windowed sums are not replayed
//! either, because a projected window would have to sum facts that have not been
//! written. A rule whose condition needs one of those simply does not fire in
//! the projection, which understates the future rather than fabricating it.
//!
//! Outward consequences — notify, ask, run, promise — never simulate. They do
//! not move a number, so a timeline has nothing to draw for them.

use crate::NucleusError;
use crate::karma::{Cadence, Carry, Condition, ConditionError, DecimalValue, ExactResolver, Gate};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// How deep one projected rule may read another's arithmetic.
const VALUE_DEPTH_CAP: usize = 4;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now: DateTime<Utc>,
    /// record uid -> current level (the quantity cache, copied).
    pub quantities: HashMap<String, f64>,
    /// slug -> record uid, for token resolution inside rule conditions.
    pub slugs: HashMap<String, String>,
    pub promises: Vec<ProjPromise>,
    pub rules: Vec<ProjRule>,
}

#[derive(Debug, Clone)]
pub struct ProjPromise {
    pub record_uid: String,
    pub delta: f64,
    /// expected keep time (window_end in v1).
    pub at: DateTime<Utc>,
}

/// How a rule moves the number it is about, per occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjMove {
    /// A capture or an add: the number is a movement, and folds.
    Add,
    /// An assignment: the number is a level, and replaces.
    Set,
}

/// One rule, in the only form a projection needs.
///
/// Deliberately not the stored rule type. A projection needs to know when a
/// rule lands and what it does to a number, and nothing else — so the stored
/// type stays free to grow consequences that mean nothing here without this
/// file having to learn about them.
#[derive(Debug, Clone)]
pub struct ProjRule {
    pub uid: String,
    pub slug: Option<String>,
    /// The Record the rule acts on.
    pub record_uid: String,
    pub cadence: Cadence,
    pub anchor: DateTime<Utc>,
    /// The *if*, already parsed, or `None` for a rule its dates alone justify.
    pub condition: Option<ProjCondition>,
    /// What it does to the number, or `None` when it only touches concepts.
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

/// Readings taken against the virtual world, at one virtual instant.
struct VirtualResolver<'a> {
    snapshot: &'a Snapshot,
    quantities: &'a HashMap<String, f64>,
    /// The rule being evaluated, and the stretch its evaluation speaks for.
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
            // The same window rule as the live engine: the boundaries of the
            // referenced rule that land in the stretch this evaluation covers.
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
            // Frozen world state and windowed sums are not invented here.
            other => Err(ConditionError::UnknownReference(format!(
                "{other}() in projection"
            ))),
        }
    }
}

/// The fold. Deterministic: same snapshot, same timeline.
pub fn project(snapshot: &Snapshot, until: DateTime<Utc>) -> Timeline {
    let mut state = snapshot.quantities.clone();
    let mut points = Vec::new();

    /// One thing that happens at a virtual instant.
    enum Ev {
        Promise(usize),
        /// A rule's occurrence, and the instant its previous one fell on.
        Rule(usize, DateTime<Utc>),
    }

    let mut events: Vec<(DateTime<Utc>, Ev)> = Vec::new();
    for (index, promise) in snapshot.promises.iter().enumerate() {
        if promise.at > snapshot.now && promise.at <= until {
            events.push((promise.at, Ev::Promise(index)));
        }
    }
    for (index, rule) in snapshot.rules.iter().enumerate() {
        // A cadence that does not validate produces no instants, which is what
        // it contributes to a real wheel too. Failing the whole projection
        // because one stored rule is malformed would hide every other rule's
        // future in order to report one broken one.
        let Ok(derived) = rule.cadence.between(rule.anchor, snapshot.now, until) else {
            continue;
        };
        // Each occurrence reads back to the one before it, so the windows a
        // rule reads over tile the projected timeline exactly as they tile the
        // real one.
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
    // Stable across equal instants: a promise kept at the same moment a rule
    // lands is applied first, so the rule sees the world the promise made.
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
                    continue; // touches no number: nothing for a timeline to draw
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
                        // The gate blocked, or the condition needed a reading
                        // this projection will not invent. Either way the rule
                        // does not act at this instant.
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

/// Parse a stored condition into the form a projection evaluates.
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
        // apples 8; a daily rule eats one; Maria delivers 5 on the 8th.
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
        // Ten days out: 8 - 10 (rule) + 5 (promise) = 3.
        let timeline = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(timeline.projected("r_APPLES"), Some(3.0));
        // The low point is reached by the rule BEFORE the delivery lifts it —
        // exactly the threshold crossing Attention watches for.
        let crossing = timeline.crossing_below("r_APPLES", 4.0).unwrap();
        assert!(crossing.at < at("2026-07-08T12:00:00Z"));
        assert!(crossing.cause.starts_with("rule:"));
        // Determinism: same snapshot, same timeline.
        let again = project(&snapshot, now + TimeDelta::days(10));
        assert_eq!(again.points, timeline.points);
    }

    #[test]
    fn a_projected_rhythm_is_counted_the_same_way_the_wheel_counts_it() {
        // The whole reason a projection may share the schedule type: a rule
        // gated on `freq(@payday)` has to land on the projected timeline
        // exactly where the heartbeat would land it.
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
        // Three weeks: three Mondays, three acts, and nothing on the other days.
        let timeline = project(&snapshot, at("2026-03-22T00:00:00Z"));
        assert_eq!(timeline.projected("r_HABIT"), Some(-3.0));
    }

    #[test]
    fn a_reading_the_projection_will_not_invent_stops_that_rule_only() {
        // A rule needing a windowed sum cannot be simulated, and the honest
        // response is to leave it out — not to guess, and not to abandon the
        // other rules' futures.
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
