use crate::{Engine, EngineError};
use serde_json::{Value, json};
use store::sqlx::{Row, Sqlite, Transaction};

impl Engine {
    pub(super) async fn inspect_meaning_on(
        &self,
        connection: &mut Transaction<'_, Sqlite>,
        concept: &str,
        actor: Option<&str>,
        offset: u32,
    ) -> Result<Value, EngineError> {
        let dictionary = store::vocabulary::dictionary_on(connection, concept).await?;
        let origin: Option<String> =
            store::sqlx::query_scalar("SELECT origin_organ FROM concept WHERE uid=?")
                .bind(concept)
                .fetch_one(&mut **connection)
                .await?;
        let terms = store::vocabulary::terms_on(connection, actor)
            .await?
            .into_iter()
            .filter(|term| term.concept_uid == concept)
            .collect::<Vec<_>>();
        let mut candidates:Vec<String>=store::sqlx::query_scalar("SELECT subject_uid FROM record_assertion WHERE retracted_at IS NULL AND (predicate_uid=? OR unit_uid=?) UNION SELECT uid FROM record WHERE unit_uid=? AND deleted_at IS NULL ORDER BY 1 LIMIT 4097")
            .bind(concept).bind(concept).bind(concept).fetch_all(&mut **connection).await?;
        let limited = candidates.len() > 4096;
        candidates.truncate(4096);
        let mut visible = Vec::new();
        for batch in candidates.chunks(128) {
            let graph = crate::access::policy_graph_on(connection, batch)
                .await
                .map_err(|error| EngineError::Forbidden(error.to_string()))?;
            let readable = self.readable_on(connection, actor, &graph).await?;
            visible.extend(batch.iter().filter(|uid| readable.contains(*uid)).cloned());
        }
        let mut uses = Vec::new();
        for uid in visible.iter().skip(offset as usize).take(30) {
            let row = store::sqlx::query(
                "SELECT head,slug,kind FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(uid)
            .fetch_optional(&mut **connection)
            .await?;
            if let Some(row) = row {
                let revisions:Vec<(i64,Option<i64>)>=store::sqlx::query_as("SELECT predicate_revision,unit_revision FROM record_assertion WHERE subject_uid=? AND retracted_at IS NULL AND (predicate_uid=? OR unit_uid=?) ORDER BY uid LIMIT 128").bind(uid).bind(concept).bind(concept).fetch_all(&mut **connection).await?;
                uses.push(json!({"uid":uid,"head":row.get::<String,_>("head"),"slug":row.get::<Option<String>,_>("slug"),"kind":row.get::<String,_>("kind"),"assertion_versions":revisions}));
            }
        }
        Ok(
            json!({"concept_uid":concept,"origin_organ":origin,"source":dictionary.source,"definitions":dictionary.definitions,"terms":terms,"uses":uses,"visible_uses":visible.len(),"offset":offset,"limited":limited,"terminology":store::vocabulary::terminology_state_on(connection,actor).await?}),
        )
    }
}
