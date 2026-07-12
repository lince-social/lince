//! Imagination, engine side (blueprint XII): builds the Snapshot from the
//! store and hands it to the pure fold in `nucleus::imagination`. Also the
//! deterministic confidence formula over verified promise history.

use chrono::{DateTime, Utc};
use nucleus::PromiseState;
use nucleus::imagination::{ProjFrequency, ProjPromise, Snapshot, Timeline};
use store::Store;
use store::sqlx::Row;

use crate::Engine;
use crate::error::EngineError;
use crate::karma::Registry;

/// Build the projection snapshot: quantity levels, slug map, agreed/active
/// promises with windows, enabled frequencies, active rules.
pub async fn build_snapshot(
    store: &Store,
    registry: &Registry,
    now: DateTime<Utc>,
) -> Result<Snapshot, EngineError> {
    let mut quantities = std::collections::HashMap::new();
    let mut slugs = std::collections::HashMap::new();
    for r in store::records::list_all(&store.pool).await? {
        quantities.insert(r.uid.clone(), r.quantity);
        if let Some(slug) = r.slug {
            slugs.insert(slug, r.uid);
        }
    }

    let mut promises = Vec::new();
    for p in store::misc::list_promises(&store.pool).await? {
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

    let frequencies = store::freqs::all_enabled(&store.pool)
        .await?
        .into_iter()
        .map(|f| ProjFrequency {
            record_uid: f.record_uid,
            spec: f.spec,
        })
        .collect();

    let rules = registry
        .rules
        .iter()
        .filter(|r| r.active)
        .map(|r| r.def.clone())
        .collect();

    Ok(Snapshot {
        now,
        quantities,
        slugs,
        promises,
        frequencies,
        rules,
    })
}

/// Deterministic confidence for a promise (blueprint XII.2): the party's
/// kept ratio with Laplace smoothing; 0.5 for strangers with no history.
pub async fn confidence(store: &Store, promise_uid: &str) -> Result<f64, EngineError> {
    let Some(p) = store::misc::get_promise(&store.pool, promise_uid).await? else {
        return Ok(0.0);
    };
    let Some(party) = p.party_uid else {
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

/// Demand curve sample (blueprint XII.2): the share (0..1) of the trailing
/// 30 days' facts on the concept's records that happened in `now`'s
/// hour-of-day. Pure over the Ledger — same inputs, same number.
pub async fn demand(
    store: &Store,
    concept_token: &str,
    now: DateTime<Utc>,
) -> Result<f64, EngineError> {
    let Some(concept_uid) = store::concepts::resolve(&store.pool, concept_token).await? else {
        return Err(EngineError::UnknownRecord(concept_token.to_string()));
    };
    let family: std::collections::HashSet<String> =
        store::concepts::descendants_including(&store.pool, &concept_uid)
            .await?
            .into_iter()
            .collect();
    let mut in_family: std::collections::HashSet<String> = Default::default();
    for r in store::records::list_all(&store.pool).await? {
        if r.concept_uid.as_ref().is_some_and(|c| family.contains(c)) {
            in_family.insert(r.uid);
        }
    }

    use chrono::Timelike;
    let since = (now - chrono::TimeDelta::days(30)).to_rfc3339();
    let mut total = 0u64;
    let mut this_hour = 0u64;
    for f in store::facts::list_since(&store.pool, Some(&since), 100_000).await? {
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

/// The crossing sweep's default horizon: a week ahead.
pub const CROSSING_HORIZON_SECS: i64 = 7 * 86_400;

impl Engine {
    /// Project the Cell's state forward to `until` — the scrubbable future.
    pub async fn project(
        &self,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Timeline, EngineError> {
        let registry = self.registry_snapshot().await;
        let snapshot = build_snapshot(&self.store, &registry, now).await?;
        Ok(nucleus::imagination::project(&snapshot, until))
    }

    /// Build the projection Snapshot for branching (blueprint XII.1): mutate
    /// the returned snapshot (toggle a rule, drag a promise) and fold it with
    /// `nucleus::imagination::project` — diffing two timelines is the compare
    /// view.
    pub async fn snapshot(&self, now: DateTime<Utc>) -> Result<Snapshot, EngineError> {
        let registry = self.registry_snapshot().await;
        build_snapshot(&self.store, &registry, now).await
    }

    /// The Imagination heartbeat arm (blueprint XII.1 → XIII, decision 4):
    /// every plain record currently at-or-above zero whose projection crosses
    /// below zero within the horizon enqueues a `crossing` decision — "apples
    /// hit 0 on Thursday". Deduped per record; returns created decision uids.
    pub async fn crossings_pass(&self, now: DateTime<Utc>) -> Result<Vec<String>, EngineError> {
        let timeline = self
            .project(now, now + chrono::TimeDelta::seconds(CROSSING_HORIZON_SECS))
            .await?;
        let asked = store::misc::open_decision_subjects(&self.store.pool).await?;
        let mut created = Vec::new();
        for record in store::records::list_all(&self.store.pool).await? {
            if record.kind != "plain" || record.quantity < 0.0 {
                continue;
            }
            let Some(crossing) = timeline.crossing_below(&record.uid, 0.0) else {
                continue;
            };
            if asked.contains(&(record.uid.clone(), "crossing".to_string())) {
                continue;
            }
            let name = record.slug.clone().unwrap_or_else(|| record.uid.clone());
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
