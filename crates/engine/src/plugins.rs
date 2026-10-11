mod document;
mod sync;

use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::plugins::{
    Binding, BindingStatus, Catalog, Command, Contribution, Delivery, Direction,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::Arc};
use store::sqlx::Row;

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn digest(value: &Value) -> Result<String, EngineError> {
    Ok(Sha256::digest(serde_json::to_vec(value)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn binding_row(row: &store::sqlx::sqlite::SqliteRow) -> Result<Binding, EngineError> {
    Ok(Binding {
        uid: row.get("uid"),
        provider: row.get("provider"),
        name: row.get("name"),
        settings: serde_json::from_str(&row.get::<String, _>("settings"))?,
        protein_uid: row.get("protein_uid"),
        incoming_filter: serde_json::from_str(&row.get::<String, _>("incoming_filter"))?,
        direction: match row.get::<String, _>("direction").as_str() {
            "incoming" => Direction::Incoming,
            "outgoing" => Direction::Outgoing,
            "both" => Direction::Both,
            _ => return Err(invalid("Invalid stored Sync direction")),
        },
        enabled: row.get("enabled"),
        interval_seconds: row.get::<i64, _>("interval_seconds") as u64,
        revision: row.get::<i64, _>("revision") as u64,
    })
}

impl Engine {
    pub async fn load_plugins(&self, directory: std::path::PathBuf) -> Result<(), EngineError> {
        let host = ::plugins::Host::discover(directory)
            .await
            .map_err(invalid)?;
        self.plugins
            .set(Arc::new(host))
            .map_err(|_| invalid("Plugins are already loaded"))?;
        store::sqlx::query("UPDATE plugin_outbox SET state='uncertain',result='Cell stopped during delivery; inspect before retrying' WHERE state='running'").execute(&self.store.pool).await?;
        Ok(())
    }

    fn plugin_host(&self) -> Result<&Arc<::plugins::Host>, EngineError> {
        self.plugins
            .get()
            .ok_or_else(|| invalid("Plugin host is unavailable on this Cell"))
    }

    async fn approved_plugin(&self, plugin: &str) -> Result<String, EngineError> {
        let digest: Option<String> =
            store::sqlx::query_scalar("SELECT digest FROM plugin_package WHERE id=? AND enabled=1")
                .bind(plugin)
                .fetch_optional(&self.store.pool)
                .await?;
        digest.ok_or_else(|| {
            EngineError::Forbidden("Enable this trusted native plugin before using it".into())
        })
    }

    async fn plugin_call(
        &self,
        qualified: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, EngineError> {
        let host = self.plugin_host()?;
        let (manifest, contribution) = host.contribution(qualified).map_err(invalid)?;
        let approved = self.approved_plugin(&manifest.id).await?;
        host.call(&manifest.id, &approved, contribution.id(), operation, input)
            .await
            .map_err(invalid)
    }

    pub async fn plugin_catalog(&self) -> Result<Catalog, EngineError> {
        let mut packages = self
            .plugins
            .get()
            .map(|host| host.packages())
            .unwrap_or_default();
        for package in &mut packages {
            package.enabled = self
                .approved_plugin(&package.manifest.id)
                .await
                .is_ok_and(|digest| digest == package.digest);
        }
        let mut diagnostics = self
            .plugins
            .get()
            .map(|host| host.diagnostics.clone())
            .unwrap_or_else(|| vec!["Plugin host is unavailable on this Cell".into()]);
        let approved: Vec<String> =
            store::sqlx::query_scalar("SELECT id FROM plugin_package WHERE enabled=1")
                .fetch_all(&self.store.pool)
                .await?;
        for id in approved {
            if !packages
                .iter()
                .any(|package| package.manifest.id == id && package.enabled)
            {
                diagnostics.push(format!(
                    "{id}: installed package is missing or changed; review its version"
                ));
            }
        }
        let rows = store::sqlx::query("SELECT b.*, (SELECT COUNT(*) FROM plugin_outbox WHERE binding_uid=b.uid AND state IN ('queued','running')) AS pending, (SELECT COUNT(*) FROM plugin_outbox WHERE binding_uid=b.uid AND state IN ('failed','uncertain')) AS failed FROM plugin_binding b ORDER BY name,uid LIMIT 128").fetch_all(&self.store.pool).await?;
        let mut bindings = Vec::new();
        for row in rows {
            let binding = binding_row(&row)?;
            let mut status: String = row.get("status");
            let available = binding.provider.split_once('/').is_some_and(|(plugin, _)| {
                packages
                    .iter()
                    .any(|package| package.manifest.id == plugin && package.enabled)
            });
            if !available {
                status = "Plugin unavailable or disabled".into();
            }
            bindings.push(BindingStatus {
                binding,
                status,
                pending: row.get::<i64, _>("pending") as u64,
                failed: row.get::<i64, _>("failed") as u64,
            });
        }
        let proteins = store::sqlx::query("SELECT uid,head FROM record WHERE kind='protein' AND deleted_at IS NULL ORDER BY head,uid LIMIT 100").fetch_all(&self.store.pool).await?.into_iter().map(|row| (row.get("uid"), row.get("head"))).collect();
        let deliveries = store::sqlx::query("SELECT uid,binding_uid,record_uid,state,result FROM plugin_outbox ORDER BY created_at DESC,uid DESC LIMIT 50").fetch_all(&self.store.pool).await?.into_iter().map(|row| Delivery { uid: row.get("uid"), binding: row.get("binding_uid"), record: row.get("record_uid"), state: row.get("state"), result: row.get("result") }).collect();
        Ok(Catalog {
            packages,
            diagnostics,
            bindings,
            proteins,
            deliveries,
        })
    }

    pub async fn plugin_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        if actor.is_some() || crate::operation_origin::is_fiote() {
            return Err(EngineError::Forbidden(
                "Plugin management currently requires the local interface session".into(),
            ));
        }
        if nucleus::execution::current().is_some() {
            return Err(invalid(
                "Executable plugins are unavailable during Simulation",
            ));
        }
        let data = match command {
            Command::Inspect => serde_json::to_value(self.plugin_catalog().await?)?,
            Command::Enable {
                plugin,
                digest,
                enabled,
            } => {
                let package = self
                    .plugin_host()?
                    .packages()
                    .into_iter()
                    .find(|package| package.manifest.id == plugin)
                    .ok_or_else(|| invalid("Plugin is unavailable"))?;
                if digest != package.digest {
                    return Err(invalid(
                        "Plugin version changed; inspect its manifest again",
                    ));
                }
                if enabled {
                    self.plugin_host()?
                        .call(&plugin, &digest, "", "initialize", json!({}))
                        .await
                        .map_err(invalid)?;
                }
                store::sqlx::query("INSERT INTO plugin_package(id,digest,enabled) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET digest=excluded.digest,enabled=excluded.enabled").bind(&plugin).bind(&digest).bind(enabled).execute(&self.store.pool).await?;
                serde_json::to_value(self.plugin_catalog().await?)?
            }
            Command::Invoke {
                contribution,
                arguments,
            } => {
                let (_, descriptor) = self
                    .plugin_host()?
                    .contribution(&contribution)
                    .map_err(invalid)?;
                let Contribution::Action {
                    arguments: fields, ..
                } = descriptor
                else {
                    return Err(invalid("Choose an Action contribution"));
                };
                nucleus::plugins::validate_values(fields, &arguments).map_err(invalid)?;
                json!({"result":self.plugin_call(&contribution, "invoke", arguments).await?})
            }
            Command::Preview { binding } => self.plugin_preview(&binding).await?,
            Command::SaveBinding { binding, preview } => {
                let _guard = self.plugin_work.lock().await;
                self.validate_plugin_binding(&binding).await?;
                if binding.enabled && binding.direction.outgoing() {
                    let current = self.plugin_preview(&binding).await?;
                    if preview.as_deref() != current["preview"].as_str() {
                        return Err(invalid(
                            "Preview the current selected Records before enabling publication",
                        ));
                    }
                }
                self.validate_document_destination(&binding).await?;
                self.plugin_call(
                    &binding.provider,
                    "validate",
                    json!({"settings":binding.settings}),
                )
                .await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                let exists: Option<i64> =
                    store::sqlx::query_scalar("SELECT revision FROM plugin_binding WHERE uid=?")
                        .bind(&binding.uid)
                        .fetch_optional(&mut *tx)
                        .await?;
                if exists.unwrap_or(0) != binding.revision as i64 {
                    return Err(invalid("Sync binding changed; reload it before saving"));
                }
                if exists.is_some() {
                    let provider: String = store::sqlx::query_scalar(
                        "SELECT provider FROM plugin_binding WHERE uid=?",
                    )
                    .bind(&binding.uid)
                    .fetch_one(&mut *tx)
                    .await?;
                    if provider != binding.provider {
                        return Err(invalid("Create a new binding to change its provider"));
                    }
                    let pending: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM plugin_outbox WHERE binding_uid=? AND state<>'delivered')").bind(&binding.uid).fetch_one(&mut *tx).await?;
                    let old_settings: String = store::sqlx::query_scalar(
                        "SELECT settings FROM plugin_binding WHERE uid=?",
                    )
                    .bind(&binding.uid)
                    .fetch_one(&mut *tx)
                    .await?;
                    if pending && old_settings != binding.settings.to_string() {
                        return Err(invalid(
                            "Resolve retained deliveries before changing connection settings",
                        ));
                    }
                } else {
                    let count: i64 =
                        store::sqlx::query_scalar("SELECT COUNT(*) FROM plugin_binding")
                            .fetch_one(&mut *tx)
                            .await?;
                    if count >= 128 {
                        return Err(invalid("At most 128 Sync bindings may be configured"));
                    }
                }
                store::sqlx::query("INSERT INTO plugin_binding(uid,provider,name,settings,protein_uid,incoming_filter,direction,interval_seconds,enabled) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(uid) DO UPDATE SET name=excluded.name,cursor=CASE WHEN plugin_binding.settings<>excluded.settings OR plugin_binding.incoming_filter<>excluded.incoming_filter THEN 'null' ELSE plugin_binding.cursor END,settings=excluded.settings,protein_uid=excluded.protein_uid,incoming_filter=excluded.incoming_filter,direction=excluded.direction,interval_seconds=excluded.interval_seconds,enabled=excluded.enabled,revision=revision+1,last_run=0,status='Settings saved'")
                    .bind(&binding.uid).bind(&binding.provider).bind(&binding.name).bind(binding.settings.to_string()).bind(&binding.protein_uid).bind(binding.incoming_filter.to_string()).bind(binding.direction.name()).bind(binding.interval_seconds as i64).bind(binding.enabled).execute(&mut *tx).await?;
                tx.commit().await?;
                serde_json::to_value(self.plugin_catalog().await?)?
            }
            Command::Run { binding } => {
                let changed = store::sqlx::query("UPDATE plugin_binding SET last_run=0,status='Refresh requested' WHERE uid=? AND enabled=1").bind(&binding).execute(&self.store.pool).await?;
                if changed.rows_affected() == 0 {
                    return Err(invalid("Enable the binding before refreshing"));
                }
                serde_json::to_value(self.plugin_catalog().await?)?
            }
            Command::Stop { binding } => {
                store::sqlx::query("UPDATE plugin_binding SET enabled=0,status='Stopped',revision=revision+1 WHERE uid=?").bind(binding).execute(&self.store.pool).await?;
                serde_json::to_value(self.plugin_catalog().await?)?
            }
            Command::Retry { delivery } => {
                let row = store::sqlx::query("SELECT o.binding_uid,b.provider,o.state FROM plugin_outbox o JOIN plugin_binding b ON b.uid=o.binding_uid WHERE o.uid=?").bind(&delivery).fetch_optional(&self.store.pool).await?.ok_or_else(|| invalid("Delivery is unavailable"))?;
                let (_, provider) = self
                    .plugin_host()?
                    .contribution(&row.get::<String, _>("provider"))
                    .map_err(invalid)?;
                if !matches!(
                    provider,
                    Contribution::Sync {
                        replay_safe: true,
                        ..
                    }
                ) {
                    return Err(invalid(
                        "This provider cannot safely replay an uncertain delivery",
                    ));
                }
                store::sqlx::query("UPDATE plugin_outbox SET state='queued',result='Retry requested' WHERE uid=? AND state IN ('failed','uncertain')").bind(&delivery).execute(&self.store.pool).await?;
                serde_json::to_value(self.plugin_catalog().await?)?
            }
        };
        self.notify_query_changed();
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }

    async fn validate_plugin_binding(&self, binding: &Binding) -> Result<(), EngineError> {
        if !nucleus::valid_uid(&binding.uid, "r")
            || binding.name.trim().is_empty()
            || binding.name.len() > 160
            || !(30..=86400).contains(&binding.interval_seconds)
            || binding.revision > i64::MAX as u64
        {
            return Err(invalid(
                "Give the Sync binding a valid identity, name, and interval",
            ));
        }
        let (manifest, contribution) = self
            .plugin_host()?
            .contribution(&binding.provider)
            .map_err(invalid)?;
        let Contribution::Sync {
            incoming,
            outgoing,
            settings,
            ..
        } = contribution
        else {
            return Err(invalid("Choose a Sync provider"));
        };
        if (binding.direction.incoming() && !incoming)
            || (binding.direction.outgoing() && !outgoing)
        {
            return Err(invalid(
                "This provider does not support the selected direction",
            ));
        }
        self.approved_plugin(&manifest.id).await?;
        nucleus::plugins::validate_values(settings, &binding.settings).map_err(invalid)?;
        sync::incoming_filters(&binding.incoming_filter)?;
        if binding.direction.outgoing() {
            if self.document_format(binding)?.is_some() {
                self.document_query(binding).await?;
                document::target(
                    &binding.settings,
                    &self
                        .document_format(binding)?
                        .ok_or_else(|| invalid("Missing document format"))?
                        .extension,
                )?;
            } else {
                self.plugin_selection(binding).await?;
            }
        }
        Ok(())
    }

    async fn plugin_selection(
        &self,
        binding: &Binding,
    ) -> Result<Vec<store::records::RecordRow>, EngineError> {
        let uid = binding
            .protein_uid
            .as_deref()
            .ok_or_else(|| invalid("Choose a saved Protein for outgoing publication"))?;
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| invalid("Selected Protein is unavailable"))?;
        if record.kind != "protein" {
            return Err(invalid("Choose a saved Protein"));
        }
        let ast = store::records::get_extension(&self.store.pool, uid, "lince.protein")
            .await?
            .ok_or_else(|| invalid("Selected Protein has no query"))?;
        let mut query: protein::Protein = serde_json::from_value(ast)?;
        protein::validate(&query)?;
        if query.source != protein::Source::Record || query.aggregate.is_some() {
            return Err(invalid(
                "Publication requires a Record Protein without aggregation",
            ));
        }
        query.limit = Some(
            query
                .limit
                .unwrap_or(nucleus::plugins::MAX_ITEMS)
                .min(nucleus::plugins::MAX_ITEMS),
        );
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let visible: HashSet<String> = store::records::list_all(&self.store.pool)
            .await?
            .into_iter()
            .filter(|row| row.organ_uid.as_deref() == Some(&organ.uid) && row.kind == "plain")
            .map(|row| row.uid)
            .collect();
        let rows = protein::select_records(&self.store, &query, Some(&visible)).await?;
        let mut eligible = Vec::new();
        for row in rows {
            if store::records::get_extension(&self.store.pool, &row.uid, "lince.source")
                .await?
                .is_none()
            {
                eligible.push(row);
            }
        }
        Ok(eligible)
    }

    async fn plugin_preview(&self, binding: &Binding) -> Result<Value, EngineError> {
        self.validate_plugin_binding(binding).await?;
        if self.document_format(binding)?.is_some() {
            return self.document_preview(binding).await;
        }
        let rows = if binding.direction.outgoing() {
            self.plugin_selection(binding).await?
        } else {
            Vec::new()
        };
        let records: Vec<Value> = rows
            .iter()
            .map(|row| json!({"uid":row.uid,"head":row.head,"body":row.body}))
            .collect();
        let plugin = binding
            .provider
            .split_once('/')
            .ok_or_else(|| invalid("Invalid provider"))?
            .0;
        let token = digest(
            &json!({"binding":binding,"records":records,"plugin_digest":self.approved_plugin(plugin).await?}),
        )?;
        Ok(
            json!({"preview":token,"records":records,"count":records.len(),"limit":nucleus::plugins::MAX_ITEMS}),
        )
    }
}
