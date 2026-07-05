//! Imagination, engine side (blueprint XII): builds the Snapshot from the
//! store and hands it to the pure fold in `nucleus::imagination`. Also the
//! deterministic confidence formula over verified promise history.

use chrono::{DateTime, Utc};
use nucleus::imagination::{ProjFrequency, ProjPromise, Snapshot, Timeline};
use nucleus::PromiseState;
use store::sqlx::Row;
use store::Store;

use crate::error::EngineError;
use crate::karma::Registry;
use crate::Engine;

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
        let (Some(record_uid), Some(window_end)) = (p.record_uid, p.window_end) else { continue };
        let Ok(at) = DateTime::parse_from_rfc3339(&window_end) else { continue };
        promises.push(ProjPromise { record_uid, delta: p.delta, at: at.with_timezone(&Utc) });
    }

    let frequencies = store::freqs::all_enabled(&store.pool)
        .await?
        .into_iter()
        .map(|f| ProjFrequency { record_uid: f.record_uid, spec: f.spec })
        .collect();

    let rules = registry
        .rules
        .iter()
        .filter(|r| r.active)
        .map(|r| r.def.clone())
        .collect();

    Ok(Snapshot { now, quantities, slugs, promises, frequencies, rules })
}

/// Deterministic confidence for a promise (blueprint XII.2): the party's
/// kept ratio with Laplace smoothing; 0.5 for strangers with no history.
pub async fn confidence(store: &Store, promise_uid: &str) -> Result<f64, EngineError> {
    let Some(p) = store::misc::get_promise(&store.pool, promise_uid).await? else {
        return Ok(0.0);
    };
    let Some(party) = p.party_uid else { return Ok(0.5) };
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

    pub async fn confidence(&self, promise_uid: &str) -> Result<f64, EngineError> {
        confidence(&self.store, promise_uid).await
    }
}
