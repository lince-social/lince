mod dumps;
mod permanent;
mod portable;
pub mod provider;
mod runtime;
mod updates;
pub mod wikimedia;

use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::knowledge::Command;
use provider::{Collection, Provider};
pub use runtime::Runtime;
use serde_json::json;
use std::sync::Arc;

fn invalid(message: &str) -> EngineError {
    EngineError::Consequence(message.into())
}

impl Engine {
    async fn knowledge_provider(
        &self,
        owner: &str,
        source: &str,
    ) -> Result<Arc<dyn Provider>, EngineError> {
        if source == "test" {
            return Ok(Arc::new(provider::test_source()));
        }
        if let Some(provider) = wikimedia::Wikimedia::from_source(source)? {
            return Ok(Arc::new(provider));
        }
        let archive = store::knowledge::archive(&self.store.pool, owner, source)
            .await?
            .ok_or_else(|| invalid("Knowledge source unavailable"))?;
        Ok(Arc::new(Collection { archive }))
    }

    pub(crate) async fn knowledge_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "record:read").await?;
        let owner = store::vocabulary::terminology_owner(actor).to_owned();
        let data = match command {
            Command::Sources => {
                let mut sources = vec![provider::test_source().description()];
                sources.extend(wikimedia::catalog());
                sources.extend(store::knowledge::archives(&self.store.pool, &owner).await?);
                let cache_bytes:i64=store::sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(content AS BLOB))),0) FROM knowledge_cache WHERE owner_key=?").bind(&owner).fetch_one(&self.store.pool).await?;
                let cached:Vec<String>=store::sqlx::query_scalar("SELECT json_object('source',source_uid,'id',item_id,'revision',revision,'title',json_extract(content,'$.title')) FROM knowledge_cache WHERE owner_key=? ORDER BY sequence DESC LIMIT 256").bind(&owner).fetch_all(&self.store.pool).await?;
                let cached = cached
                    .into_iter()
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .collect::<Result<Vec<_>, _>>()?;
                json!({"sources":sources,"cache_bytes":cache_bytes,"cache_limit":nucleus::knowledge::CACHE_LIMIT,"cached_items":cached})
            }
            Command::InstallArchive { package } => {
                let archive = store::knowledge::read_archive(&package)?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                let source =
                    store::knowledge::install_archive_on(&mut tx, &owner, &archive).await?;
                tx.commit().await?;
                json!({"source_uid":source})
            }
            Command::ImportDump {
                format,
                text,
                language,
            } => {
                let archive = dumps::parse(&format, &text, &language)?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                let source =
                    store::knowledge::install_archive_on(&mut tx, &owner, &archive).await?;
                tx.commit().await?;
                json!({"source_uid":source,"items":archive.items.len(),"offline":true})
            }
            Command::ReplaceArchive { source, package } => {
                let archive = store::knowledge::read_archive(&package)?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                store::knowledge::replace_archive_on(&mut tx, &owner, &source, &archive).await?;
                self.knowledge.clear(&owner, Some(&source))?;
                store::knowledge::clear_cache_on(&mut tx, &owner, Some(&source)).await?;
                tx.commit().await?;
                json!({"source_uid":source})
            }
            Command::RemoveSource { source } => {
                self.knowledge.clear(&owner, Some(&source))?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                let changed =
                    store::sqlx::query("DELETE FROM knowledge_source WHERE uid=? AND owner_key=?")
                        .bind(&source)
                        .bind(&owner)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected();
                if changed == 0 {
                    return Err(invalid("Knowledge source unavailable"));
                }
                store::knowledge::clear_cache_on(&mut tx, &owner, Some(&source)).await?;
                tx.commit().await?;
                json!({"removed":source})
            }
            Command::Search {
                request_id,
                sources,
                text,
                offset,
            } => {
                if sources.is_empty()
                    || sources.len() > 8
                    || text.len() > 256
                    || text.contains('\0')
                    || offset > 10000
                {
                    return Err(invalid(
                        "Choose 1–8 knowledge sources and search text of at most 256 bytes",
                    ));
                }
                let mut providers = Vec::new();
                let mut unique = std::collections::HashSet::new();
                for source in sources {
                    if !unique.insert(source.clone()) {
                        return Err(invalid("Choose each source once"));
                    }
                    providers.push(self.knowledge_provider(&owner, &source).await?);
                }
                let control = self.knowledge.start(
                    &owner,
                    &request_id,
                    providers
                        .iter()
                        .map(|provider| provider.description().id)
                        .collect(),
                )?;
                let runtime = self.knowledge.clone();
                let id = request_id.clone();
                tokio::spawn(async move {
                    let request = async {
                        let mut results = Vec::new();
                        for source in providers {
                            control.checkpoint()?;
                            let result = source.search(&text, offset, &control).await?;
                            if result.items.len() > nucleus::knowledge::RESULT_LIMIT
                                || result.source != source.description().id
                            {
                                return Err(invalid(
                                    "The knowledge source exceeded its result limits",
                                ));
                            }
                            results.push(result);
                        }
                        let data = json!({"sources":results});
                        if serde_json::to_vec(&data)?.len() > nucleus::knowledge::ITEM_LIMIT {
                            return Err(invalid("Knowledge search results exceed 4 MiB"));
                        }
                        Ok(data)
                    };
                    let result = tokio::select! {_=control.cancelled()=>Err(invalid("Knowledge request cancelled")),result=tokio::time::timeout(std::time::Duration::from_secs(20),request)=>result.unwrap_or_else(|_|Err(invalid("Knowledge source timed out")))};
                    runtime.finish(&id, result);
                });
                json!({"request_id":request_id,"status":"loading","complete":false})
            }
            Command::Read {
                request_id,
                source,
                item,
            } => {
                if item.is_empty() || item.len() > 1024 || item.contains('\0') {
                    return Err(invalid("Choose an existing source item"));
                }
                let provider = self.knowledge_provider(&owner, &source).await?;
                let control = self
                    .knowledge
                    .start(&owner, &request_id, vec![source.clone()])?;
                let runtime = self.knowledge.clone();
                let pool = self.store.pool.clone();
                let id = request_id.clone();
                tokio::spawn(async move {
                    let request = async {
                        let value = provider.read(&item, &control).await?;
                        control.checkpoint()?;
                        if value.source != source || value.id != item {
                            return Err(invalid("The source returned a different item identity"));
                        }
                        store::knowledge::validate_item(&value)?;
                        let mut tx = store::write_tx(&pool).await?;
                        control.checkpoint()?;
                        store::knowledge::cache_on(&mut tx, &owner, &value).await?;
                        tx.commit().await?;
                        Ok(
                            json!({"item":value,"cached":true,"offline":!provider.description().online}),
                        )
                    };
                    let result = tokio::select! {_=control.cancelled()=>Err(invalid("Knowledge request cancelled")),result=tokio::time::timeout(std::time::Duration::from_secs(20),request)=>result.unwrap_or_else(|_|Err(invalid("Knowledge source timed out")))};
                    runtime.finish(&id, result);
                });
                json!({"request_id":request_id,"status":"loading","complete":false})
            }
            Command::Poll { request_id } => self.knowledge.poll(&owner, &request_id)?,
            Command::ReadCached { source, item } => {
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                let item = store::knowledge::cached_on(&mut tx, &owner, &source, &item)
                    .await?
                    .ok_or_else(|| invalid("Temporary source content is unavailable offline"))?;
                tx.commit().await?;
                json!({"item":item,"cached":true,"offline":true})
            }
            Command::Cancel { request_id } => {
                self.knowledge.cancel(&owner, &request_id)?;
                self.knowledge.poll(&owner, &request_id)?
            }
            Command::ClearCache { source } => {
                self.knowledge.clear(&owner, source.as_deref())?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                store::knowledge::clear_cache_on(&mut tx, &owner, source.as_deref()).await?;
                store::sqlx::query("DELETE FROM knowledge_review WHERE owner_key=? AND applied IS NULL AND (? IS NULL OR json_extract(content,'$.source')=? OR json_extract(content,'$.incoming.source')=? OR EXISTS(SELECT 1 FROM json_each(content,'$.entries') e WHERE json_extract(e.value,'$.item.source')=?))").bind(&owner).bind(&source).bind(&source).bind(&source).bind(&source).execute(&mut *tx).await?;
                tx.commit().await?;
                json!({"cleared":true})
            }
            Command::MapMeaning {
                source,
                external,
                kind,
                concept,
            } => {
                self.knowledge_provider(&owner, &source).await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:read")
                    .await?;
                let mapping = store::knowledge::map_meaning_on(
                    &mut tx, &owner, &source, &external, kind, &concept,
                )
                .await?;
                tx.commit().await?;
                json!({"mapping":mapping})
            }
            Command::Reference {
                source,
                item,
                organ,
            } => {
                self.knowledge_reference(&source, &item, &organ, actor)
                    .await?
            }
            Command::InspectSaved { record } => {
                self.inspect_saved_knowledge(&record, actor).await?
            }
            Command::PreviewSave {
                source,
                item,
                organ,
                selection,
            } => {
                self.preview_knowledge_save(&source, &item, &organ, &selection, actor)
                    .await?
            }
            Command::Save { preview } => self.save_knowledge_review(&preview, actor).await?,
            Command::Export { record } => self.export_knowledge(&record, actor).await?,
            Command::Import { package, organ } => {
                self.import_knowledge(&package, &organ, actor).await?
            }
            Command::ReviewUpdate { record } => {
                self.review_knowledge_update(&record, actor).await?
            }
            Command::ApplyUpdate {
                review,
                fields,
                fork,
            } => {
                self.apply_knowledge_update(&review, &fields, fork, actor)
                    .await?
            }
        };
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }
}
