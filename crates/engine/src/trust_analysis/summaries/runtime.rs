use super::*;

impl Engine {
    pub async fn schedule_claim_recipes(&self) -> Result<(), EngineError> {
        store::sqlx::query("UPDATE profile_claim_recipe_runtime SET next_at=? WHERE record_uid IN (SELECT c.record_uid FROM profile_claim_recipe_record c JOIN record r ON r.uid=c.record_uid WHERE r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_recipe_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.recipe_uid=c.recipe_uid))").bind(nucleus::execution::now().to_rfc3339()).execute(&self.store.pool).await?;
        Ok(())
    }

    pub async fn refresh_claim_recipes_once(&self) -> Result<usize, EngineError> {
        let now = nucleus::execution::now();
        let rows:Vec<(String,String)>=store::sqlx::query_as("SELECT c.record_uid,c.owner_person_uid FROM profile_claim_recipe_record c JOIN record r ON r.uid=c.record_uid JOIN profile_claim_recipe_runtime s ON s.record_uid=c.record_uid WHERE r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_recipe_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.recipe_uid=c.recipe_uid) AND ((SELECT mode FROM profile_claim_recipe_control k WHERE k.record_uid=c.record_uid ORDER BY revision DESC LIMIT 1)='enabled' AND s.status NOT IN ('expired','output_superseded') OR ((SELECT mode FROM profile_claim_recipe_control k WHERE k.record_uid=c.record_uid ORDER BY revision DESC LIMIT 1)='revoked' AND s.status!='revoked')) AND (s.next_at IS NULL OR s.next_at<=?) ORDER BY COALESCE(s.checked_at,''),c.record_uid LIMIT 4").bind(now.to_rfc3339()).fetch_all(&self.store.pool).await?;
        let count = rows.len();
        for (record, owner) in rows {
            let outcome = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                self.access_scope(true, async {
                    if !store::people::is_active(&self.store.pool, &owner).await? {
                        return Err(invalid("The recipe controller is no longer active"));
                    }
                    self.recompute_claim_recipe(
                        &owner,
                        Some(&owner),
                        &record,
                        nucleus::execution::now(),
                    )
                    .await
                }),
            )
            .await;
            if !matches!(outcome, Ok(Ok(_))) {
                let status = if matches!(outcome, Err(_)) {
                    "deferred"
                } else {
                    "permission_or_authority_withheld"
                };
                let now = nucleus::execution::now();
                store::claim_recipes::status_on(
                    &mut *self.store.pool.acquire().await?,
                    &record,
                    status,
                    now,
                    now + chrono::TimeDelta::seconds(60),
                    None,
                    None,
                )
                .await?;
            }
            tokio::task::yield_now().await;
        }
        Ok(count)
    }

    pub async fn next_claim_recipe_refresh(&self) -> Result<Option<DateTime<Utc>>, EngineError> {
        let next:Option<String>=store::sqlx::query_scalar("SELECT MIN(s.next_at) FROM profile_claim_recipe_runtime s JOIN profile_claim_recipe_record c ON c.record_uid=s.record_uid JOIN record r ON r.uid=c.record_uid WHERE r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_recipe_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.recipe_uid=c.recipe_uid) AND ((SELECT mode FROM profile_claim_recipe_control k WHERE k.record_uid=c.record_uid ORDER BY revision DESC LIMIT 1)='enabled' AND s.status NOT IN ('expired','output_superseded') OR ((SELECT mode FROM profile_claim_recipe_control k WHERE k.record_uid=c.record_uid ORDER BY revision DESC LIMIT 1)='revoked' AND s.status!='revoked'))").fetch_one(&self.store.pool).await?;
        next.map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|error| invalid(error.to_string()))
        })
        .transpose()
    }

    pub async fn is_generated_claim_record(&self, record: &str) -> Result<bool, EngineError> {
        Ok(store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profile_claim_record WHERE record_uid=? AND generation_recipe_uid IS NOT NULL)").bind(record).fetch_one(&self.store.pool).await?)
    }
}
