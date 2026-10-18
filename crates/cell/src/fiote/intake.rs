use super::*;
use chrono::{DateTime, FixedOffset};
use fiote::config::Intake;

impl Host {
    pub(crate) async fn addressed_title(&self, record: &str) -> Result<Option<String>, String> {
        Ok(self
            .fiote_choices()
            .await?
            .into_iter()
            .find(|choice| choice.record == record)
            .map(|choice| choice.title))
    }

    pub async fn intake_selection(&self) -> Result<Option<Intake>, String> {
        let rows: Vec<(String, String, String, i64)> = store::sqlx::query_as(
            "WITH parents AS (SELECT DISTINCT a.subject_uid AS child, a.object_uid AS parent FROM record_assertion a JOIN vocabulary_role_concept c ON c.concept_uid = a.predicate_uid JOIN vocabulary_feature f ON f.role_uid = c.role_uid AND f.feature = 'ancestry' JOIN record child ON child.uid = a.subject_uid AND child.deleted_at IS NULL JOIN record_extension ce ON ce.record_uid = child.uid AND ce.namespace = 'lince.fiote' WHERE a.retracted_at IS NULL AND a.object_uid IS NOT NULL) SELECT r.uid, r.head, r.created_at, COUNT(DISTINCT p.child) FROM record r JOIN record_extension e ON e.record_uid = r.uid AND e.namespace = 'lince.fiote' LEFT JOIN parents p ON p.parent = r.uid WHERE r.deleted_at IS NULL GROUP BY r.uid ORDER BY r.uid LIMIT 10001",
        ).fetch_all(&self.engine.store.pool).await.map_err(|error| error.to_string())?;
        if rows.len() > 10_000 {
            return Err("Assistant selection supports up to 10,000 Fiotes; the hierarchy was not ranked from a truncated list.".into());
        }
        let mut candidates = Vec::with_capacity(rows.len());
        for (record, title, created_at, children) in rows {
            let instant = created_at
                .parse::<DateTime<FixedOffset>>()
                .map_err(|_| format!("Fiote {record} has an invalid creation timestamp."))?;
            let children = usize::try_from(children).map_err(|_| "Invalid Fiote child count.")?;
            candidates.push((children, instant, record, title, created_at));
        }
        candidates.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        let Some((children, _, record, title, created_at)) = candidates.into_iter().next() else {
            return Ok(None);
        };
        let enabled = self
            .load(&record)?
            .is_some_and(|config| config.settings.enabled);
        Ok(Some(Intake {
            record,
            title,
            children,
            created_at,
            enabled,
        }))
    }
}
