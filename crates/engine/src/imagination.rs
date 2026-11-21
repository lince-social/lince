// non-lean-yet
// Scope: fn build_snapshot, fn confidence, fn demand, const CROSSING_HORIZON_SECS.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 17.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 123, 171.
// - [Clippy probe: clippy::map_unwrap_or] called `map(<f>).unwrap_or(<a>)` on an `Option` value.
//   Sites 61, 65.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 11, 105, 126, 178, 187, 191, 255.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   11, 105, 126, 175, 178, 187, 191, 255.

use chrono::{DateTime, Utc};
use nucleus::PromiseState;
use nucleus::imagination::{ProjCondition, ProjMove, ProjPromise, ProjRule, Snapshot, Timeline};
use std::collections::HashMap;
use store::Store;
use store::sqlx::Row;

use crate::Engine;
use crate::error::EngineError;

pub async fn build_snapshot(store: &Store, now: DateTime<Utc>) -> Result<Snapshot, EngineError> {
    let _read = nucleus::performance::Span::new("imagination-snapshot-await");
    let mut quantities = std::collections::HashMap::new();
    let mut slugs = std::collections::HashMap::new();
    for r in store::records::named_levels(&store.pool).await? {
        if let Some(slug) = r.slug {
            slugs.insert(slug, r.uid.clone());
        }
        quantities.insert(r.uid, r.quantity.to_f64());
    }

    let mut promises = Vec::new();
    for p in store::misc::promise_projections(&store.pool).await? {
        if !matches!(p.state, PromiseState::Agreed | PromiseState::Active) {
            continue;
        }
        let (Some(record_uid), Some(window_end)) = (p.record_uid, p.window_end) else {
            continue;
        };
        let Ok(at) = DateTime::parse_from_rfc3339(&window_end) else {
            continue;
        };
        promises.push(ProjPromise {
            record_uid,
            delta: p.delta,
            at: at.with_timezone(&Utc),
        });
    }

    let mut slug_of: HashMap<&str, &str> = Default::default();
    for (slug, uid) in &slugs {
        slug_of.insert(uid.as_str(), slug.as_str());
    }
    let mut rules = Vec::new();
    for rule in store::recurrence::all(&store.pool).await? {
        if rule.is_paused() {
            continue;
        }
        let Ok(anchor) = crate::actions::parse_instant_field(&rule.anchor_at) else {
            continue;
        };
        let movement = rule
            .consequences
            .iter()
            .find_map(|consequence| match consequence {
                nucleus::karma::Consequence::CaptureEntry { amount, .. } => {
                    Some((ProjMove::Add, amount.to_f64()))
                }
                nucleus::karma::Consequence::AddQuantity { delta } => Some((
                    ProjMove::Add,
                    delta.map(|value| value.to_f64()).unwrap_or(0.0),
                )),
                nucleus::karma::Consequence::SetQuantity { value } => Some((
                    ProjMove::Set,
                    value.map(|value| value.to_f64()).unwrap_or(0.0),
                )),
                _ => None,
            });
        let condition = match rule.condition {
            None => None,
            Some(stored) => {
                let Ok(condition) = nucleus::karma::Condition::parse(&stored.source) else {
                    continue;
                };
                Some(ProjCondition {
                    condition,
                    gate: stored.gate,
                    carry: stored.carry,
                })
            }
        };
        rules.push(ProjRule {
            uid: rule.uid,
            slug: slug_of
                .get(rule.record_uid.as_str())
                .map(|slug| (*slug).to_owned()),
            record_uid: rule.record_uid,
            cadence: rule.cadence,
            anchor,
            condition,
            movement,
        });
    }
    drop(slug_of);

    Ok(Snapshot {
        now,
        quantities,
        slugs,
        promises,
        rules,
    })
}

pub async fn confidence(store: &Store, promise_uid: &str) -> Result<f64, EngineError> {
    let Some(party) = store::misc::promise_party(&store.pool, promise_uid).await? else {
        return Ok(0.0);
    };
    let Some(party) = party else {
        return Ok(0.5);
    };
    let row = store::sqlx::query(
        "SELECT
            COALESCE(SUM(state = 'kept'), 0) AS kept,
            COALESCE(SUM(state = 'broken'), 0) AS broken
         FROM promise WHERE party_uid = ?",
    )
    .bind(&party)
    .fetch_one(&store.pool)
    .await?;
    let kept: i64 = row.get("kept");
    let broken: i64 = row.get("broken");
    Ok((kept as f64 + 1.0) / ((kept + broken) as f64 + 2.0))
}

pub async fn demand(
    store: &Store,
    concept_token: &str,
    now: DateTime<Utc>,
) -> Result<f64, EngineError> {
    let _read = nucleus::performance::Span::new("imagination-demand-await");
    let Some(concept_uid) = store::concepts::resolve(&store.pool, concept_token).await? else {
        return Err(EngineError::UnknownRecord(concept_token.to_string()));
    };
    let family: std::collections::HashSet<String> =
        store::concepts::descendants_including(&store.pool, &concept_uid)
            .await?
            .into_iter()
            .collect();
    let mut in_family: std::collections::HashSet<String> = Default::default();
    for r in store::records::grouping(
        &store.pool,
        store::records::GroupingFields {
            unit: false,
            identity: true,
        },
    )
    .await?
    {
        if r.identity.as_ref().is_some_and(|c| family.contains(c)) {
            in_family.insert(r.uid);
        }
    }

    use chrono::Timelike;
    let since = (now - chrono::TimeDelta::days(30)).to_rfc3339();
    let mut total = 0u64;
    let mut this_hour = 0u64;
    for f in store::facts::ledger_since(&store.pool, Some(&since), 100_000, false).await? {
        if !in_family.contains(&f.record_uid) {
            continue;
        }
        total += 1;
        if f.at.hour() == now.hour() {
            this_hour += 1;
        }
    }
    Ok(if total == 0 {
        0.0
    } else {
        this_hour as f64 / total as f64
    })
}

pub const CROSSING_HORIZON_SECS: i64 = 7 * 86_400;

impl Engine {
    pub async fn project(
        &self,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Timeline, EngineError> {
        let snapshot = build_snapshot(&self.store, now).await?;
        Ok(nucleus::imagination::project(&snapshot, until))
    }

    pub async fn snapshot(&self, now: DateTime<Utc>) -> Result<Snapshot, EngineError> {
        build_snapshot(&self.store, now).await
    }

    pub async fn crossings_pass(&self, now: DateTime<Utc>) -> Result<Vec<String>, EngineError> {
        let _pass = nucleus::performance::Span::new("imagination-crossings-await");
        let timeline = self
            .project(now, now + chrono::TimeDelta::seconds(CROSSING_HORIZON_SECS))
            .await?;
        let asked = store::misc::open_decision_subjects(&self.store.pool)
            .await?
            .into_iter()
            .filter_map(|(uid, kind)| (kind == "crossing").then_some(uid))
            .collect::<std::collections::HashSet<_>>();
        let mut points = timeline.points.iter();
        let mut crossings = HashMap::new();
        let mut created = Vec::new();
        for record in store::records::named_levels(&self.store.pool).await? {
            if record.kind != "plain" || record.quantity.is_negative() {
                continue;
            }
            let crossing = crossings.get(record.uid.as_str()).copied().or_else(|| {
                if points.as_slice().is_empty() {
                    return None;
                }
                let _index = nucleus::performance::Span::new("imagination-crossing-index");
                let mut examined = 0;
                let crossing = points.by_ref().find_map(|point| {
                    examined += 1;
                    if point.quantity <= 0.0 {
                        let first = *crossings.entry(point.record_uid.as_str()).or_insert(point);
                        (point.record_uid == record.uid).then_some(first)
                    } else {
                        None
                    }
                });
                nucleus::performance::add("imagination-crossing-points", examined);
                crossing
            });
            let Some(crossing) = crossing else {
                continue;
            };
            if asked.contains(record.uid.as_str()) {
                continue;
            }
            let name = record.slug.as_deref().unwrap_or(&record.uid);
            created.push(
                store::misc::create_decision(
                    &self.store.pool,
                    &record.uid,
                    "crossing",
                    &format!(
                        "{name} hits {:.0} on {} ({})",
                        crossing.quantity,
                        crossing.at.format("%A %Y-%m-%d"),
                        crossing.cause,
                    ),
                    &serde_json::json!([
                        { "label": "acknowledge" },
                        { "label": "act" },
                    ]),
                )
                .await?,
            );
        }
        Ok(created)
    }

    pub async fn confidence(&self, promise_uid: &str) -> Result<f64, EngineError> {
        confidence(&self.store, promise_uid).await
    }
}
