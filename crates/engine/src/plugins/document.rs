use super::*;
use nucleus::plugins::{DocumentFormat, ExportDocument, ExportSnapshot, SnapshotSource};
use std::path::{Component, Path, PathBuf};

pub(super) fn updating(error: &EngineError) -> bool {
    matches!(error, EngineError::Consequence(message) if message.starts_with("Simulation is updating;"))
}

fn selection_rows(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            if row["origin"]["allocation"]["cursor_offset_ms"].is_i64() {
                row["time"] = Value::Null;
            }
            row
        })
        .collect()
}

pub(super) fn target(settings: &Value, extension: &str) -> Result<PathBuf, EngineError> {
    let directory = settings["directory"]
        .as_str()
        .ok_or_else(|| invalid("Choose an export directory"))?;
    let filename = settings["filename"]
        .as_str()
        .ok_or_else(|| invalid("Choose an export filename"))?;
    let directory = Path::new(directory);
    if !directory.is_absolute()
        || directory
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || filename.is_empty()
        || filename.len() > 255
        || filename.starts_with('.')
        || filename.contains(['/', '\\'])
        || filename.chars().any(char::is_control)
        || Path::new(filename)
            .extension()
            .and_then(|value| value.to_str())
            != Some(extension)
    {
        return Err(invalid(format!(
            "Use an absolute directory and a plain .{extension} filename"
        )));
    }
    Ok(directory.join(filename))
}

async fn normalized(path: &Path) -> Result<PathBuf, EngineError> {
    let mut ancestor = path.to_path_buf();
    let mut suffix = Vec::new();
    while !tokio::fs::try_exists(&ancestor)
        .await
        .map_err(|error| invalid(error.to_string()))?
    {
        suffix.push(
            ancestor
                .file_name()
                .ok_or_else(|| invalid("Invalid export path"))?
                .to_owned(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| invalid("Invalid export path"))?
            .to_path_buf();
    }
    let mut result = tokio::fs::canonicalize(ancestor)
        .await
        .map_err(|error| invalid(error.to_string()))?;
    for component in suffix.into_iter().rev() {
        result.push(component);
    }
    Ok(result)
}

impl Engine {
    pub(super) fn document_format(
        &self,
        binding: &Binding,
    ) -> Result<Option<DocumentFormat>, EngineError> {
        let (_, contribution) = self
            .plugin_host()?
            .contribution(&binding.provider)
            .map_err(invalid)?;
        match contribution {
            Contribution::Sync { document, .. } => Ok(document.clone()),
            _ => Err(invalid("Choose a Sync provider")),
        }
    }

    pub(super) async fn validate_document_destination(
        &self,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let Some(format) = self.document_format(binding)? else {
            return Ok(());
        };
        let destination = normalized(&target(&binding.settings, &format.extension)?).await?;
        let rows: Vec<(String, String)> =
            store::sqlx::query_as("SELECT uid,settings FROM plugin_binding WHERE uid<>?")
                .bind(&binding.uid)
                .fetch_all(&self.store.pool)
                .await?;
        for (_, settings) in rows {
            let settings: Value = serde_json::from_str(&settings)?;
            if let Ok(other) = target(&settings, &format.extension)
                && normalized(&other).await? == destination
            {
                return Err(invalid(
                    "Another Sync owns this file; choose a different directory or filename",
                ));
            }
        }
        Ok(())
    }

    pub(super) async fn document_query(
        &self,
        binding: &Binding,
    ) -> Result<protein::Protein, EngineError> {
        let uid = binding
            .protein_uid
            .as_deref()
            .ok_or_else(|| invalid("Choose a saved Protein for export"))?;
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .filter(|row| row.kind == "protein")
            .ok_or_else(|| invalid("Selected Protein is unavailable"))?;
        let ast = store::records::get_extension(&self.store.pool, &record.uid, "lince.protein")
            .await?
            .ok_or_else(|| invalid("Selected Protein has no query"))?;
        let mut query: protein::Protein = serde_json::from_value(ast)?;
        protein::validate(&query)?;
        let format = self
            .document_format(binding)?
            .ok_or_else(|| invalid("Missing document format"))?;
        let expected = match format.source {
            SnapshotSource::Record => protein::Source::Record,
            SnapshotSource::Schedule => protein::Source::Schedule,
        };
        if query.source != expected || query.aggregate.is_some() {
            return Err(invalid(format!(
                "This format requires a {expected:?} Protein without aggregation"
            )));
        }
        if query.include.projection.is_some()
            || query.include.extension.is_some()
            || query.include.relation_context.is_some()
        {
            return Err(invalid(
                "Export uses the selected Records or schedule without additional included data",
            ));
        }
        if query.source == protein::Source::Schedule {
            protein::calendar::context(&query, None)?;
        }
        if query
            .limit
            .is_some_and(|limit| limit > nucleus::plugins::MAX_SNAPSHOT_ITEMS)
        {
            return Err(invalid("Document export supports at most 1000 entries"));
        }
        query.limit = Some(
            query
                .limit
                .unwrap_or(nucleus::plugins::MAX_SNAPSHOT_ITEMS + 1),
        );
        Ok(query)
    }

    async fn document_snapshot(&self, binding: &Binding) -> Result<ExportSnapshot, EngineError> {
        let now = nucleus::execution::now().timestamp_millis();
        let query = self.document_query(binding).await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut rows = Vec::new();
        let window = if query.source == protein::Source::Schedule {
            let context = protein::calendar::context(&query, None)?;
            self.request_projection(context.clone()).await?;
            let selected = protein::execute_for(&self.store, &query, None).await?;
            let status: nucleus::projection::Status = serde_json::from_value(
                selected
                    .iter()
                    .find(|row| row["kind"] == "projection-status")
                    .ok_or_else(|| invalid("Schedule status is unavailable"))?["status"]
                    .clone(),
            )?;
            match status {
                nucleus::projection::Status::Ready { .. } => {}
                nucleus::projection::Status::Updating {} => {
                    return Err(invalid(
                        "Simulation is updating; keeping the last exported calendar. Preview or refresh again shortly",
                    ));
                }
                nucleus::projection::Status::Incomplete { reason, .. } => {
                    return Err(invalid(format!(
                        "Simulation is incomplete ({reason:?}); keeping the last exported calendar"
                    )));
                }
            }
            for mut row in selected {
                if row["kind"] != "schedule-entry" {
                    continue;
                }
                if let Some(offset) = row["origin"]["allocation"]["cursor_offset_ms"].as_i64() {
                    let duration = row["origin"]["allocation"]["duration_ms"]
                        .as_i64()
                        .ok_or_else(|| invalid("Schedule duration is unavailable"))?;
                    row["time"] = json!(
                        nucleus::schedule::TimeRange::at_cursor(now, offset, duration)
                            .ok_or_else(|| invalid("Schedule time overflow"))?
                    );
                }
                let in_window =
                    serde_json::from_value::<nucleus::schedule::TimeRange>(row["time"].clone())
                        .is_ok_and(|time| {
                            time.overlaps(context.window.from_ms, context.window.until_ms)
                        })
                        || row["category"] == "all-day";
                if in_window {
                    let mut entry = serde_json::Map::new();
                    for key in [
                        "uid",
                        "record_uid",
                        "head",
                        "time",
                        "start_date",
                        "due_date",
                        "origin",
                        "category",
                    ] {
                        if let Some(value) = row.get(key) {
                            entry.insert(key.into(), value.clone());
                        }
                    }
                    rows.push(Value::Object(entry));
                }
            }
            Some(context.window)
        } else {
            for row in protein::select_records(&self.store, &query, None).await? {
                rows.push(json!({"uid":row.uid,"head":row.head,"body":row.body}));
            }
            None
        };
        if rows.len() > nucleus::plugins::MAX_SNAPSHOT_ITEMS
            || serde_json::to_vec(&rows)?.len() > nucleus::plugins::MAX_BYTES / 2
        {
            return Err(invalid(
                "Export exceeds its entry or content budget; narrow the Protein",
            ));
        }
        rows.sort_by(|a, b| a["uid"].as_str().cmp(&b["uid"].as_str()));
        Ok(ExportSnapshot {
            scope: cell.uid,
            generated_at_ms: now,
            window,
            rows,
        })
    }

    pub(super) async fn document_preview(&self, binding: &Binding) -> Result<Value, EngineError> {
        self.validate_document_destination(binding).await?;
        let snapshot = self.document_snapshot(binding).await?;
        let plugin = binding
            .provider
            .split_once('/')
            .ok_or_else(|| invalid("Invalid provider"))?
            .0;
        let token = digest(
            &json!({"binding":binding,"query":self.document_query(binding).await?,"rows":selection_rows(&snapshot.rows),"window":snapshot.window,"plugin_digest":self.approved_plugin(plugin).await?}),
        )?;
        Ok(
            json!({"preview":token,"records":snapshot.rows,"count":snapshot.rows.len(),"limit":nucleus::plugins::MAX_SNAPSHOT_ITEMS,"document":true,"window":snapshot.window,"destination":target(&binding.settings,&self.document_format(binding)?.ok_or_else(|| invalid("Missing format"))?.extension)?.to_string_lossy()}),
        )
    }

    pub(super) async fn export_plugin_document(
        &self,
        binding: &Binding,
        cursor: Value,
    ) -> Result<usize, EngineError> {
        self.validate_document_destination(binding).await?;
        let query_before = serde_json::to_value(self.document_query(binding).await?)?;
        let revision = store::projection::revision(&self.store.pool).await?;
        let mut snapshot = self.document_snapshot(binding).await?;
        let input_digest = digest(&json!({"rows":snapshot.rows,"settings":binding.settings}))?;
        if cursor["input_digest"].as_str() == Some(&input_digest) {
            snapshot.generated_at_ms = cursor["generated_at_ms"]
                .as_i64()
                .unwrap_or(snapshot.generated_at_ms);
        }
        let rendered: ExportDocument = serde_json::from_value(
            self.plugin_call(
                &binding.provider,
                "render",
                json!({"settings":binding.settings,"snapshot":snapshot}),
            )
            .await?,
        )?;
        if rendered.content.len() > nucleus::plugins::MAX_BYTES / 2 {
            return Err(invalid("Rendered document exceeds its content budget"));
        }
        self.active_binding(binding).await?;
        if query_before != serde_json::to_value(self.document_query(binding).await?)? {
            return Err(invalid("Protein changed during export; refresh again"));
        }
        if store::projection::revision(&self.store.pool).await? != revision {
            return Err(invalid(
                "Schedule changed during export; keeping the previous file",
            ));
        }
        let format = self
            .document_format(binding)?
            .ok_or_else(|| invalid("Missing document format"))?;
        let destination = target(&binding.settings, &format.extension)?;
        let count = snapshot.rows.len();
        let content = rendered.content;
        tokio::task::spawn_blocking(move || write_document(&destination, content.as_bytes()))
            .await
            .map_err(|error| invalid(error.to_string()))??;
        store::sqlx::query(
            "UPDATE plugin_binding SET cursor=? WHERE uid=? AND revision=? AND enabled=1",
        )
        .bind(
            json!({"input_digest":input_digest,"generated_at_ms":snapshot.generated_at_ms,"successful_at_ms":nucleus::execution::now().timestamp_millis()})
                .to_string(),
        )
        .bind(&binding.uid)
        .bind(binding.revision as i64)
        .execute(&self.store.pool)
        .await?;
        Ok(count)
    }
}

fn write_document(destination: &Path, content: &[u8]) -> Result<(), EngineError> {
    use std::io::{Read, Write};
    let directory = destination
        .parent()
        .ok_or_else(|| invalid("Export directory is missing"))?;
    std::fs::create_dir_all(directory).map_err(|error| invalid(error.to_string()))?;
    let directory = std::fs::canonicalize(directory).map_err(|error| invalid(error.to_string()))?;
    let destination = directory.join(
        destination
            .file_name()
            .ok_or_else(|| invalid("Export filename is missing"))?,
    );
    match std::fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(invalid("Export destination must be a regular file"));
            }
            if metadata.len() == content.len() as u64 {
                let mut existing = Vec::new();
                std::fs::File::open(&destination)
                    .map_err(|error| invalid(error.to_string()))?
                    .take(nucleus::plugins::MAX_BYTES as u64)
                    .read_to_end(&mut existing)
                    .map_err(|error| invalid(error.to_string()))?;
                if existing == content {
                    return Ok(());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(invalid(error.to_string())),
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(&directory).map_err(|error| invalid(error.to_string()))?;
    temporary
        .write_all(content)
        .map_err(|error| invalid(error.to_string()))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| invalid(error.to_string()))?;
    temporary
        .persist(&destination)
        .map_err(|error| invalid(error.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(&directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| invalid(error.to_string()))?;
    Ok(())
}
