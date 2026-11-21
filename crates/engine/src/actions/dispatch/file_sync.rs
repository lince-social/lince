// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchFuture, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_file_sync(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::SetFileSyncEnabled { enabled } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("Local Organ is unavailable".into())
                        })?;
                    let mut config = store::records::get_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                    )
                    .await?
                    .filter(serde_json::Value::is_object)
                    .unwrap_or_else(|| serde_json::json!({"format":"lingua"}));
                    config["enabled"] = enabled.into();
                    self.check_legacy_sync_path(&config).await?;
                    store::records::set_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                        &config,
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            organ.uid,
                            actor,
                            serde_json::json!({"extension":"lince.file_sync"}),
                            now,
                        )
                        .await?;
                }
                Action::ConfigureFileSync {
                    protein,
                    path,
                    format,
                    enabled,
                } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can configure directory sync".into(),
                        ));
                    }
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("Local Organ is unavailable".into())
                        })?;
                    let config = if enabled {
                        let path = path.trim();
                        if !std::path::Path::new(path).is_absolute() {
                            return Err(EngineError::Consequence(
                                "Choose an absolute directory path".into(),
                            ));
                        }
                        if std::path::Path::new(path).exists()
                            && !std::path::Path::new(path).is_dir()
                        {
                            return Err(EngineError::Consequence(
                                "The sync path must be a directory".into(),
                            ));
                        }
                        let mut config =
                            serde_json::json!({"enabled": true, "path": path, "format": format});
                        if !protein.trim().is_empty() {
                            let uid = self.resolve(&protein).await?;
                            self.file_sync_protein(&uid).await?;
                            config["protein"] = uid.into();
                        }
                        config
                    } else {
                        let mut config = store::records::get_extension(
                            &self.store.pool,
                            &organ.uid,
                            "lince.file_sync",
                        )
                        .await?
                        .filter(serde_json::Value::is_object)
                        .unwrap_or_else(|| serde_json::json!({}));
                        config["enabled"] = false.into();
                        config
                    };
                    self.check_legacy_sync_path(&config).await?;
                    store::records::set_extension(
                        &self.store.pool,
                        &organ.uid,
                        "lince.file_sync",
                        &config,
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            organ.uid,
                            actor,
                            serde_json::json!({"extension": "lince.file_sync"}),
                            now,
                        )
                        .await?;
                }
                Action::DirectorySync { request } => {
                    if actor.is_some() {
                        return Err(EngineError::Forbidden(
                            "Only the local owner can manage directory syncs".into(),
                        ));
                    }
                    let changed = !matches!(request, crate::file_sync::Request::List);
                    outcome.data = Some(self.directory_sync_command(request).await?);
                    if changed {
                        let organ =
                            store::organs::local(&self.store.pool)
                                .await?
                                .ok_or_else(|| {
                                    EngineError::Consequence("Local Organ is unavailable".into())
                                })?;
                        outcome.facts = self
                            .annotate(
                                organ.uid,
                                actor,
                                serde_json::json!({"extension": "lince.file_sync"}),
                                now,
                            )
                            .await?;
                    }
                }
                Action::FileSyncStatus { organ } => {
                    let organ_uid = self.resolve(&organ).await?;
                    outcome.data = Some(serde_json::json!({
                        "checked": self.file_sync_has_ticked(&organ_uid),
                        "conflicts": self
                            .file_sync_conflicts(&organ_uid)
                            .into_iter()
                            .map(|c| serde_json::json!({ "path": c.path, "reason": c.reason }))
                            .collect::<Vec<_>>(),
                    }));
                }
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
