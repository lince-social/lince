use crate::{
    Engine, EngineError, actions::ActionOutcome, record_creation::Draft,
    record_policy::ScalarChange,
};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use nucleus::{
    Fact,
    optimization::{Model, Options},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) const RUN: &str = "lince.optimization.run";
pub(crate) const RESULT: &str = "lince.optimization.result";
const MAX_RESULT_BYTES: usize = 6_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Start {
        model: Model,
        #[serde(default)]
        options: Options,
    },
    Poll {
        run: String,
    },
    Cancel {
        run: String,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Start { .. } => "record:create",
            _ => "record:update",
        }
    }
}

#[derive(Default)]
pub(crate) struct Runtime {
    pub gate: tokio::sync::Mutex<()>,
    jobs: std::sync::Mutex<BTreeMap<String, Job>>,
}

struct Job {
    cancelled: Arc<AtomicBool>,
    work: tokio::task::JoinHandle<Result<Value, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Run {
    pub controller: Option<String>,
    pub plan: Option<String>,
    pub kind: String,
    pub hash: String,
    pub state: String,
    pub snapshot: Value,
    pub created_at: String,
    pub options: Options,
    pub message: Option<String>,
}

pub(crate) fn invalid(error: impl ToString) -> EngineError {
    EngineError::Consequence(error.to_string())
}

impl Engine {
    async fn store_optimization_result(
        &self,
        uid: &str,
        result: &Value,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let bytes = serde_json::to_vec(result)?;
        if bytes.len() > MAX_RESULT_BYTES {
            return Err(invalid(
                "Result exceeds its 6 MB limit; select fewer inputs",
            ));
        }
        if bytes.len() <= store::records::MAX_EXTENSION_BYTES {
            return self
                .managed_write(uid, RESULT, result, None, actor, now)
                .await;
        }
        let mut facts = Vec::new();
        let mut count = 0;
        for (index, part) in bytes.chunks(500_000).enumerate() {
            let data = json!({"base64":base64::prelude::BASE64_STANDARD.encode(part)});
            facts.extend(
                self.managed_write(
                    uid,
                    &format!("{RESULT}.part-{index}"),
                    &data,
                    None,
                    actor,
                    now,
                )
                .await?,
            );
            count += 1;
        }
        let manifest = json!({"chunked":true,"parts":count,"hash":nucleus::demeter::fingerprint(result).map_err(invalid)?});
        facts.extend(
            self.managed_write(uid, RESULT, &manifest, None, actor, now)
                .await?,
        );
        Ok(facts)
    }

    async fn read_optimization_result(&self, uid: &str) -> Result<Option<Value>, EngineError> {
        let Some(value) = store::records::get_extension(&self.store.pool, uid, RESULT).await?
        else {
            return Ok(None);
        };
        if value["chunked"] != true {
            return Ok(Some(value));
        }
        let count = value["parts"]
            .as_u64()
            .filter(|n| (1..=12).contains(n))
            .ok_or_else(|| invalid("Invalid result part manifest"))?;
        let mut bytes = Vec::new();
        for index in 0..count {
            let page = store::records::get_extension(
                &self.store.pool,
                uid,
                &format!("{RESULT}.part-{index}"),
            )
            .await?
            .ok_or_else(|| invalid("Result part is missing"))?;
            let encoded = page["base64"]
                .as_str()
                .filter(|s| s.len() <= 666_668)
                .ok_or_else(|| invalid("Result part exceeds its limit"))?;
            let part = base64::prelude::BASE64_STANDARD
                .decode(encoded)
                .map_err(invalid)?;
            if part.len() > 500_000 || bytes.len() + part.len() > MAX_RESULT_BYTES {
                return Err(invalid("Result exceeds its decoded size limit"));
            }
            bytes.extend(part);
        }
        let decoded: Value = serde_json::from_slice(&bytes)?;
        if value["hash"] != nucleus::demeter::fingerprint(&decoded).map_err(invalid)? {
            return Err(invalid("Result integrity check failed"));
        }
        Ok(Some(decoded))
    }
    pub(crate) async fn managed_write(
        &self,
        uid: &str,
        namespace: &str,
        value: &Value,
        expected: Option<&Value>,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        self.change_record_scalar_as(
            uid,
            ScalarChange::Extension {
                namespace,
                value,
                expected,
            },
            actor,
            now,
        )
        .await
    }

    pub(crate) async fn planning_read(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<String, EngineError> {
        let uid = self.resolve(token).await?;
        if !self.may_read_record(actor, &uid).await? {
            return Err(EngineError::Forbidden(
                "Planning input is not visible".into(),
            ));
        }
        Ok(uid)
    }

    pub(crate) async fn load_optimization_run(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<(String, Run), EngineError> {
        let uid = self.planning_read(token, actor).await?;
        let run: Run = serde_json::from_value(
            store::records::get_extension(&self.store.pool, &uid, RUN)
                .await?
                .ok_or_else(|| invalid("Choose a saved optimization run"))?,
        )
        .map_err(invalid)?;
        if actor.is_some() && run.controller.as_deref() != actor {
            return Err(EngineError::Forbidden(
                "Use your own optimization run".into(),
            ));
        }
        Ok((uid, run))
    }

    pub(crate) async fn start_optimization_run<F>(
        &self,
        mut run: Run,
        actor: Option<&str>,
        now: DateTime<Utc>,
        solve: F,
    ) -> Result<ActionOutcome, EngineError>
    where
        F: FnOnce(Arc<AtomicBool>) -> Result<Value, String> + Send + 'static,
    {
        self.require_permission(actor, "record:create").await?;
        self.require_permission(actor, "record:update").await?;
        if run.options.time_limit_ms > 30_000
            || run.options.slice_ms == 0
            || run.options.slice_ms > 200
        {
            return Err(invalid(
                "Use a budget up to 30000 ms and slices of 1–200 ms",
            ));
        }
        if serde_json::to_vec(&run)?.len() > 950_000 {
            return Err(invalid(
                "The frozen run exceeds its storage limit; select fewer inputs",
            ));
        }
        {
            let jobs = self.meal_runtime.jobs.lock().map_err(invalid)?;
            if jobs.len() >= 16 {
                return Err(invalid(
                    "Poll or cancel an existing run before starting another",
                ));
            }
        }
        run.controller = actor.map(str::to_owned);
        let draft = Draft { head: if run.kind == "demeter" { "Demeter food optimization" } else { "Linear optimization" }.into(), body: "Frozen inputs and bounded continuous optimization. Review the result before accepting work.".into(), ..Default::default() };
        let uid = draft.uid.clone();
        let mut outcome = self
            .create_record_draft(draft, actor.map(str::to_owned), now)
            .await?;
        outcome.facts.extend(
            self.managed_write(&uid, RUN, &serde_json::to_value(&run)?, None, actor, now)
                .await?,
        );
        let cancelled = Arc::new(AtomicBool::new(false));
        let control = cancelled.clone();
        let work = tokio::task::spawn_blocking(move || solve(control));
        self.meal_runtime
            .jobs
            .lock()
            .map_err(invalid)?
            .insert(uid.clone(), Job { cancelled, work });
        outcome.data = Some(json!({"run":uid,"state":"running"}));
        Ok(outcome)
    }

    pub(crate) async fn poll_optimization(
        &self,
        token: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
        cancel: bool,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "record:update").await?;
        let (uid, mut run) = self.load_optimization_run(token, actor).await?;
        let before = serde_json::to_value(&run)?;
        let mut outcome = ActionOutcome::default();
        let job = {
            let mut jobs = self.meal_runtime.jobs.lock().map_err(invalid)?;
            if cancel {
                if let Some(job) = jobs.get(&uid) {
                    job.cancelled.store(true, Ordering::Relaxed);
                    run.state = "cancelling".into();
                }
            }
            if jobs.get(&uid).is_some_and(|j| j.work.is_finished()) {
                jobs.remove(&uid)
            } else {
                None
            }
        };
        if let Some(job) = job {
            match job.work.await.map_err(invalid)? {
                Ok(result) => {
                    run.state = result.get("solution").unwrap_or(&result)["status"]
                        .as_str()
                        .unwrap_or("failed")
                        .into();
                    outcome.facts.extend(
                        self.store_optimization_result(&uid, &result, actor, now)
                            .await?,
                    );
                }
                Err(error) => {
                    run.state = "failed".into();
                    run.message = Some(error);
                }
            }
        } else if ["running", "cancelling"].contains(&run.state.as_str())
            && !self
                .meal_runtime
                .jobs
                .lock()
                .map_err(invalid)?
                .contains_key(&uid)
        {
            run.state = "interrupted".into();
            run.message = Some("Runtime restarted; start another bounded run".into());
        }
        if serde_json::to_value(&run)? != before {
            outcome.facts.extend(
                self.managed_write(
                    &uid,
                    RUN,
                    &serde_json::to_value(&run)?,
                    Some(&before),
                    actor,
                    now,
                )
                .await?,
            );
        }
        let result = self.read_optimization_result(&uid).await?;
        outcome.data = Some(
            json!({"run":uid,"state":run.state,"message":run.message,"result":result,"hash":run.hash,"plan":run.plan}),
        );
        Ok(outcome)
    }

    pub(crate) async fn optimization_command(
        &self,
        request: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, request.permission()).await?;
        let _gate = self.meal_runtime.gate.lock().await;
        match request {
            Command::Start { model, options } => {
                model.validate().map_err(invalid)?;
                let hash = nucleus::demeter::fingerprint(&model).map_err(invalid)?;
                let run = Run {
                    controller: actor.map(str::to_owned),
                    plan: None,
                    kind: "linear".into(),
                    hash,
                    state: "running".into(),
                    snapshot: serde_json::to_value(&model)?,
                    created_at: now.to_rfc3339(),
                    options,
                    message: None,
                };
                self.start_optimization_run(run, actor, now, move |cancelled| {
                    serde_json::to_value(nucleus::optimization::solve(&model, options, &cancelled))
                        .map_err(|e| e.to_string())
                })
                .await
            }
            Command::Poll { run } => self.poll_optimization(&run, actor, now, false).await,
            Command::Cancel { run } => self.poll_optimization(&run, actor, now, true).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn large_results_survive_worker_removal_and_reject_corrupt_parts() {
        let engine = Engine::open_memory().await.unwrap();
        store::organs::ensure_local(&engine.store.pool, "http://optimization.test")
            .await
            .unwrap();
        let now = Utc::now();
        let expected = json!({"status":"optimal","payload":"x".repeat(2_000_000)});
        let value = expected.clone();
        let run = Run {
            controller: None,
            plan: None,
            kind: "linear".into(),
            hash: "test".into(),
            state: "running".into(),
            snapshot: json!({}),
            created_at: now.to_rfc3339(),
            options: Options::default(),
            message: None,
        };
        let uid = engine
            .start_optimization_run(run, None, now, move |_| Ok(value))
            .await
            .unwrap()
            .created
            .unwrap();
        for _ in 0..200 {
            let poll = engine
                .poll_optimization(&uid, None, now, false)
                .await
                .unwrap();
            if poll.data.as_ref().unwrap()["state"] == "optimal" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let first = engine
            .poll_optimization(&uid, None, now, false)
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(first["result"], expected);
        assert!(!engine.meal_runtime.jobs.lock().unwrap().contains_key(&uid));
        let manifest = store::records::get_extension(&engine.store.pool, &uid, RESULT)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(manifest["chunked"], true);
        store::records::set_extension(
            &engine.store.pool,
            &uid,
            &format!("{RESULT}.part-0"),
            &json!({"base64":"AAAA"}),
        )
        .await
        .unwrap();
        assert!(
            engine
                .poll_optimization(&uid, None, now, false)
                .await
                .is_err()
        );
    }
}
