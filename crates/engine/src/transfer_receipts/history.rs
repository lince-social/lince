use super::*;
use crate::actions::ActionOutcome;
use nucleus::transfer::{receipt_fragment::State, receipt_history};
use serde_json::json;

impl Engine {
    pub(super) async fn receipt_history(
        &self,
        origin: &str,
        occurrence: &CanonicalHash,
        before: Option<&str>,
        person: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if !nucleus::valid_uid(origin, "r") {
            return Err(invalid("Choose a source Organ identity"));
        }
        let purpose = format!(
            "transfer-receipt-history:{}",
            receipt_history::source_key(origin, occurrence)
                .map_err(invalid)?
                .as_str()
        );
        let principal = super::browsing::principal(person, actor)?;
        let position = match before {
            Some(token) => Some(
                self.decode_read_cursor::<(String, String)>(
                    &purpose,
                    &principal,
                    token,
                    now.timestamp(),
                )
                .await?,
            ),
            None => None,
        };
        let rows = store::receipt_events::candidates(
            &self.store.pool,
            origin,
            occurrence.as_str(),
            position
                .as_ref()
                .map(|(date, uid)| (date.as_str(), uid.as_str())),
        )
        .await?;
        let mut more = rows.len() > 64;
        let mut partial = more || before.is_some();
        let allowed = super::collection::visible(self, &rows, actor).await?;
        let mut collection = super::collection::Collection::new(origin, occurrence);
        let mut positions = Vec::new();
        for row in rows.into_iter().take(64) {
            if allowed.contains(&row.record) && !collection.load(self, &row, true, now).await? {
                partial = true;
                more = true;
                break;
            }
            positions.push((
                (row.record.clone(), row.version as u64),
                (row.created_at, row.record),
            ));
        }
        let resolutions = collection.complete(self, actor, now).await?;
        partial |= collection.partial;
        let mut copies = Vec::new();
        let mut unresolved = Vec::new();
        let mut projected_bytes = 0usize;
        let mut consumed = None;
        for (key, position) in positions {
            if collection.selected.contains(&key) {
                let saved = &collection.artifacts[&key];
                let bundle = saved.document.public_bundle().unwrap();
                let scope = &bundle.publication.scope;
                let scope_id = scope.id().map_err(invalid)?;
                let resolution = &resolutions[&scope_id];
                let (projected, pending) = super::collection::project_history(saved, resolution)?;
                partial |= !pending.is_empty();
                let size = serde_json::to_vec(&(&projected, &pending))?.len();
                if projected_bytes.saturating_add(size) > 4 * 1024 * 1024 {
                    partial = true;
                    more = true;
                    break;
                }
                projected_bytes += size;
                copies.extend(projected);
                unresolved.extend(pending);
            }
            consumed = Some(position);
        }
        let next = if more {
            match consumed {
                Some(position) => Some(
                    self.encode_read_cursor(&purpose, &principal, &position, now.timestamp())
                        .await?,
                ),
                None => None,
            }
        } else {
            None
        };
        let missing = resolutions
            .iter()
            .flat_map(|(scope, result)| {
                result
                    .missing
                    .iter()
                    .map(move |event| json!({"scope":scope,"event":event}))
            })
            .collect::<Vec<_>>();
        if resolutions.values().any(|result| {
            result
                .unresolved
                .iter()
                .any(|issue| issue.state != State::Complete)
        }) {
            partial = true;
        }
        let history = receipt_history::merge(copies);
        Ok(ActionOutcome {
            data: Some(
                json!({"receipt":{"state":"source_history","document":{"kind":"source_history","value":{"origin":origin,"occurrence":occurrence,"checked_at":now.timestamp(),"history":history,"unresolved":unresolved,"missing":missing,"partial":partial,"next_before":next,"page_only":before.is_some(),"failures":collection.failures,"coverage":"This page contains permitted local receipt evidence and bounded checks of its local dependencies. Heads and conflicts refer to this page's evidence only; other pages can change that picture. Unresolved events are excluded from accepted outcomes. Missing evidence does not prove non-delivery. Do not add quantities or totals across pages."}}}}),
            ),
            ..Default::default()
        })
    }
}
