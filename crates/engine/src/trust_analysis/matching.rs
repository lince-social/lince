use super::*;
use crate::actions::ActionOutcome;
use chrono::{DateTime, Utc};
use nucleus::transfer::matching::Request;
use serde_json::{Value, json};

impl Engine {
    pub(super) async fn match_suppliers(
        &self,
        person: &str,
        request: &Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let minimum = request.validate().map_err(invalid)?;
        self.require_permission(actor, "transfer:read").await?;
        self.refuse_unreadable(Some(person), &[request.need.need.clone()])
            .await?;
        let revision = *self.watch_query_changes().borrow();
        let mut tx = self.store.pool.begin().await?;
        let need = store::transfer_evidence::freeze_need_on(&mut tx, person, &request.need).await?;
        drop(tx);
        let (offers, mut partial) = if request.offers.is_empty() {
            let query = protein::Protein {
                source: protein::Source::Transfer,
                filter: Vec::new(),
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: Some(33),
            };
            let rows = protein::execute_for(&self.store, &query, Some(person)).await?;
            let offers = rows
                .iter()
                .filter_map(|row| row["uid"].as_str())
                .filter(|uid| nucleus::valid_uid(uid, "r"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let partial = offers.len() > 32;
            (offers.into_iter().take(32).collect::<Vec<_>>(), partial)
        } else {
            (request.offers.clone(), false)
        };
        let mut bytes = 0usize;
        let mut candidates = Vec::<Value>::new();
        let anchor = if request.within_metres.is_some() {
            self.saved_location_of(&request.need.need, Some(person))
                .await?
                .selected
        } else {
            None
        };
        for offer in &offers {
            if store::location::transfer_ended(&self.store.pool, offer).await? {
                continue;
            }
            let query = protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(offer.clone())],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: Some(1),
            };
            let rows = protein::execute_for(&self.store, &query, Some(person)).await?;
            let Some(row) = rows
                .iter()
                .find(|row| row["uid"].as_str() == Some(offer.as_str()))
            else {
                continue;
            };
            for promise in row["promises"].as_array().into_iter().flatten() {
                if !matches!(promise["state"].as_str(), Some("proposed" | "open"))
                    || promise["delta"].as_f64().is_none_or(|value| value >= 0.0)
                {
                    continue;
                }
                let Some(record) = promise["record"]
                    .as_str()
                    .filter(|record| !record.is_empty())
                else {
                    continue;
                };
                if !self.may_read_record(Some(person), record).await? {
                    continue;
                }
                let unit = need
                    .measure
                    .unit
                    .as_ref()
                    .map(|unit| unit.concept_uid.as_str());
                if promise["unit"].as_str() != unit {
                    continue;
                }
                let options =
                    store::transfer_evidence::source_measures(&self.store.pool, record, None)
                        .await?;
                if !options
                    .iter()
                    .any(|option| option["measure"] == json!(need.measure))
                {
                    continue;
                }
                let available = store::transfer_balances::read(&self.store.pool, record)
                    .await?
                    .can_offer;
                let offered = promise["delta"].as_f64().map(f64::abs).unwrap_or(0.0);
                if offered < need.requested.to_f64()
                    || available.exact_numeric_cmp(need.requested).is_lt()
                {
                    continue;
                }
                if promise["window_end"].as_str().is_some_and(|end| {
                    chrono::DateTime::parse_from_rfc3339(end)
                        .map_or(true, |end| end.timestamp() <= now.timestamp())
                }) {
                    continue;
                }
                if !overlaps(
                    request.from.as_deref(),
                    request.until.as_deref(),
                    promise["window_start"].as_str(),
                    promise["window_end"].as_str(),
                ) {
                    continue;
                }
                if let Some(range) = request.within_metres {
                    let Some(anchor) = anchor.as_ref() else {
                        continue;
                    };
                    let Some(place) = self.saved_location_of(record, Some(person)).await?.selected
                    else {
                        continue;
                    };
                    let distance = distance(
                        anchor.latitude,
                        anchor.longitude,
                        place.latitude,
                        place.longitude,
                    );
                    if distance
                        + f64::from(anchor.radius_metres.unwrap_or(0))
                        + f64::from(place.radius_metres.unwrap_or(0))
                        > f64::from(range)
                    {
                        continue;
                    }
                }
                let giver = promise["giver"]
                    .as_str()
                    .or_else(|| promise["party"].as_str());
                let Some(giver) =
                    giver.filter(|giver| nucleus::valid_uid(giver, "r") && *giver != person)
                else {
                    continue;
                };
                let mut sources = request.sources.clone();
                sources.subject = Identity::Person(giver.into());
                sources.role = nucleus::transfer::outcome::Role::Giver;
                sources.issuers.include.retain(|rule| {
                    !rule.identities.is_empty()
                        || rule.known.is_some()
                        || rule.lower.is_some()
                        || rule.upper.is_some()
                });
                sources.measurements = vec![nucleus::transfer::receipt::Measurement {
                    meaning: need.measure.meaning.clone(),
                    unit: need.measure.unit.clone(),
                    object_commitment: None,
                }];
                if need.measure.object.is_some() {
                    continue;
                }
                let analysis = self
                    .evaluate_trust_query(person, actor, &sources, false, now)
                    .await?;
                let experience = analysis
                    .analysis
                    .groups
                    .iter()
                    .filter_map(|group| group.raw_quantity)
                    .max_by(|left, right| left.exact_numeric_cmp(*right))
                    .unwrap_or_else(store::exact::zero);
                let index = analysis
                    .analysis
                    .groups
                    .iter()
                    .filter_map(|group| group.weighted_index)
                    .max_by(|left, right| left.exact_numeric_cmp(*right))
                    .unwrap_or_else(store::exact::zero);
                if index.exact_numeric_cmp(minimum).is_lt() {
                    continue;
                }
                let candidate = json!({"transfer":offer,"revision":row["revision"],"promise":promise["uid"],"giver":giver,"available":available,"experience":experience,"weighted_index":index,"experience_by_evidence":analysis.analysis.groups,"offer":row,"draft_only":true});
                let size = serde_json::to_vec(&candidate)?.len();
                let maximum = nucleus::trust_analysis::MAX_BYTES
                    .min(nucleus::social::MAX_FRAME_BYTES.saturating_sub(16 * 1024));
                if bytes + size > maximum {
                    partial = true;
                    break;
                }
                bytes += size;
                candidates.push(candidate);
                break;
            }
        }
        if *self.watch_query_changes().borrow() != revision {
            return Err(invalid(
                "Offers or trust evidence changed during matching; search again",
            ));
        }
        Ok(ActionOutcome {
            data: Some(
                json!({"trust":{"kind":"supplier_candidates","need":request.need,"candidates":candidates,"partial":partial,"execution":"Select a candidate and review current party and terms; matching executes nothing"}}),
            ),
            ..Default::default()
        })
    }
}
fn overlaps(
    from: Option<&str>,
    until: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> bool {
    let parse = |value: Option<&str>| value.map(chrono::DateTime::parse_from_rfc3339).transpose();
    let (Ok(from), Ok(until), Ok(start), Ok(end)) =
        (parse(from), parse(until), parse(start), parse(end))
    else {
        return false;
    };
    if from.is_some() && end.is_none() || until.is_some() && start.is_none() {
        return false;
    }
    !from.zip(end).is_some_and(|(from, end)| from >= end)
        && !until
            .zip(start)
            .is_some_and(|(until, start)| until <= start)
}
fn distance(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let lat = (c - a).to_radians() / 2.0;
    let lon = (d - b).to_radians() / 2.0;
    let h = lat.sin().powi(2) + a.to_radians().cos() * c.to_radians().cos() * lon.sin().powi(2);
    12_742_000.0 * h.clamp(0.0, 1.0).sqrt().asin()
}
