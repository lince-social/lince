use crate::{Engine, EngineError};
use std::collections::BTreeSet;

impl Engine {
    pub(crate) async fn configure_fiote(
        &self,
        target: &str,
        parent: Option<&str>,
        run_assigned: bool,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let _guard = self.fiote_config_lock.lock().await;
        self.refuse_unreadable(actor, &[target.to_string()]).await?;
        let row = store::records::get(&self.store.pool, target)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(target.into()))?;
        if !matches!(row.kind.as_str(), "plain" | "person") {
            return Err(EngineError::Consequence(
                "Only a plain Record or Agent can become a Fiote.".into(),
            ));
        }
        let agent = store::concepts::ensure(&self.store.pool, "agent").await?;
        if row.kind == "person" && row.identity_predicate_uid.as_deref() != Some(&agent) {
            return Err(EngineError::Consequence(
                "Choose an Agent, not a human Person, for Fiote.".into(),
            ));
        }
        self.validate_fiote_parent(target, parent, actor).await?;
        store::concepts::ensure(&self.store.pool, "assigned-to").await?;
        let actor_concept = store::concepts::ensure(&self.store.pool, "actor").await?;
        store::concepts::add_parent(&self.store.pool, &agent, &actor_concept).await?;
        let predicate = store::concepts::ensure(&self.store.pool, "descendant-of").await?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        if row.kind == "plain" {
            store::sqlx::query("UPDATE record SET kind = 'person', updated_at = ? WHERE uid = ?")
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(target)
                .execute(&mut *tx)
                .await?;
            store::sync_ops::log_local_tx(
                &mut tx,
                "record",
                target,
                "kind",
                store::sync_ops::OpKind::Set,
                Some(serde_json::json!("person").to_string()),
            )
            .await?;
        }
        store::records::set_extension_on(
            &mut tx,
            target,
            "lince.fiote",
            &serde_json::json!({"run_assigned":run_assigned}),
        )
        .await?;
        let assertions: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid = ? AND predicate_uid = ? AND retracted_at IS NULL")
            .bind(target).bind(&predicate).fetch_all(&mut *tx).await?;
        for assertion in assertions {
            store::assertions::retract_tx(&mut tx, &assertion, actor).await?;
        }
        if let Some(parent) = parent {
            store::assertions::insert_tx(
                &mut tx,
                &nucleus::new_uid("a"),
                store::assertions::NewAssertion {
                    subject_uid: target,
                    predicate_uid: &predicate,
                    object_uid: Some(parent),
                    role: store::assertions::AssertionRole::Ordinary,
                    quantity: None,
                    unit_uid: None,
                    asserted_by: actor,
                },
            )
            .await?;
        }
        tx.commit().await?;
        store::assertions::set_identity(&self.store.pool, target, Some(&agent), actor).await?;
        Ok(())
    }
    pub async fn fiote_parent(&self, target: &str) -> Result<Option<String>, EngineError> {
        let Some(predicate) = store::concepts::resolve(&self.store.pool, "descendant-of").await?
        else {
            return Ok(None);
        };
        let parents =
            store::assertions::object_uids_from_subject(&self.store.pool, target, &predicate)
                .await?;
        if parents.len() > 1 {
            return Err(EngineError::Consequence(
                "A Fiote must have at most one prompt parent.".into(),
            ));
        }
        Ok(parents.into_iter().next())
    }

    pub(crate) async fn validate_fiote_parent(
        &self,
        target: &str,
        parent: Option<&str>,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut visited = BTreeSet::from([target.to_string()]);
        let mut next = parent.map(str::to_owned);
        while let Some(uid) = next {
            if !visited.insert(uid.clone()) || visited.len() > 32 {
                return Err(EngineError::Consequence(
                    "Prompt ancestry must be acyclic and at most 32 Records deep.".into(),
                ));
            }
            self.refuse_unreadable(actor, &[uid.clone()]).await?;
            let _config = store::records::get_extension(&self.store.pool, &uid, "lince.fiote")
                .await?
                .ok_or_else(|| {
                    EngineError::Consequence("Choose a configured Fiote as prompt parent.".into())
                })?;
            if store::records::get(&self.store.pool, &uid).await?.is_none() {
                return Err(EngineError::UnknownRecord(uid));
            }
            next = self.fiote_parent(&uid).await?;
        }
        Ok(())
    }
}
