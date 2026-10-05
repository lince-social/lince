use nucleus::command::{Command, CommandResponse};
use serde::{Deserialize, Serialize};
use store::sqlx::Row;

use crate::{Engine, EngineError};

mod runner;

impl Engine {
    pub fn set_command_directory(&self, directory: &std::path::Path) -> Result<(), EngineError> {
        let metadata = std::fs::symlink_metadata(directory).map_err(invalid)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("The command directory must be an ordinary directory"));
        }
        *self.command_directory.lock().map_err(invalid)? = Some(directory.canonicalize().map_err(invalid)?);
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct QueryContext {
    pub rule_uid: String,
    pub revision: i64,
    pub event: crate::rule_runtime::RuleEvent,
}

tokio::task_local! {
    pub(crate) static QUERY_CONTEXT: QueryContext;
    pub(crate) static COMMAND_AUTHORING: bool;
    pub(crate) static SAMPLE_ORIGIN: Option<String>;
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_command_invalid",
        message: message.to_string(),
    }
}

fn numeric_stdout(stdout: &str) -> Result<nucleus::DecimalValue, String> {
    let text = stdout.trim();
    let unsigned = text
        .strip_prefix('-')
        .or_else(|| text.strip_prefix('+'))
        .unwrap_or(text);
    let (integer, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|fraction| {
            fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err("Command output must contain exactly one decimal number".into());
    }
    let integer = integer.trim_start_matches('0');
    let canonical = format!(
        "{}{}{}",
        if text.starts_with('-') { "-" } else { "" },
        if integer.is_empty() { "0" } else { integer },
        fraction.map_or_else(String::new, |fraction| format!(".{fraction}"))
    );
    nucleus::DecimalValue::parse_inferred(&canonical)
        .map_err(|error| format!("Command output is not one representable exact number: {error}"))
}

#[derive(Clone)]
struct Definition {
    uid: String,
    revision: i64,
    configuration: Command,
    host: Option<String>,
}

impl Definition {
    fn matches(&self, row: &store::sqlx::sqlite::SqliteRow) -> Result<bool, EngineError> {
        Ok(
            Some(self.revision) == row.get::<Option<i64>, _>("command_revision")
                && self.host == row.get::<Option<String>, _>("host_uid")
                && self.configuration
                    == serde_json::from_str::<Command>(&row.get::<String, _>("configuration"))?,
        )
    }
}

impl Engine {
    pub fn supply_command_responses(
        &self,
        responses: Vec<CommandResponse>,
    ) -> Result<(), EngineError> {
        if responses.len() > 1024
            || responses.iter().any(|response| {
                response.command.len() > 256
                    || response.stdout.len() > 65_536
                    || response.stderr.len() > 65_536
            })
        {
            return Err(invalid(
                "Command responses exceed the Simulation input limits",
            ));
        }
        *self
            .command_responses
            .write()
            .map_err(|_| invalid("Command responses are unavailable"))? = Some(responses);
        Ok(())
    }

    async fn command_definition(&self, name: &str) -> Result<Definition, EngineError> {
        let uid = self.resolve(name).await?;
        let record = store::records::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Choose a saved command Signal"))?;
        if record.kind != "signal" {
            return Err(invalid("Choose a saved command Signal"));
        }
        if let Some(definition) =
            store::records::get_extension(&self.store.pool, &uid, "lince.command").await?
        {
            let configuration: Command =
                serde_json::from_value(definition["configuration"].clone())?;
            configuration.validate().map_err(invalid)?;
            return Ok(Definition {
                uid,
                revision: definition["revision"]
                    .as_i64()
                    .ok_or_else(|| invalid("Invalid command revision"))?,
                configuration,
                host: definition["host"].as_str().map(str::to_owned),
            });
        }
        let row = store::sqlx::query(
            "SELECT source FROM signal WHERE record_uid = ? AND source_kind = 'command'",
        )
        .bind(&uid)
        .fetch_optional(&self.store.pool)
        .await?
        .ok_or_else(|| invalid("Command configuration is unavailable"))?;
        let configuration = Command::Shell {
            script: row.get("source"),
        };
        configuration.validate().map_err(invalid)?;
        Ok(Definition {
            uid,
            revision: 1,
            configuration,
            host: store::cells::local(&self.store.pool)
                .await?
                .map(|cell| cell.uid),
        })
    }

    pub(crate) async fn save_command(
        &self,
        target: Option<String>,
        expected_revision: Option<i64>,
        slug: String,
        head: String,
        configuration: Command,
        host: Option<String>,
        actor: Option<&str>,
    ) -> Result<String, EngineError> {
        let _guard = self.command_authoring.lock().await;
        configuration.validate().map_err(invalid)?;
        self.require_permission(actor, "organ:update").await?;
        if !nucleus::valid_slug(&slug) || slug.len() > 256 || head.len() > 256 {
            return Err(invalid("Choose a valid command slug and name"));
        }
        let editing = target.is_some();
        let local = store::cells::local(&self.store.pool).await?;
        let host = host.or_else(|| local.as_ref().map(|cell| cell.uid.clone()));
        if let Some(host) = &host
            && local.as_ref().is_none_or(|cell| &cell.uid != host)
        {
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ for this execution host"))?;
            let roster = self
                .roster_of(&organ.uid)
                .await?
                .ok_or_else(|| invalid("Execution host needs a signed Organ roster"))?;
            if !crate::roster::roster_signature_is_valid(&roster)
                || !roster
                    .roster
                    .cells
                    .iter()
                    .any(|cell| &cell.cell_uid == host)
            {
                return Err(invalid("Choose an enrolled execution Cell"));
            }
        }
        let uid = match target {
            Some(target) => {
                let definition = self.command_definition(&target).await?;
                self.refuse_unreadable_karma_inputs(actor, &[definition.uid.clone()])
                    .await?;
                if expected_revision != Some(definition.revision) {
                    return Err(invalid("Command changed; refresh before saving"));
                }
                definition.uid
            }
            None => {
                self.require_permission(actor, "record:create").await?;
                if !nucleus::valid_slug(&slug) || slug.len() > 256 || head.len() > 256 {
                    return Err(invalid("Choose a valid command slug and name"));
                }
                Box::pin(self.act(
                    crate::actions::Action::CreateRecord {
                        slug: Some(slug.clone()),
                        kind: nucleus::RecordKind::Signal,
                        head: head.clone(),
                        body: String::new(),
                        quantity: 0.0,
                    },
                    actor.map(str::to_owned),
                ))
                .await?
                .created
                .ok_or_else(|| invalid("Command creation returned no Record"))?
            }
        };
        let current = store::records::get_extension(&self.store.pool, &uid, "lince.command")
            .await?
            .and_then(|value| value["revision"].as_i64());
        if expected_revision.is_some() && expected_revision != Some(current.unwrap_or(1)) {
            return Err(invalid("Command changed; refresh before saving"));
        }
        if editing {
            let record = store::records::get(&self.store.pool, &uid)
                .await?
                .ok_or_else(|| invalid("Command was deleted"))?;
            if record.slug.as_deref() != Some(&slug) {
                Box::pin(self.act(
                    crate::actions::Action::SetSlug {
                        target: uid.clone(),
                        slug: Some(slug),
                    },
                    actor.map(str::to_owned),
                ))
                .await?;
            }
            if record.head != head {
                Box::pin(self.act(
                    crate::actions::Action::EditRecordText {
                        target: uid.clone(),
                        head: Some(head),
                        body: None,
                    },
                    actor.map(str::to_owned),
                ))
                .await?;
            }
        }
        COMMAND_AUTHORING.scope(true, Box::pin(self.act(crate::actions::Action::SetExtension { target: uid.clone(), namespace: "lince.command".into(), fds: serde_json::json!({"revision":current.map_or(1, |revision| revision + 1),"configuration":configuration,"host":host}) }, actor.map(str::to_owned)))).await?;
        store::sqlx::query("UPDATE signal SET actor_uid = ? WHERE record_uid = ?")
            .bind(actor)
            .bind(&uid)
            .execute(&self.store.pool)
            .await?;
        self.notify_karma_deadline_change();
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(uid)
    }

    pub(crate) async fn command_snapshot(
        &self,
        name: &str,
        actor: Option<&str>,
    ) -> Result<serde_json::Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        let definition = self.command_definition(name).await?;
        self.refuse_unreadable_karma_inputs(actor, &[definition.uid.clone()])
            .await?;
        Ok(
            serde_json::json!({"uid":definition.uid,"revision":definition.revision,"configuration":definition.configuration,"host":definition.host}),
        )
    }

    pub(crate) async fn recover_commands(&self) -> Result<(), EngineError> {
        store::sqlx::query("UPDATE karma_command_invocation SET status = 'indeterminate', error = 'Worker stopped after starting this command; inspect before retrying' WHERE status = 'running'").execute(&self.store.pool).await?;
        let contexts: Vec<String> = store::sqlx::query_scalar("SELECT context FROM karma_command_invocation WHERE context IS NOT NULL AND status IN ('completed','failed','indeterminate') ORDER BY created_at").fetch_all(&self.store.pool).await?;
        for context in contexts {
            self.resume_command_query(&serde_json::json!({"query_context":serde_json::from_str::<serde_json::Value>(&context)?})).await?;
        }
        store::sqlx::query("UPDATE effect_queue SET status = 'queued' WHERE status = 'uncertain' AND json_extract(payload, '$.request_id') IN (SELECT uid FROM karma_command_invocation WHERE status = 'queued')").execute(&self.store.pool).await?;
        Ok(())
    }

    async fn queue_command(
        &self,
        uid: &str,
        definition: &Definition,
        actor: Option<&str>,
        numeric: bool,
        context: Option<&QueryContext>,
    ) -> Result<(), EngineError> {
        self.require_permission(actor, "organ:update").await?;
        self.refuse_unreadable_karma_inputs(actor, &[definition.uid.clone()])
            .await?;
        let now = nucleus::execution::now();
        let mut tx = store::write_tx(&self.store.pool).await?;
        store::sqlx::query("INSERT OR IGNORE INTO karma_command_invocation(uid, command_uid, command_revision, configuration, host_uid, actor_uid, numeric, context, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'queued', ?)")
            .bind(uid).bind(&definition.uid).bind(definition.revision).bind(serde_json::to_string(&definition.configuration)?).bind(&definition.host).bind(actor).bind(numeric).bind(context.map(serde_json::to_string).transpose()?).bind(now.to_rfc3339()).execute(&mut *tx).await?;
        let payload = serde_json::json!({ "request_id": uid, "actor": actor, "saved_command": definition.uid, "query_context": context, "rule": context.map(|context| &context.rule_uid), "revision": context.map(|context| context.revision) });
        if nucleus::execution::current().is_none() || context.is_none() {
            crate::rule_runtime::queue_effect_tx(
                &mut tx,
                uid,
                "saved-command",
                payload,
                &definition.uid,
                now,
            )
            .await?;
        }
        tx.commit().await?;
        self.effects_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(())
    }

    pub(crate) async fn run_saved_command(
        &self,
        name: &str,
        request: &str,
        numeric: bool,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if request.is_empty() || request.len() > 256 {
            return Err(invalid("Use a command request identity of 1–256 bytes"));
        }
        self.require_karma_execution(None).await?;
        let definition = self.command_definition(name).await?;
        let existing = store::sqlx::query(
            "SELECT command_uid, actor_uid, numeric FROM karma_command_invocation WHERE uid = ?",
        )
        .bind(request)
        .fetch_optional(&self.store.pool)
        .await?;
        if existing.is_some_and(|row| {
            row.get::<Option<String>, _>("command_uid").as_deref() != Some(&definition.uid)
                || row.get::<Option<String>, _>("actor_uid").as_deref() != actor
                || row.get::<bool, _>("numeric") != numeric
        }) {
            return Err(invalid(
                "Request identity belongs to a different invocation",
            ));
        }
        self.queue_command(request, &definition, actor, numeric, None)
            .await
    }

    pub(crate) async fn query_command_reading(
        &self,
        name: &str,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let context = QUERY_CONTEXT.try_with(Clone::clone).map_err(|_| {
            invalid("Fresh commands run only during Rule execution; use Run to test a command")
        })?;
        let rule = store::recurrence::get(&self.store.pool, &context.rule_uid)
            .await?
            .ok_or_else(|| invalid("Rule is unavailable"))?;
        let definition = self.command_definition(name).await?;
        let request = format!(
            "query:{}:{}:{}:{}:{}",
            context.rule_uid,
            context.revision,
            context.event.id,
            context.event.attempt,
            definition.uid
        );
        let row = store::sqlx::query("SELECT status, value, error, command_revision, configuration, host_uid FROM karma_command_invocation WHERE uid = ?").bind(&request).fetch_optional(&self.store.pool).await?;
        if let Some(row) = row {
            if !definition.matches(&row)? {
                return Err(invalid("Command changed while the Rule was waiting"));
            }
            match row.get::<String, _>("status").as_str() {
                "completed" => {
                    return nucleus::DecimalValue::parse_inferred(&row.get::<String, _>("value"))
                        .map_err(invalid);
                }
                "failed" | "indeterminate" => {
                    return Err(invalid(
                        row.get::<Option<String>, _>("error")
                            .unwrap_or_else(|| "Command failed".into()),
                    ));
                }
                _ => {}
            }
        } else if nucleus::execution::current().is_some() {
            let response = self.controlled_command_response(&definition.uid, name)?;
            let Some(response) = response else {
                return Err(EngineError::Conflict {
                    code: "karma_command_response_missing",
                    message: format!("Simulation has no controlled response for @{name}"),
                });
            };
            self.queue_command(
                &request,
                &definition,
                rule.actor_uid.as_deref(),
                true,
                Some(&context),
            )
            .await?;
            let (ok, result) = self
                .record_command_response(&request, response, Some(&definition.uid), true)
                .await?;
            return if ok {
                numeric_stdout(&result).map_err(invalid)
            } else {
                Err(invalid(result))
            };
        } else {
            self.queue_command(
                &request,
                &definition,
                rule.actor_uid.as_deref(),
                true,
                Some(&context),
            )
            .await?;
        }
        Err(EngineError::Conflict {
            code: "karma_query_pending",
            message: "Waiting for a command result".into(),
        })
    }

    pub fn append_command_response(&self, response: CommandResponse) -> Result<(), EngineError> {
        if response.command.len() > 256
            || response.stdout.len() > 65_536
            || response.stderr.len() > 65_536
        {
            return Err(invalid(
                "Command response exceeds the Simulation input limits",
            ));
        }
        let mut responses = self
            .command_responses
            .write()
            .map_err(|_| invalid("Command responses are unavailable"))?;
        let responses = responses.get_or_insert_with(Vec::new);
        if responses.len() >= 1024 {
            return Err(invalid("Too many controlled command responses"));
        }
        responses.push(response);
        Ok(())
    }

    fn controlled_command_response(
        &self,
        uid: &str,
        name: &str,
    ) -> Result<Option<CommandResponse>, EngineError> {
        let mut responses = self
            .command_responses
            .write()
            .map_err(|_| invalid("Command responses are unavailable"))?;
        let Some(responses) = responses.as_mut() else {
            return Ok(None);
        };
        Ok(responses
            .iter()
            .position(|response| {
                response.command == uid
                    || response.command.trim_start_matches('@') == name.trim_start_matches('@')
            })
            .map(|index| responses.remove(index)))
    }

    pub(crate) async fn saved_command_reading(
        &self,
        name: &str,
    ) -> Result<nucleus::DecimalValue, EngineError> {
        let uid = self.resolve(name).await?;
        let value: Option<String> = store::sqlx::query_scalar("WITH samples AS (SELECT f.record_uid, f.rowid AS position, f.at AS at, COALESCE(json_extract(o.payload, '$.cause.kind'), f.cause_kind) AS cause_kind, COALESCE(json_extract(o.payload, '$.cause.uid'), f.cause_uid) AS cause_uid, CASE WHEN o.fact_uid IS NULL THEN f.payload ELSE json_extract(o.payload, '$.payload') END AS payload FROM fact f LEFT JOIN fact_origin o ON o.fact_uid = f.uid WHERE f.record_uid = ?) SELECT json_extract(payload, '$.sample') FROM samples WHERE cause_kind = 'signal' AND cause_uid = record_uid AND json_valid(payload) AND json_type(payload, '$.command_invocation') = 'text' AND json_type(payload, '$.sample') = 'text' ORDER BY at DESC, position DESC LIMIT 1").bind(&uid).fetch_optional(&self.store.pool).await?;
        nucleus::DecimalValue::parse_inferred(
            &value.ok_or_else(|| invalid("Signal has no successful numeric sample"))?,
        )
        .map_err(invalid)
    }

    pub(crate) async fn execute_command_effect(
        &self,
        request: &str,
        payload: &serde_json::Value,
    ) -> Result<(bool, String), EngineError> {
        let actor = payload["actor"].as_str();
        self.require_permission(actor, "organ:update").await?;
        self.require_karma_execution(None).await?;
        let mut row = store::sqlx::query("SELECT * FROM karma_command_invocation WHERE uid = ?")
            .bind(request)
            .fetch_optional(&self.store.pool)
            .await?;
        if row.is_none() {
            let configuration = Command::Shell {
                script: payload["command"].as_str().unwrap_or_default().into(),
            };
            let signal = payload["signal"]
                .as_str()
                .or_else(|| payload["saved_command"].as_str());
            let definition = signal.map(|signal| self.command_definition(signal));
            let definition = match payload
                .get("command_snapshot")
                .filter(|snapshot| !snapshot.is_null())
            {
                Some(snapshot) => Some(Definition {
                    uid: snapshot["uid"]
                        .as_str()
                        .ok_or_else(|| invalid("Missing command identity"))?
                        .into(),
                    revision: snapshot["revision"]
                        .as_i64()
                        .ok_or_else(|| invalid("Missing command revision"))?,
                    configuration: serde_json::from_value(snapshot["configuration"].clone())?,
                    host: snapshot["host"].as_str().map(str::to_owned),
                }),
                None => match definition {
                    Some(future) => Some(future.await?),
                    None => None,
                },
            };
            let configuration = definition
                .as_ref()
                .map_or(configuration, |definition| definition.configuration.clone());
            configuration.validate().map_err(invalid)?;
            let host = definition
                .as_ref()
                .and_then(|definition| definition.host.clone());
            store::sqlx::query("INSERT OR IGNORE INTO karma_command_invocation(uid, command_uid, command_revision, configuration, host_uid, actor_uid, numeric, status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'queued', ?)")
                .bind(request).bind(signal).bind(definition.as_ref().map(|definition| definition.revision)).bind(serde_json::to_string(&configuration)?).bind(host).bind(actor).bind(payload["signal"].is_string()).bind(nucleus::execution::now().to_rfc3339()).execute(&self.store.pool).await?;
            row = store::sqlx::query("SELECT * FROM karma_command_invocation WHERE uid = ?")
                .bind(request)
                .fetch_optional(&self.store.pool)
                .await?;
        }
        let row = row.ok_or_else(|| invalid("Command invocation is missing"))?;
        let command_uid: Option<String> = row.get("command_uid");
        let numeric: bool = row.get("numeric");
        if row.get::<String, _>("status") != "queued" {
            return Ok((
                row.get::<String, _>("status") == "completed",
                row.get::<Option<String>, _>("error")
                    .or_else(|| row.get("stdout"))
                    .unwrap_or_default(),
            ));
        }
        let _capacity = if nucleus::execution::current().is_none() {
            Some(self.command_capacity.acquire().await.map_err(invalid)?)
        } else {
            None
        };
        self.require_permission(actor, "organ:update").await?;
        self.require_karma_execution(None).await?;
        if let Some(uid) = payload["rule"].as_str() {
            let rule = store::recurrence::get(&self.store.pool, uid)
                .await?
                .filter(|rule| {
                    !rule.is_paused() && Some(rule.revision) == payload["revision"].as_i64()
                })
                .ok_or_else(|| invalid("Rule changed before command dispatch"))?;
            self.require_karma_execution(Some(&rule.record_uid)).await?;
        }
        if let Some(uid) = &command_uid {
            self.refuse_unreadable_karma_inputs(actor, &[uid.clone()])
                .await?;
            let current = self.command_definition(uid).await?;
            if !current.matches(&row)? {
                return Err(invalid("Command was revised before dispatch"));
            }
        }
        if let Some(host) = row.get::<Option<String>, _>("host_uid")
            && store::cells::local(&self.store.pool)
                .await?
                .is_none_or(|cell| cell.uid != host)
        {
            return Err(invalid("This Cell is not the command execution host"));
        }
        let changed = store::sqlx::query("UPDATE karma_command_invocation SET status = 'running' WHERE uid = ? AND status = 'queued'").bind(request).execute(&self.store.pool).await?;
        if changed.rows_affected() == 0 {
            return Ok((false, "Command already claimed".into()));
        }
        let response = if nucleus::execution::current().is_some() {
            self.controlled_command_response(
                command_uid.as_deref().unwrap_or(request),
                command_uid.as_deref().unwrap_or(request),
            )?
            .ok_or_else(|| EngineError::Conflict {
                code: "karma_command_response_missing",
                message: "Simulation has no controlled command response".into(),
            })?
        } else {
            let directory = self.command_directory.lock().map_err(invalid)?.clone();
            runner::run(&serde_json::from_str::<Command>(
                &row.get::<String, _>("configuration"),
            )?, directory.as_deref())
            .await
        };
        self.access_scope(true, async {
            self.require_permission(actor, "organ:update").await?;
            self.require_karma_execution(None).await?;
            if let Some(uid) = &command_uid {
                self.refuse_unreadable_karma_inputs(actor, &[uid.clone()])
                    .await?;
                if !self.command_definition(uid).await?.matches(&row)? {
                    return Err(invalid("Command changed during execution"));
                }
            }
            if let Some(uid) = payload["rule"].as_str() {
                let rule = store::recurrence::get(&self.store.pool, uid)
                    .await?
                    .filter(|rule| {
                        !rule.is_paused() && Some(rule.revision) == payload["revision"].as_i64()
                    })
                    .ok_or_else(|| invalid("Rule changed during command execution"))?;
                self.require_karma_execution(Some(&rule.record_uid)).await?;
            }
            self.record_command_response(request, response, command_uid.as_deref(), numeric)
                .await
        })
        .await
    }

    fn record_command_response<'a>(
        &'a self,
        request: &'a str,
        response: CommandResponse,
        command: Option<&'a str>,
        numeric: bool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(bool, String), EngineError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let parsed = if response.ok && numeric {
                Some(numeric_stdout(&response.stdout))
            } else {
                None
            };
            let error = if !response.ok {
                Some(if response.stderr.is_empty() {
                    "Command failed".into()
                } else {
                    response.stderr.clone()
                })
            } else {
                parsed
                    .as_ref()
                    .and_then(|parsed| parsed.as_ref().err().cloned())
            };
            let value = parsed.and_then(Result::ok);
            let now = nucleus::execution::now();
            let mut tx = store::write_tx(&self.store.pool).await?;
            store::sqlx::query("UPDATE karma_command_invocation SET status = ?, stdout = ?, stderr = ?, value = ?, error = ?, finished_at = ? WHERE uid = ? AND status IN ('queued', 'running')")
            .bind(if error.is_some() { "failed" } else { "completed" }).bind(&response.stdout).bind(&response.stderr).bind(value.map(|value| value.to_string())).bind(&error).bind(now.to_rfc3339()).bind(request).execute(&mut *tx).await?;
            if let (Some(command), Some(value)) = (command, value) {
                let level = store::exact::read_decimal(&store::sqlx::query("SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ? AND deleted_at IS NULL").bind(command).fetch_one(&mut *tx).await?, "quantity")?;
                let mut fact = nucleus::NewFact::quantity(
                    command,
                    store::exact::difference(value, level)?,
                    nucleus::Cause::signal(command),
                );
                fact.payload = Some(
                    serde_json::json!({"command_invocation": request, "sample": value.to_string()})
                        .to_string(),
                );
                let signer = self.signer.lock().await.clone();
                let fact =
                    crate::append::append_one_in_transaction(&mut tx, fact, now, signer.as_ref())
                        .await?;
                store::sqlx::query("INSERT INTO karma_signal_sample(signal_uid, invocation_uid, value, sampled_at) VALUES (?, ?, ?, ?) ON CONFLICT(signal_uid) DO UPDATE SET invocation_uid = excluded.invocation_uid, value = excluded.value, sampled_at = excluded.sampled_at")
                .bind(command).bind(request).bind(value.to_string()).bind(now.to_rfc3339()).execute(&mut *tx).await?;
                store::sqlx::query("UPDATE signal SET last_sampled_at = ? WHERE record_uid = ?")
                    .bind(now.to_rfc3339())
                    .bind(command)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                if let Some(fact) = fact {
                    let _ = self.bus.send(fact.clone());
                    {
                        let context: Option<String> = store::sqlx::query_scalar(
                            "SELECT context FROM karma_command_invocation WHERE uid = ?",
                        )
                        .bind(request)
                        .fetch_one(&self.store.pool)
                        .await?;
                        let origin = context
                            .and_then(|context| serde_json::from_str::<QueryContext>(&context).ok())
                            .map(|context| context.rule_uid);
                        if QUERY_CONTEXT.try_with(|_| ()).is_ok()
                            && nucleus::execution::current().is_some()
                        {
                            SAMPLE_ORIGIN
                                .scope(
                                    origin,
                                    Box::pin(self.run_rule_reactions(
                                        vec![command.into()],
                                        fact.uid,
                                        now,
                                    )),
                                )
                                .await?;
                        } else {
                            SAMPLE_ORIGIN
                                .scope(
                                    origin,
                                    Box::pin(self.react_to_event(
                                        vec![command.into()],
                                        fact.uid,
                                        now,
                                    )),
                                )
                                .await?;
                        }
                    }
                }
            } else {
                tx.commit().await?;
            }
            self.query_changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            Ok((error.is_none(), error.unwrap_or(response.stdout)))
        })
    }

    pub(crate) async fn fail_command(&self, request: &str, error: &str) -> Result<(), EngineError> {
        store::sqlx::query("UPDATE karma_command_invocation SET status = 'failed', error = ?, finished_at = ? WHERE uid = ? AND status IN ('queued', 'running')").bind(error).bind(nucleus::execution::now().to_rfc3339()).bind(request).execute(&self.store.pool).await?;
        Ok(())
    }

    pub(crate) async fn resume_command_query(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), EngineError> {
        let Some(context) = payload
            .get("query_context")
            .filter(|context| !context.is_null())
        else {
            return Ok(());
        };
        let context: QueryContext = serde_json::from_value(context.clone())?;
        let Some(rule) = store::recurrence::get(&self.store.pool, &context.rule_uid)
            .await?
            .filter(|rule| rule.revision == context.revision && !rule.is_paused())
        else {
            return Ok(());
        };
        let now = nucleus::execution::now();
        let facts = self
            .access_scope(true, async {
                let _guard = self.rule_execution.lock().await;
                match Box::pin(self.execute_rule_event(&rule, &context.event, now)).await {
                    Ok(facts) => Ok(facts),
                    Err(error) => {
                        self.record_rule_failure(&rule, &context.event, &error, now)
                            .await?;
                        Ok(Vec::new())
                    }
                }
            })
            .await?;
        for fact in facts {
            let _ = self.bus.send(fact.clone());
            Box::pin(self.react_to_event(vec![fact.record_uid], fact.uid, now)).await?;
        }
        Ok(())
    }

    pub(crate) async fn inspect_commands(
        &self,
        actor: Option<&str>,
    ) -> Result<serde_json::Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        let mut rows = Vec::new();
        for signal in store::sqlx::query_scalar::<_, String>(
            "SELECT uid FROM record WHERE kind = 'signal' AND deleted_at IS NULL",
        )
        .fetch_all(&self.store.pool)
        .await?
        {
            if self
                .refuse_unreadable_karma_inputs(actor, &[signal.clone()])
                .await
                .is_err()
            {
                continue;
            }
            let Ok(definition) = self.command_definition(&signal).await else {
                continue;
            };
            let record = store::records::get(&self.store.pool, &signal)
                .await?
                .ok_or_else(|| invalid("Command deleted"))?;
            let history = store::sqlx::query("SELECT uid, status, stdout, stderr, value, error, context, created_at, finished_at FROM karma_command_invocation WHERE command_uid = ? ORDER BY created_at DESC, uid DESC LIMIT 20").bind(&signal).fetch_all(&self.store.pool).await?;
            let history: Vec<_> = history.iter().map(|row| serde_json::json!({"request":row.get::<String,_>("uid"),"status":row.get::<String,_>("status"),"stdout":row.get::<Option<String>,_>("stdout"),"stderr":row.get::<Option<String>,_>("stderr"),"value":row.get::<Option<String>,_>("value"),"error":row.get::<Option<String>,_>("error"),"context":row.get::<Option<String>,_>("context"),"at":row.get::<String,_>("created_at"),"finished_at":row.get::<Option<String>,_>("finished_at")})).collect();
            let sample = self
                .saved_command_reading(&signal)
                .await
                .ok()
                .map(|value| value.to_string());
            rows.push(serde_json::json!({"uid":definition.uid,"slug":record.slug,"head":record.head,"revision":definition.revision,"configuration":definition.configuration,"host":definition.host,"sample":sample,"history":history}));
        }
        Ok(serde_json::json!({"commands":rows}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_stdout_is_one_bounded_decimal() {
        for text in [
            "",
            "1 2",
            "1 apples",
            "{}",
            "NaN",
            "inf",
            "--0",
            "1.2.3",
            "1e3",
            "1.",
            "170141183460469231731687303715884105728",
        ] {
            assert!(numeric_stdout(text).is_err(), "{text}");
        }
        assert_eq!(numeric_stdout("  -002.50\n").unwrap().to_string(), "-2.50");
        assert_eq!(numeric_stdout("+0").unwrap().to_string(), "0");
    }
}
