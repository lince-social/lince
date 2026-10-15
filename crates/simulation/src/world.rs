use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use cell::Cell;
use engine::{Engine, EngineError};
use nucleus::execution::Execution;
use nucleus::karma::{CanonicalHash, ReferenceKind, TypedUid, canonical_hash};
use nucleus::simulation::{self as report, Cause, Observation, Quantity, Refusal};
use serde::{Deserialize, Serialize};
use store::sqlx::Row;

use crate::Result;
use crate::scenario::{Event, Input, Invocation, Scenario};

pub struct Node {
    pub cell: Cell,
    pub execution: Execution,
    pub next_ms: Option<i64>,
    pub offset_ms: i64,
    path: PathBuf,
    state_hasher: store::snapshot::StateHasher,
    secret: [u8; 32],
    fact_position: i64,
    application_position: i64,
    quantities: BTreeMap<String, Quantity>,
    loan_quantities: BTreeMap<String, Quantity>,
    loan_unit_changes: std::collections::BTreeSet<String>,
    deleted_records: BTreeSet<String>,
    pub(crate) person_key: Option<engine::trust::Signer>,
    pub online: bool,
}

impl Node {
    pub fn engine(&self) -> &Engine {
        &self.cell.runtime().engine
    }

    pub(crate) fn set_time(&self, now_ms: i64) -> Result<()> {
        self.execution
            .set_time(now_ms.checked_add(self.offset_ms).ok_or("clock overflow")?)?;
        Ok(())
    }
}

#[derive(Clone, Serialize)]
struct Message {
    id: u64,
    from: String,
    to: String,
    payload: Payload,
}

#[derive(Clone, Serialize)]
pub(crate) enum Payload {
    Sync(engine::sync::OpBatch),
    Pull {
        uid: String,
        reference: String,
        after_cursor: u64,
        wire: cell::transfer::Authenticated<cell::transfer::PullRequest>,
    },
    PullResult {
        uid: String,
        reference: String,
        after_cursor: u64,
        wire: cell::transfer::Authenticated<cell::transfer::PullResult>,
    },
    Transfer(cell::transfer::Authenticated<cell::transfer::DeliveryPush>),
    Receipt(cell::transfer::Authenticated<nucleus::transfer_delivery::TransferPackageReceiptV1>),
    Command {
        input: String,
        wire: cell::transfer::Authenticated<nucleus::transfer_delivery::TransferRemoteCommandV1>,
    },
    CommandResult {
        input: String,
        wire: cell::transfer::Authenticated<cell::transfer::CommandResult>,
    },
    Attestation(cell::transfer::Authenticated<cell::transfer::ApplicationAttestationRequest>),
    AttestationResult(cell::transfer::Authenticated<cell::transfer::ApplicationAttestationResult>),
}

#[derive(Clone, Serialize)]
enum Task {
    Input(Input),
    Deliver(Message),
    Check,
}

pub struct World {
    pub scenario: Scenario,
    pub nodes: BTreeMap<String, Node>,
    pub now_ms: i64,
    pub trace: Vec<report::Event>,
    pub captured: BTreeMap<String, String>,
    pub steps: u64,
    pub inputs: u64,
    pub evidence_bytes: u64,
    pub(crate) source_history: BTreeSet<String>,
    pub evidence_micros: u64,
    pub stop: Option<report::Stop>,
    pub control: nucleus::execution::control::Control,
    pub(crate) assumed_transfers: crate::assumptions::Transfers,
    tasks: BTreeMap<(i64, u8, u64), Task>,
    partitions: BTreeSet<(String, String)>,
    serial: u64,
    journal: std::fs::File,
}

struct Observations<'a> {
    values: Vec<Observation>,
    bytes: u64,
    limit: u64,
    row_limit: u64,
    cell: &'a str,
    cause: &'a Cause,
    now_ms: i64,
    sequence: u64,
}

impl Observations<'_> {
    fn push(&mut self, observation: Observation) -> Result<()> {
        #[derive(Serialize)]
        struct Event<'a> {
            sequence: u64,
            virtual_ms: i64,
            cell: &'a str,
            caused_by: &'a Cause,
            observation: &'a Observation,
        }
        if self.values.len() as u64 >= self.row_limit {
            return Err(Box::new(crate::limits::Exceeded {
                resource: "observation rows",
                limit: self.limit,
            }));
        }
        let event = Event {
            sequence: self.sequence + self.values.len() as u64,
            virtual_ms: self.now_ms,
            cell: self.cell,
            caused_by: self.cause,
            observation: &observation,
        };
        let bytes = crate::limits::size(
            &event,
            self.limit
                .saturating_sub(self.bytes)
                .saturating_sub(1)
                .min(crate::limits::JSON_BYTES - 1),
            "observation evidence",
        )? + 1;
        self.bytes += bytes;
        self.values.push(observation);
        Ok(())
    }
}

async fn source_history(
    store: &store::Store,
    maximum: i64,
    from_ms: i64,
    until_ms: i64,
) -> Result<bool> {
    let mut position = 0i64;
    loop {
        let rows = store::sqlx::query("SELECT rowid AS position, substr(intended_at,1,128) AS intended_at, length(CAST(intended_at AS BLOB)) AS bytes FROM karma_rule_application WHERE rowid > ? AND rowid <= ? AND status = 'applied' AND unixepoch(intended_at, 'subsec') * 1000 BETWEEN ? AND ? ORDER BY rowid LIMIT ?")
            .bind(position).bind(maximum).bind(from_ms.saturating_sub(1)).bind(until_ms.saturating_add(1)).bind(crate::limits::BATCH_ROWS as i64).fetch_all(&store.pool).await?;
        if rows.is_empty() {
            return Ok(false);
        }
        for row in rows {
            position = row.try_get("position")?;
            if row.try_get::<i64, _>("bytes")? > 128 {
                return Err("source application timestamp exceeds its limit".into());
            }
            let intended: String = row.try_get("intended_at")?;
            let at = chrono::DateTime::parse_from_rfc3339(&intended)?.timestamp_millis();
            if (from_ms..=until_ms).contains(&at) {
                return Ok(true);
            }
        }
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    #[test]
    fn canonical_state_hash_preserves_large_runtime_values() {
        let value = "x".repeat((crate::limits::JSON_BYTES + 1) as usize);
        let expected = nucleus::fact::sha256_hex(&serde_json::to_vec(&value).unwrap());
        assert_eq!(
            digest(&value).unwrap().as_str(),
            format!("sha256:{expected}")
        );
    }

    #[tokio::test]
    async fn source_history_ignores_old_rows_and_uses_scheduler_milliseconds() {
        let directory = tempfile::tempdir().unwrap();
        let store = store::Store::open(&format!(
            "sqlite://{}",
            directory.path().join("source.sqlite").display()
        ))
        .await
        .unwrap();
        for index in 0..600 {
            store::sqlx::query("INSERT INTO karma_rule_application(event_id,rule_uid,rule_revision,status,at,intended_at) VALUES(?,'rule',1,'applied',?,?)").bind(format!("old-{index}")).bind("2000-01-01T00:00:00Z").bind("2000-01-01T00:00:00Z").execute(&store.pool).await.unwrap();
        }
        let at = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert!(!source_history(&store, 600, at, at).await.unwrap());
        store::sqlx::query("INSERT INTO karma_rule_application(event_id,rule_uid,rule_revision,status,at,intended_at) VALUES('edge','rule',1,'applied',?,?)").bind("2026-01-01T00:00:00Z").bind("2026-01-01T00:00:00.000999Z").execute(&store.pool).await.unwrap();
        assert!(source_history(&store, 601, at, at).await.unwrap());
        assert!(!source_history(&store, 601, at + 1, at + 1).await.unwrap());
        store.pool.close().await;
    }

    #[tokio::test]
    async fn application_batches_stop_before_retaining_excess_rows() {
        let directory = tempfile::tempdir().unwrap();
        let mut scenario = crate::fixtures::daily();
        scenario.limits.steps = 1;
        let mut world = World::open(scenario, &directory.path().join("source"))
            .await
            .unwrap();
        let initial = world.trace.len();
        let store = world.nodes["a"].engine().store.clone();
        for index in 0..800 {
            store::sqlx::query("INSERT INTO karma_rule_application(event_id,rule_uid,rule_revision,status,at,intended_at) VALUES(?,'rule',1,'applied',?,?)").bind(format!("batch-{index}")).bind("2026-01-01T00:00:00Z").bind("2026-01-01T00:00:00Z").execute(&store.pool).await.unwrap();
        }
        world.observe("a", Cause::Seed {}).await.unwrap();
        assert_eq!(
            world.trace[initial..]
                .iter()
                .filter(|event| matches!(event.observation, Observation::RuleApplication { .. }))
                .count(),
            800
        );
        let initial = world.trace.len();
        for index in 0..800 {
            store::sqlx::query("INSERT INTO karma_rule_application(event_id,rule_uid,rule_revision,status,at,intended_at) VALUES(?,'rule',1,'applied',?,?)").bind(format!("excess-{index}")).bind("2026-01-01T00:00:00Z").bind("2026-01-01T00:00:00Z").execute(&store.pool).await.unwrap();
        }
        world.scenario.limits.evidence_bytes = world.evidence_bytes + 4096;
        let error = world.observe("a", Cause::Seed {}).await.unwrap_err();
        assert!(crate::limits::is_exceeded(&error), "{error}");
        assert_eq!(world.stop, Some(report::Stop::EvidenceBudget {}));
        assert_eq!(world.trace.len(), initial);
        assert!(world.nodes["a"].application_position < 1600);
        store.pool.close().await;
    }
}

pub fn digest<T: Serialize>(value: &T) -> Result<CanonicalHash> {
    use sha2::Digest;
    struct Hasher(sha2::Sha256);
    impl Write for Hasher {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hash = Hasher(sha2::Sha256::new());
    serde_json::to_writer(&mut hash, value)?;
    let encoded: String = hash
        .0
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(CanonicalHash::parse(format!("sha256:{encoded}"))?)
}

pub(crate) fn derived_secret(seed: u64, cell: &str, purpose: &str) -> [u8; 32] {
    let hex = nucleus::fact::sha256_hex(
        format!("lince.simulation.v1:{seed}:{cell}:{purpose}").as_bytes(),
    );
    std::array::from_fn(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
}

impl World {
    pub async fn open(scenario: Scenario, directory: &Path) -> Result<Self> {
        Self::open_controlled(scenario, directory, None, None, None).await
    }

    pub(crate) async fn open_controlled(
        scenario: Scenario,
        directory: &Path,
        replay_checkpoint: Option<u64>,
        control: Option<nucleus::execution::control::Control>,
        signer: Option<engine::trust::Signer>,
    ) -> Result<Self> {
        scenario.validate()?;
        std::fs::create_dir_all(directory.join("seed"))?;
        std::fs::create_dir_all(directory.join("working"))?;
        let journal = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("trace.jsonl"))?;
        let mut world = Self {
            now_ms: scenario.start_ms,
            control: control.unwrap_or_else(|| {
                nucleus::execution::control::Control::new(
                    scenario.limits.rule_evaluations,
                    scenario.limits.wall_time_ms,
                    replay_checkpoint,
                )
            }),
            scenario,
            nodes: BTreeMap::new(),
            trace: Vec::new(),
            captured: BTreeMap::new(),
            steps: 0,
            inputs: 0,
            evidence_bytes: 0,
            source_history: BTreeSet::new(),
            evidence_micros: 0,
            stop: None,
            assumed_transfers: BTreeMap::new(),
            tasks: BTreeMap::new(),
            partitions: BTreeSet::new(),
            serial: 0,
            journal,
        };
        let mut snapshot_remaining = world
            .scenario
            .limits
            .evidence_bytes
            .saturating_add(world.scenario.cells.len() as u64 * crate::limits::JSON_BYTES);
        for seed in world.scenario.cells.clone() {
            let execution = Execution::new(
                derived_secret(
                    world.scenario.seed,
                    &seed.name,
                    &seed.database.as_ref().map_or_else(
                        || "execution".into(),
                        |database| format!("execution:{}", database.hash.as_str()),
                    ),
                ),
                world.now_ms,
            )?
            .controlled(world.control.clone(), seed.name.clone());
            let secret = derived_secret(world.scenario.seed, &seed.name, "signer");
            let path = directory
                .join("working")
                .join(format!("{}.sqlite", seed.name));
            if path.exists() {
                return Err("world database already exists".into());
            }
            let store = if let Some(database) = &seed.database {
                std::fs::copy(directory.join(&database.file), &path)?;
                execution
                    .scope(store::Store::open_existing_durable(&format!(
                        "sqlite://{}",
                        path.display()
                    )))
                    .await?
            } else {
                execution
                    .scope(store::Store::open(&format!("sqlite://{}", path.display())))
                    .await?
            };
            let integrity: String = store::sqlx::query_scalar("PRAGMA quick_check(1)")
                .fetch_one(&store.pool)
                .await?;
            if integrity != "ok" {
                return Err(format!("seed integrity: {integrity}").into());
            }
            if let Some(proposal) = &world.scenario.proposal
                && proposal.cell == seed.name
            {
                proposal.verify_source(&store).await?;
            }
            let fact_position =
                store::sqlx::query_scalar("SELECT COALESCE(MAX(rowid), 0) FROM fact")
                    .fetch_one(&store.pool)
                    .await?;
            let application_position = store::sqlx::query_scalar(
                "SELECT COALESCE(MAX(rowid), 0) FROM karma_rule_application",
            )
            .fetch_one(&store.pool)
            .await?;
            let prior = source_history(
                &store,
                application_position,
                world.scenario.start_ms,
                world.scenario.end_ms,
            )
            .await?;
            if prior {
                world.source_history.insert(seed.name.clone());
            }
            let cell = Cell::isolated(store, &execution, secret).await?;
            let state_hasher =
                store::snapshot::StateHasher::new(cell.runtime().store.clone()).await?;
            let mut node = Node {
                state_hasher,
                cell,
                execution,
                next_ms: None,
                offset_ms: 0,
                path,
                secret,
                fact_position,
                application_position,
                quantities: BTreeMap::new(),
                loan_quantities: BTreeMap::new(),
                loan_unit_changes: Default::default(),
                deleted_records: BTreeSet::new(),
                person_key: None,
                online: true,
            };
            if seed.name == "current"
                && let Some(signer) = &signer
            {
                node.execution
                    .scope(node.engine().set_signer(signer.clone()))
                    .await?;
                node.person_key = Some(signer.clone());
            }
            (node.quantities, node.deleted_records) = quantities(node.engine()).await?;
            let organ = store::organs::local(&node.engine().store.pool)
                .await?
                .ok_or("seed Organ missing")?;
            let local = store::cells::local(&node.engine().store.pool)
                .await?
                .ok_or("seed Cell missing")?;
            world
                .captured
                .insert(format!("cell:{}:organ", seed.name), organ.uid);
            world
                .captured
                .insert(format!("cell:{}:uid", seed.name), local.uid);
            world.nodes.insert(seed.name.clone(), node);
            if let Some(proposal) = &world.scenario.proposal
                && proposal.cell == seed.name
            {
                proposal.bind(world.nodes[&seed.name].engine()).await?;
            }
            if !seed.lingua.is_empty() {
                world.refresh_control_checks().await?;
                let node = &world.nodes[&seed.name];
                let working = directory
                    .join("working")
                    .join(format!("{}.lingua", seed.name));
                let imported = node
                    .execution
                    .scope(crate::lingua::import(
                        node.engine(),
                        &seed.lingua,
                        directory,
                        &working,
                    ))
                    .await;
                match imported {
                    Ok(observation) => world.emit(&seed.name, Cause::Seed {}, observation)?,
                    Err(error) => {
                        if let Some(limit) = world.control.stopped() {
                            world.stop = Some(control_stop(limit));
                            world.emit(
                                &seed.name,
                                Cause::Seed {},
                                Observation::LinguaInterrupted {
                                    files: seed
                                        .lingua
                                        .iter()
                                        .map(|file| (file.name.clone(), file.hash.clone()))
                                        .collect(),
                                },
                            )?;
                        } else {
                            return Err(error);
                        }
                    }
                }
                world.observe(&seed.name, Cause::Seed {}).await?;
            }
            for invocation in seed.seed {
                world
                    .invoke(&seed.name, &invocation, Cause::Seed {})
                    .await?;
            }
            world.tick(&seed.name, Cause::Seed {}).await?;
            let node = &world.nodes[&seed.name];
            let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
                .fetch_one(&node.engine().store.pool)
                .await?;
            let page_size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
                .fetch_one(&node.engine().store.pool)
                .await?;
            let bytes = u64::try_from(pages)?.saturating_mul(u64::try_from(page_size)?);
            if bytes > snapshot_remaining || bytes > crate::limits::LINES_BYTES {
                return Err(Box::new(crate::limits::Exceeded {
                    resource: "seed snapshots",
                    limit: snapshot_remaining.min(crate::limits::LINES_BYTES),
                }));
            }
            snapshot_remaining -= bytes;
            node.cell
                .runtime()
                .store
                .snapshot_into(&directory.join("seed").join(format!("{}.sqlite", seed.name)))
                .await?;
            crate::artifacts::atomic(
                &directory
                    .join("seed")
                    .join(format!("{}.environment.json", seed.name)),
                &node.execution.snapshot(),
            )?;
        }
        for input in world.scenario.effective_inputs()? {
            world.serial += 1;
            world
                .tasks
                .insert((input.at_ms, 0, world.serial), Task::Input(input));
        }
        world.schedule_check(world.scenario.start_ms - 1);
        Ok(world)
    }

    fn schedule_check(&mut self, after: i64) {
        if let Some(at) = self
            .scenario
            .checks
            .iter()
            .filter_map(|check| {
                check.next_time(after, self.scenario.start_ms, self.scenario.end_ms)
            })
            .min()
        {
            self.tasks.insert((at, 3, 0), Task::Check);
        }
    }

    async fn refresh_control_checks(&self) -> Result<()> {
        use nucleus::simulation::{FailureMode, Predicate};
        let mut checks = Vec::new();
        if self.scenario.checking.on_failure == FailureMode::Stop {
            for definition in &self.scenario.checks {
                if !definition.options.enabled {
                    continue;
                }
                let record = match &definition.predicate {
                    Predicate::Quantity { cell, record, .. }
                    | Predicate::QuantityEquals { cell, record, .. }
                    | Predicate::Nonnegative { cell, record } => {
                        let Some(node) = self.nodes.get(cell) else {
                            continue;
                        };
                        store::records::resolve(
                            &node.engine().store.pool,
                            &self.resolve_reference(record),
                        )
                        .await?
                        .map(|record| record.uid)
                    }
                    Predicate::NoRuleCycles { .. } => None,
                    _ => continue,
                };
                checks.push(nucleus::execution::control::ResolvedCheck {
                    definition: definition.clone(),
                    record,
                });
            }
        }
        self.control
            .configure(self.scenario.limits.evidence_bytes / 2, checks);
        self.control.set_time(self.now_ms);
        Ok(())
    }

    pub fn domain_next_ms(&self) -> Option<i64> {
        self.tasks
            .iter()
            .filter(|(_, task)| !matches!(task, Task::Check))
            .map(|(key, _)| key.0)
            .chain(
                self.nodes
                    .values()
                    .filter(|node| node.online)
                    .filter_map(|node| node.next_ms),
            )
            .min()
    }

    pub(crate) fn emit(
        &mut self,
        cell: &str,
        caused_by: Cause,
        observation: Observation,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let event = report::Event {
            sequence: self.trace.len() as u64,
            virtual_ms: self.now_ms,
            cell: cell.into(),
            caused_by,
            observation,
        };
        let remaining = self
            .scenario
            .limits
            .evidence_bytes
            .saturating_sub(self.evidence_bytes)
            .saturating_sub(1)
            .min(crate::limits::JSON_BYTES - 1);
        if self.trace.len() as u64 >= crate::limits::event_rows(&self.scenario) {
            self.stop.get_or_insert(report::Stop::EvidenceBudget {});
            return Err(Box::new(crate::limits::Exceeded {
                resource: "event rows",
                limit: self.scenario.limits.evidence_bytes,
            }));
        }
        let bytes = match crate::limits::bytes(&event, remaining, "event evidence") {
            Ok(bytes) => bytes,
            Err(error) => {
                if crate::limits::is_exceeded(&error) {
                    self.stop.get_or_insert(report::Stop::EvidenceBudget {});
                }
                return Err(error);
            }
        };
        self.evidence_bytes = self
            .evidence_bytes
            .checked_add(bytes.len() as u64 + 1)
            .ok_or("evidence counter overflow")?;
        self.journal.write_all(&bytes)?;
        self.journal.write_all(b"\n")?;
        self.trace.push(event);
        self.evidence_micros += elapsed(started);
        Ok(())
    }

    pub(crate) async fn observe(&mut self, name: &str, cause: Cause) -> Result<()> {
        let result = self.observe_inner(name, cause).await;
        if result.as_ref().is_err_and(crate::limits::is_exceeded) {
            self.stop.get_or_insert(report::Stop::EvidenceBudget {});
        }
        result
    }

    async fn observe_inner(&mut self, name: &str, cause: Cause) -> Result<()> {
        let started = std::time::Instant::now();
        let evidence_before = self.evidence_micros;
        let node = self.nodes.get_mut(name).ok_or("unknown Cell")?;
        let mut observations = Observations {
            values: Vec::new(),
            bytes: 0,
            limit: self
                .scenario
                .limits
                .evidence_bytes
                .saturating_sub(self.evidence_bytes),
            row_limit: crate::limits::event_rows(&self.scenario)
                .saturating_sub(self.trace.len() as u64),
            cell: name,
            cause: &cause,
            now_ms: self.now_ms,
            sequence: self.trace.len() as u64,
        };
        let previous_quantities = node.quantities.clone();
        loop {
            let facts = store::facts::after_position_with_commits(
                &node.engine().store.pool,
                node.fact_position,
                256,
            )
            .await?;
            if facts.is_empty() {
                break;
            }
            for (position, commit, fact) in facts {
                let unit: Option<String> =
                    store::sqlx::query_scalar("SELECT unit_uid FROM record WHERE uid = ?")
                        .bind(&fact.record_uid)
                        .fetch_one(&node.engine().store.pool)
                        .await?;
                let unit = unit
                    .map(|uid| TypedUid::new(ReferenceKind::Unit, uid))
                    .transpose()?;
                let before = node
                    .quantities
                    .get(&fact.record_uid)
                    .cloned()
                    .unwrap_or(Quantity {
                        value: store::exact::zero(),
                        unit,
                    });
                let after = Quantity {
                    value: store::exact::sum_exact([before.value, fact.delta])?,
                    unit: before.unit.clone(),
                };
                node.quantities
                    .insert(fact.record_uid.clone(), after.clone());
                let fact_cause = fact_cause(node.engine(), &fact).await?;
                observations.push(Observation::CommittedQuantity {
                    record: TypedUid::new(ReferenceKind::Record, fact.record_uid)?,
                    fact: fact.uid.try_into()?,
                    before,
                    after,
                    delta: fact.delta,
                    at_ms: fact.at.timestamp_millis(),
                    cause: fact_cause,
                    previous_fact_hash: fact.prev_hash,
                    fact_hash: fact.hash,
                    commit,
                })?;
                node.fact_position = position;
            }
        }
        let applications = store::sqlx::query("SELECT count(*) AS rows, COALESCE(sum(length(CAST(rule_uid AS BLOB)) + length(CAST(event_id AS BLOB)) + length(CAST(intended_at AS BLOB)) + length(CAST(status AS BLOB)) + COALESCE(length(CAST(frequency_uid AS BLOB)),0) + COALESCE(length(CAST(reason AS BLOB)),0)),0) AS bytes FROM karma_rule_application WHERE rowid > ?").bind(node.application_position).fetch_one(&node.engine().store.pool).await?;
        if u64::try_from(applications.try_get::<i64, _>("bytes")?)?
            > observations.limit.saturating_sub(observations.bytes)
            || u64::try_from(applications.try_get::<i64, _>("rows")?)?
                > observations
                    .row_limit
                    .saturating_sub(observations.values.len() as u64)
        {
            return Err(Box::new(crate::limits::Exceeded {
                resource: "application evidence",
                limit: observations.limit,
            }));
        }
        loop {
            let applications = store::sqlx::query("SELECT rowid AS position,rule_uid,rule_revision,event_id,frequency_uid,intended_at,status,reason FROM karma_rule_application WHERE rowid > ? ORDER BY rowid LIMIT ?")
            .bind(node.application_position).bind(crate::limits::BATCH_ROWS as i64).fetch_all(&node.engine().store.pool).await?;
            if applications.is_empty() {
                break;
            }
            for row in applications {
                let status: String = row.get("status");
                let status = match status.as_str() {
                    "applied" => report::ApplicationStatus::Applied,
                    "blocked" => report::ApplicationStatus::Blocked,
                    "failed" => report::ApplicationStatus::Failed,
                    _ => return Err(format!("unknown rule application status {status}").into()),
                };
                let intended_at: String = row.get("intended_at");
                let frequency: Option<String> = row.get("frequency_uid");
                observations.push(Observation::RuleApplication {
                    occurrence: report::RuleOccurrence {
                        rule_uid: row.get("rule_uid"),
                        revision: row.get::<i64, _>("rule_revision").try_into()?,
                        event_id: row.get("event_id"),
                        frequency: frequency
                            .map(|uid| TypedUid::new(ReferenceKind::Frequency, uid))
                            .transpose()?,
                        intended_at_ms: Some(
                            chrono::DateTime::parse_from_rfc3339(&intended_at)?.timestamp_millis(),
                        ),
                    },
                    status,
                    reason: row.get("reason"),
                })?;
                node.application_position = row.get("position");
            }
        }
        (node.quantities, node.deleted_records) = quantities(node.engine()).await?;
        let adjustments = node
            .execution
            .scope(store::transfer_loans::adjustments(
                &node.engine().store.pool,
                node.execution.now().timestamp_millis(),
            ))
            .await?;
        for adjustment in &adjustments {
            if let Some(next) = adjustment.next_ms {
                node.next_ms = node
                    .next_ms
                    .into_iter()
                    .chain(Some(next.saturating_sub(node.offset_ms).max(self.now_ms)))
                    .min();
            }
        }
        let mut loan_quantities = BTreeMap::new();
        node.loan_unit_changes.clear();
        for adjustment in adjustments {
            if adjustment.unit_changed {
                node.loan_unit_changes.insert(adjustment.record);
                continue;
            }
            let Some(physical) = node.quantities.get(&adjustment.record) else {
                continue;
            };
            let after = Quantity {
                value: store::exact::sum_exact([physical.value, adjustment.delta])?,
                unit: physical.unit.clone(),
            };
            let before = node
                .loan_quantities
                .get(&adjustment.record)
                .or_else(|| previous_quantities.get(&adjustment.record))
                .cloned()
                .unwrap_or_else(|| physical.clone());
            if before != after || !node.loan_quantities.contains_key(&adjustment.record) {
                observations.push(Observation::LoanQuantity {
                    record: TypedUid::new(ReferenceKind::Record, adjustment.record.clone())?,
                    before,
                    after: after.clone(),
                    physical: physical.clone(),
                    at_ms: node.execution.now().timestamp_millis(),
                })?;
            }
            loan_quantities.insert(adjustment.record, after);
        }
        for (record, before) in &node.loan_quantities {
            if !node.loan_unit_changes.contains(record)
                && !loan_quantities.contains_key(record)
                && let Some(physical) = node.quantities.get(record)
            {
                observations.push(Observation::LoanQuantity {
                    record: TypedUid::new(ReferenceKind::Record, record.clone())?,
                    before: before.clone(),
                    after: physical.clone(),
                    physical: physical.clone(),
                    at_ms: node.execution.now().timestamp_millis(),
                })?;
            }
        }
        node.loan_quantities = loan_quantities;
        let hash = node.state_hasher.hash().await?;
        observations.push(Observation::State { hash })?;
        let mut pending = self
            .tasks
            .values()
            .any(|task| matches!(task, Task::Deliver(_)));
        for node in self.nodes.values() {
            let effects: bool = store::sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM effect_queue WHERE status IN ('queued', 'running'))",
            )
            .fetch_one(&node.engine().store.pool)
            .await?;
            pending |= effects;
            let commands: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_transfer_command k JOIN transfer_remote_command c ON c.command_uid = k.command_uid WHERE k.cancelled = 0 AND c.direction = 'outgoing' AND c.status IN ('queued', 'sent'))").fetch_one(&node.engine().store.pool).await?;
            pending |= commands;
        }
        if !pending && self.control.stopped().is_none() {
            self.control.settle(name);
        }
        for cycle in self.control.drain_cycles(name) {
            observations.push(Observation::RuleCycle { cycle })?;
        }
        let actor = self
            .scenario
            .proposal
            .as_ref()
            .filter(|proposal| proposal.cell == name)
            .and_then(|proposal| proposal.actor.as_deref());
        for mut schedule in self.control.drain_schedules(name) {
            if self.nodes[name]
                .engine()
                .refuse_unreadable(actor, &[schedule.record.as_str().into()])
                .await
                .is_err()
            {
                schedule.time = None;
                schedule.unavailable = true;
            }
            observations.push(Observation::RuleSchedule { schedule })?;
        }
        for observation in observations.values {
            self.emit(name, cause.clone(), observation)?;
        }
        self.journal.sync_data()?;
        self.evidence_micros +=
            elapsed(started).saturating_sub(self.evidence_micros - evidence_before);
        Ok(())
    }

    pub async fn tick(&mut self, name: &str, cause: Cause) -> Result<()> {
        self.refresh_control_checks().await?;
        if let Some(limit) = self.control.stopped() {
            self.stop.get_or_insert(control_stop(limit));
            return self.observe(name, cause).await;
        }
        let node = self.nodes.get_mut(name).ok_or("unknown Cell")?;
        if !node.online {
            return Ok(());
        }
        node.set_time(self.now_ms)?;
        let mut deadline_changes = node.engine().subscribe_karma_deadline_changes();
        let step = match node
            .execution
            .scope(node.engine().step_karma_time(node.execution.now(), 1))
            .await
        {
            Ok(step) => step,
            Err(EngineError::ExecutionLimit(limit)) => {
                self.stop = Some(control_stop(limit));
                return self.observe(name, cause).await;
            }
            Err(error) => return Err(error.into()),
        };
        node.next_ms = step
            .next_at_ms
            .map(|time| time.saturating_sub(node.offset_ms).max(self.now_ms));
        let _ = deadline_changes.borrow_and_update();
        let effects = match node
            .execution
            .scope(node.engine().run_database_effects())
            .await
        {
            Ok(effects) => effects,
            Err(EngineError::ExecutionLimit(limit)) => {
                self.stop = Some(control_stop(limit));
                return self.observe(name, cause).await;
            }
            Err(error) => return Err(error.into()),
        };
        if effects.pending || deadline_changes.has_changed().unwrap_or(false) {
            node.next_ms = Some(self.now_ms);
        }
        if effects.unsupported {
            self.stop = Some(report::Stop::UnsupportedEffect { cell: name.into() });
        }
        for effect in effects.outcomes {
            let kind = match effect.kind.as_str() {
                "action" => report::DatabaseEffectKind::Action,
                "consequence" => report::DatabaseEffectKind::Consequence,
                "notify" => report::DatabaseEffectKind::Notify,
                "command" | "signal" | "saved-command" => report::DatabaseEffectKind::Command,
                _ => return Err("unsupported database effect executed".into()),
            };
            self.emit(
                name,
                cause.clone(),
                Observation::DatabaseEffect {
                    uid: effect.uid,
                    effect: kind,
                    ok: effect.ok,
                    result: effect.result,
                },
            )?;
        }
        self.observe(name, cause).await
    }

    pub(crate) async fn invoke(
        &mut self,
        name: &str,
        invocation: &Invocation,
        cause: Cause,
    ) -> Result<()> {
        if self.stop.is_some() {
            return Ok(());
        }
        self.refresh_control_checks().await?;
        let action = resolve(&invocation.action, &self.captured)?;
        let actor = invocation.actor.as_ref().map(|actor| {
            self.captured
                .get(actor.strip_prefix('$').unwrap_or(actor))
                .cloned()
                .unwrap_or_else(|| actor.clone())
        });
        let node = &self.nodes[name];
        node.set_time(self.now_ms)?;
        let outcome = node.execution.scope(node.engine().act(action, actor)).await;
        let observation = match outcome {
            Ok(outcome) => {
                if let Some(created) = &outcome.created {
                    self.captured.insert(invocation.id.clone(), created.clone());
                }
                Observation::ActionAccepted {
                    input: invocation.id.clone(),
                    created: outcome.created,
                }
            }
            Err(EngineError::ExecutionLimit(limit)) => {
                self.stop = Some(control_stop(limit));
                Observation::ActionInterrupted {
                    input: invocation.id.clone(),
                }
            }
            Err(error) => match refusal(&error) {
                Some(refusal) => Observation::ActionRefused {
                    input: invocation.id.clone(),
                    refusal,
                },
                None => {
                    self.stop = Some(report::Stop::ExecutionError {
                        cell: name.into(),
                        input: Some(invocation.id.clone()),
                        category: error_category(&error),
                    });
                    self.observe(name, cause).await?;
                    return Err(error.into());
                }
            },
        };
        self.emit(name, cause.clone(), observation)?;
        self.observe(name, cause).await
    }

    pub fn next_ms(&self) -> Option<i64> {
        let task = self.tasks.first_key_value().map(|(key, _)| key.0);
        self.nodes
            .values()
            .filter(|node| node.online)
            .filter_map(|node| node.next_ms)
            .chain(task)
            .min()
    }

    pub async fn step(&mut self) -> Result<bool> {
        if self.stop.is_some() {
            return Ok(false);
        }
        self.control.wait_running().await;
        if let Err(limit) = self.control.checkpoint(false) {
            self.stop = Some(control_stop(limit));
            return Ok(false);
        }
        if self.steps >= self.scenario.limits.steps {
            self.stop = Some(report::Stop::EventBudget {});
            return Ok(false);
        }
        let Some(next) = self.next_ms().filter(|next| *next <= self.scenario.end_ms) else {
            self.now_ms = self.scenario.end_ms;
            self.stop = Some(report::Stop::HorizonReached {});
            return Ok(false);
        };
        self.now_ms = next;
        self.refresh_control_checks().await?;
        let key = self
            .tasks
            .first_key_value()
            .map(|(key, _)| *key)
            .filter(|key| key.0 == next);
        if let Some(key) = key.filter(|key| key.1 < 2) {
            self.steps += 1;
            let task = self.tasks.remove(&key).unwrap();
            let (cell, input) = match &task {
                Task::Input(input) => (input.cell.clone(), Some(input.id.clone())),
                Task::Deliver(message) => (message.to.clone(), None),
                Task::Check => unreachable!(),
            };
            if let Err(error) = self.run_task(task).await {
                if let Some(limit) = self.control.stopped() {
                    self.stop = Some(control_stop(limit));
                    let names: Vec<_> = self.nodes.keys().cloned().collect();
                    for name in names {
                        self.observe(&name, Cause::Timer {}).await?;
                    }
                    return Ok(true);
                }
                self.stop.get_or_insert(report::Stop::ExecutionError {
                    cell,
                    input,
                    category: report::ExecutionError::Domain,
                });
                return Err(error);
            }
        } else if let Some(name) = self
            .nodes
            .iter()
            .find(|(_, node)| node.online && node.next_ms == Some(next))
            .map(|(name, _)| name.clone())
        {
            self.steps += 1;
            self.tick(&name, Cause::Timer {}).await?;
        } else if let Some(key) = key {
            self.tasks.remove(&key);
            self.schedule_check(self.now_ms);
        }
        Ok(true)
    }

    async fn run_task(&mut self, task: Task) -> Result<()> {
        match task {
            Task::Check => {}
            Task::Deliver(message) => self.deliver(message).await?,
            Task::Input(input) => {
                self.inputs += 1;
                let cause = Cause::Input {
                    id: input.id.clone(),
                };
                if !self.nodes[&input.cell].online
                    && !matches!(
                        input.event,
                        Event::Online { .. }
                            | Event::Restart {}
                            | Event::Clock { .. }
                            | Event::Link { .. }
                    )
                {
                    return self.emit(
                        &input.cell,
                        cause,
                        Observation::ActionRefused {
                            input: input.id,
                            refusal: Refusal::Conflict {
                                code: "cell_offline".into(),
                            },
                        },
                    );
                }
                match input.event {
                    Event::CommandResponse { response } => {
                        self.nodes[&input.cell]
                            .engine()
                            .append_command_response(response.clone())?;
                    }
                    Event::AssumeLoan { timing } => {
                        self.assume_loan(&input.cell, timing, cause).await?
                    }
                    Event::Online { online } => {
                        self.nodes.get_mut(&input.cell).unwrap().online = online;
                        self.emit(
                            &input.cell,
                            cause.clone(),
                            Observation::AvailabilityChanged { online },
                        )?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::Enrol { peer } => self.enrol(&input.cell, &peer, cause).await?,
                    Event::Discover { peer } => self.discover(&input.cell, &peer, cause).await?,
                    Event::PersonKey { person } => {
                        self.select_person(&input.cell, &person, cause).await?
                    }
                    Event::AcceptInvitation {
                        transfer,
                        person,
                        peer,
                    } => {
                        self.decide_invitation(
                            &input.cell,
                            &input.id,
                            &transfer,
                            &person,
                            peer.as_deref(),
                            true,
                            cause.clone(),
                        )
                        .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::RejectInvitation {
                        transfer,
                        person,
                        peer,
                    } => {
                        self.decide_invitation(
                            &input.cell,
                            &input.id,
                            &transfer,
                            &person,
                            peer.as_deref(),
                            false,
                            cause.clone(),
                        )
                        .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::TransferCommand {
                        peer,
                        transfer,
                        invocation,
                        delay_ms,
                        copies,
                        duplicate_spacing_ms,
                        drop,
                    } => {
                        self.transfer_command(
                            &input.cell,
                            &peer,
                            &transfer,
                            &invocation,
                            (delay_ms, copies, duplicate_spacing_ms, drop),
                            cause,
                        )
                        .await?;
                    }
                    Event::SettleReviewed {
                        occurrence,
                        person,
                        quantity,
                    } => {
                        self.settle_reviewed(
                            &input.cell,
                            &input.id,
                            &occurrence,
                            &person,
                            quantity,
                            cause.clone(),
                        )
                        .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::ApplyReceivedTransfer {
                        transfer,
                        occurrence,
                        person,
                        local_record,
                    } => {
                        self.apply_received_transfer(
                            &input.cell,
                            &input.id,
                            &transfer,
                            &occurrence,
                            &person,
                            &local_record,
                            cause.clone(),
                        )
                        .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::AssumeTransfer { assumption } => {
                        self.assume_transfer(&input.cell, &input.id, assumption, cause.clone())
                            .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::RefreshTransfer { transfer, person } => {
                        self.refresh_transfer(
                            &input.cell,
                            &input.id,
                            &transfer,
                            &person,
                            cause.clone(),
                        )
                        .await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::TransferDelivery {
                        peer,
                        delay_ms,
                        copies,
                        duplicate_spacing_ms,
                        drop,
                    } => {
                        let node = &self.nodes[&input.cell];
                        node.set_time(self.now_ms)?;
                        let remote = store::organs::local(&self.nodes[&peer].engine().store.pool)
                            .await?
                            .ok_or("missing recipient Organ")?;
                        let rows = store::transfer_delivery::outbox_due(
                            &node.engine().store.pool,
                            node.execution.now(),
                            32,
                        )
                        .await?;
                        let mut messages = Vec::new();
                        let mut cancelled_commands = Vec::new();
                        for row in rows {
                            let (recipient, request) = node
                                .execution
                                .scope(cell::transfer::prepare_envelope(node.cell.runtime(), &row))
                                .await?;
                            if recipient == remote.uid {
                                messages.push(Payload::Transfer(request));
                            }
                        }
                        for row in store::transfer_delivery::application_attestations_due(
                            &node.engine().store.pool,
                            node.execution.now(),
                            32,
                        )
                        .await?
                        {
                            if row.origin_organ_uid == remote.uid {
                                let request = node
                                    .execution
                                    .scope(cell::transfer::prepare_application_attestation(
                                        node.cell.runtime(),
                                        &row,
                                    ))
                                    .await?;
                                messages.push(Payload::Attestation(request));
                            }
                        }
                        for row in store::transfer_delivery::remote_commands_due(
                            &node.engine().store.pool,
                            node.execution.now(),
                            32,
                        )
                        .await?
                        {
                            if row.origin_organ_uid == remote.uid {
                                let command: nucleus::transfer_delivery::TransferRemoteCommandV1 =
                                    serde_json::from_str(&row.payload)?;
                                let wire = match node
                                    .execution
                                    .scope(cell::transfer::prepare_command(
                                        node.cell.runtime(),
                                        &row,
                                    ))
                                    .await
                                {
                                    Ok(wire) => wire,
                                    Err(error)
                                        if store::karma_commands::get(
                                            &node.engine().store.pool,
                                            &row.command_uid,
                                        )
                                        .await?
                                        .is_some_and(|state| state.cancelled) =>
                                    {
                                        cancelled_commands.push((row.command_uid, error));
                                        continue;
                                    }
                                    Err(error) => return Err(error.into()),
                                };
                                messages.push(Payload::Command {
                                    input: command.message_id,
                                    wire,
                                });
                            }
                        }
                        for row in store::transfer_delivery::pulls_due(
                            &node.engine().store.pool,
                            node.execution.now(),
                            32,
                        )
                        .await?
                        {
                            let (origin, wire) = node
                                .execution
                                .scope(cell::transfer::prepare_pull(node.cell.runtime(), &row))
                                .await?;
                            if origin == remote.uid {
                                messages.push(Payload::Pull {
                                    uid: row.uid,
                                    reference: row.reference_uid,
                                    after_cursor: row.after_cursor,
                                    wire,
                                });
                            }
                        }
                        for (uid, result) in cancelled_commands {
                            self.emit(
                                &input.cell,
                                cause.clone(),
                                Observation::DatabaseEffect {
                                    uid,
                                    effect: report::DatabaseEffectKind::Action,
                                    ok: false,
                                    result,
                                },
                            )?;
                        }
                        for payload in messages {
                            self.queue(
                                &input.cell,
                                &peer,
                                payload,
                                (delay_ms, copies, duplicate_spacing_ms, drop),
                                cause.clone(),
                            )?;
                        }
                        self.observe(&input.cell, cause).await?;
                    }
                    Event::Action { invocation } => {
                        self.invoke(&input.cell, &invocation, cause.clone()).await?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::Pair { peer } => {
                        self.publish_identity(&input.cell).await?;
                        self.publish_identity(&peer).await?;
                        let node = &self.nodes[&input.cell];
                        node.set_time(self.now_ms)?;
                        let introduction =
                            node.execution.scope(node.engine().introduction()).await?;
                        let receiver = &self.nodes[&peer];
                        receiver.set_time(self.now_ms)?;
                        let response = receiver
                            .execution
                            .scope(receiver.engine().introduction())
                            .await?;
                        receiver
                            .execution
                            .scope(receiver.engine().adopt_introduction(&introduction, 1))
                            .await?;
                        receiver
                            .execution
                            .scope(store::organs::set_trust(
                                &receiver.engine().store.pool,
                                &introduction.organ_uid,
                                "known",
                            ))
                            .await?;
                        node.execution
                            .scope(node.engine().adopt_introduction(&response, 1))
                            .await?;
                        if let Some(roster) =
                            node.engine().roster_of(&introduction.organ_uid).await?
                        {
                            receiver
                                .execution
                                .scope(receiver.engine().adopt_roster(&roster))
                                .await?;
                        }
                        if let Some(roster) =
                            receiver.engine().roster_of(&response.organ_uid).await?
                        {
                            node.execution
                                .scope(node.engine().adopt_roster(&roster))
                                .await?;
                        }
                        node.execution
                            .scope(store::organs::set_trust(
                                &node.engine().store.pool,
                                &response.organ_uid,
                                "known",
                            ))
                            .await?;
                        self.observe(&input.cell, cause.clone()).await?;
                        self.observe(&peer, cause).await?;
                    }
                    Event::Sync {
                        peer,
                        delay_ms,
                        copies,
                        duplicate_spacing_ms,
                        drop,
                    } => {
                        if self
                            .tasks
                            .values()
                            .filter(|task| matches!(task, Task::Deliver(_)))
                            .count()
                            + copies as usize
                            > self.scenario.limits.pending_messages
                        {
                            self.stop = Some(report::Stop::EventBudget {});
                            return Ok(());
                        }
                        let node = &self.nodes[&input.cell];
                        node.set_time(self.now_ms)?;
                        let organ = store::organs::local(&node.engine().store.pool)
                            .await?
                            .ok_or("local Organ missing")?;
                        let receiver = &self.nodes[&peer];
                        let remote = store::organs::local(&receiver.engine().store.pool)
                            .await?
                            .ok_or("peer Organ missing")?;
                        let vector = store::sync_ops::version_vector_for_organ(
                            &receiver.engine().store.pool,
                            &organ.uid,
                        )
                        .await?;
                        let batch = node
                            .execution
                            .scope(node.engine().export_sync_page(&remote.uid, &vector, 2000))
                            .await;
                        let batch = match batch {
                            Ok(page) => page.batch,
                            Err(error) => {
                                if let Some(refusal) = refusal(&error) {
                                    self.emit(
                                        &input.cell,
                                        cause.clone(),
                                        Observation::ActionRefused {
                                            input: input.id,
                                            refusal,
                                        },
                                    )?;
                                    self.observe(&input.cell, cause).await?;
                                    return Ok(());
                                }
                                return Err(error.into());
                            }
                        };
                        let payload_hash = digest(&batch)?;
                        for copy in 0..copies {
                            self.serial += 1;
                            let id = self.serial;
                            self.emit(
                                &input.cell,
                                cause.clone(),
                                Observation::MessageQueued {
                                    message: id,
                                    to: peer.clone(),
                                    payload_hash: payload_hash.clone(),
                                },
                            )?;
                            if drop {
                                self.emit(
                                    &input.cell,
                                    cause.clone(),
                                    Observation::MessageDropped {
                                        message: id,
                                        reason: report::DropReason::ScriptedLoss,
                                    },
                                )?;
                            } else {
                                self.tasks.insert(
                                    (
                                        self.now_ms
                                            + (delay_ms + duplicate_spacing_ms * u64::from(copy))
                                                as i64,
                                        1,
                                        id,
                                    ),
                                    Task::Deliver(Message {
                                        id,
                                        from: input.cell.clone(),
                                        to: peer.clone(),
                                        payload: Payload::Sync(batch.clone()),
                                    }),
                                );
                            }
                        }
                    }
                    Event::Link { peer, connected } => {
                        for link in [
                            (input.cell.clone(), peer.clone()),
                            (peer.clone(), input.cell.clone()),
                        ] {
                            if connected {
                                self.partitions.remove(&link);
                            } else {
                                self.partitions.insert(link);
                            }
                        }
                        self.emit(
                            &input.cell,
                            cause,
                            Observation::LinkChanged { peer, connected },
                        )?;
                    }
                    Event::Restart {} => {
                        let node = self.nodes.get_mut(&input.cell).unwrap();
                        node.set_time(self.now_ms)?;
                        node.cell.runtime().store.pool.close().await;
                        let store = node
                            .execution
                            .scope(store::Store::open_existing_durable(&format!(
                                "sqlite://{}",
                                node.path.display()
                            )))
                            .await?;
                        node.cell = Cell::isolated(store, &node.execution, node.secret).await?;
                        node.state_hasher =
                            store::snapshot::StateHasher::new(node.cell.runtime().store.clone())
                                .await?;
                        if let Some(signer) = &node.person_key {
                            node.execution
                                .scope(node.engine().set_signer(signer.clone()))
                                .await?;
                        }
                        self.emit(&input.cell, cause.clone(), Observation::Restarted {})?;
                        self.tick(&input.cell, cause).await?;
                    }
                    Event::Clock { offset_ms } => {
                        self.nodes.get_mut(&input.cell).unwrap().offset_ms = offset_ms;
                        self.emit(
                            &input.cell,
                            cause.clone(),
                            Observation::ClockChanged { offset_ms },
                        )?;
                        self.tick(&input.cell, cause).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn deliver(&mut self, message: Message) -> Result<()> {
        let cause = Cause::Delivery {
            message: message.id,
        };
        if !self.nodes[&message.to].online {
            return self.emit(
                &message.to,
                cause,
                Observation::MessageDropped {
                    message: message.id,
                    reason: report::DropReason::Offline,
                },
            );
        }
        if self
            .partitions
            .contains(&(message.from.clone(), message.to.clone()))
        {
            return self.emit(
                &message.to,
                cause,
                Observation::MessageDropped {
                    message: message.id,
                    reason: report::DropReason::Partition,
                },
            );
        }
        let node = &self.nodes[&message.to];
        node.set_time(self.now_ms)?;
        let batch = match message.payload {
            Payload::Sync(batch) => batch,
            Payload::Pull {
                uid,
                reference,
                after_cursor,
                wire,
            } => {
                match node
                    .execution
                    .scope(cell::transfer::pull_envelope(node.cell.runtime(), wire))
                    .await
                {
                    Ok(value) => self.queue(
                        &message.to,
                        &message.from,
                        Payload::PullResult {
                            uid,
                            reference,
                            after_cursor,
                            wire: serde_json::from_value(value)?,
                        },
                        (1, 1, 0, false),
                        cause.clone(),
                    )?,
                    Err((refusal, detail)) => {
                        if matches!(refusal, cell::transfer::Refusal::Internal) {
                            return Err(detail.into());
                        }
                        self.emit(
                            &message.to,
                            cause.clone(),
                            Observation::MessageRefused {
                                message: message.id,
                                from: message.from,
                                refusal: transfer_refusal(refusal)?,
                            },
                        )?;
                    }
                }
                self.observe(&message.to, cause.clone()).await?;
                return self.tick(&message.to, cause).await;
            }
            Payload::PullResult {
                uid,
                reference,
                after_cursor,
                wire,
            } => {
                let (cursor, receipt) = node
                    .execution
                    .scope(cell::transfer::receive_pull_result(
                        node.cell.runtime(),
                        &reference,
                        after_cursor,
                        wire,
                    ))
                    .await?;
                node.execution
                    .scope(store::transfer_delivery::pull_mark_completed(
                        &node.engine().store.pool,
                        &uid,
                        cursor,
                        node.execution.now(),
                    ))
                    .await?;
                if let Some(receipt) = receipt {
                    self.queue(
                        &message.to,
                        &message.from,
                        Payload::Receipt(receipt),
                        (1, 1, 0, false),
                        cause.clone(),
                    )?;
                }
                self.emit(
                    &message.to,
                    cause.clone(),
                    Observation::TransferReceived {
                        message: message.id,
                        from: message.from,
                        receipt: false,
                    },
                )?;
                return self.observe(&message.to, cause).await;
            }
            Payload::Attestation(wire) => {
                let result = node
                    .execution
                    .scope(cell::transfer::receive_application_attestation(
                        node.cell.runtime(),
                        wire,
                    ))
                    .await;
                match result {
                    Ok(value) => self.queue(
                        &message.to,
                        &message.from,
                        Payload::AttestationResult(serde_json::from_value(value)?),
                        (1, 1, 0, false),
                        cause.clone(),
                    )?,
                    Err((refusal, detail)) => {
                        if matches!(refusal, cell::transfer::Refusal::Internal) {
                            return Err(detail.into());
                        }
                        self.emit(
                            &message.to,
                            cause.clone(),
                            Observation::MessageRefused {
                                message: message.id,
                                from: message.from,
                                refusal: transfer_refusal(refusal)?,
                            },
                        )?;
                    }
                }
                self.observe(&message.to, cause.clone()).await?;
                return self.tick(&message.to, cause).await;
            }
            Payload::AttestationResult(wire) => {
                node.execution
                    .scope(cell::transfer::receive_application_attestation_result(
                        node.cell.runtime(),
                        wire,
                    ))
                    .await
                    .map_err(|(_, error)| error)?;
                self.emit(
                    &message.to,
                    cause.clone(),
                    Observation::TransferReceived {
                        message: message.id,
                        from: message.from,
                        receipt: true,
                    },
                )?;
                return self.observe(&message.to, cause).await;
            }
            Payload::Command { input, wire } => {
                let result = node
                    .execution
                    .scope(cell::transfer::receive_command(node.cell.runtime(), wire))
                    .await;
                match result {
                    Ok(value) => {
                        self.queue(
                            &message.to,
                            &message.from,
                            Payload::CommandResult {
                                input,
                                wire: serde_json::from_value(value)?,
                            },
                            (1, 1, 0, false),
                            cause.clone(),
                        )?;
                    }
                    Err((refusal, detail)) => {
                        if matches!(refusal, cell::transfer::Refusal::Internal) {
                            return Err(detail.into());
                        }
                        self.emit(
                            &message.to,
                            cause.clone(),
                            Observation::MessageRefused {
                                message: message.id,
                                from: message.from,
                                refusal: transfer_refusal(refusal)?,
                            },
                        )?;
                    }
                }
                self.observe(&message.to, cause.clone()).await?;
                return self.tick(&message.to, cause).await;
            }
            Payload::CommandResult { input, wire } => {
                let result = node
                    .execution
                    .scope(cell::transfer::receive_command_result(
                        node.cell.runtime(),
                        wire,
                    ))
                    .await
                    .map_err(|(_, error)| error)?;
                let automatic =
                    store::karma_commands::get(&node.engine().store.pool, &result.command_uid)
                        .await?
                        .is_some();
                let observation = if automatic {
                    if let Some(created) = &result.created {
                        self.captured
                            .insert(result.command_uid.clone(), created.clone());
                    }
                    Observation::DatabaseEffect {
                        uid: result.command_uid,
                        effect: report::DatabaseEffectKind::Action,
                        ok: result.accepted,
                        result: if result.accepted {
                            format!(
                                "Transfer command accepted at revision {}",
                                result.authoritative_revision
                            )
                        } else {
                            format!(
                                "Transfer command refused: {}",
                                result.message.unwrap_or_else(|| result
                                    .code
                                    .unwrap_or_else(|| "remote_action_rejected".into()))
                            )
                        },
                    }
                } else if result.accepted {
                    if let Some(created) = &result.created {
                        self.captured.insert(input.clone(), created.clone());
                    }
                    Observation::ActionAccepted {
                        input,
                        created: result.created,
                    }
                } else {
                    Observation::ActionRefused {
                        input,
                        refusal: Refusal::Conflict {
                            code: result
                                .code
                                .unwrap_or_else(|| "remote_action_rejected".into()),
                        },
                    }
                };
                self.emit(&message.to, cause.clone(), observation)?;
                return self.observe(&message.to, cause).await;
            }
            Payload::Transfer(request) => {
                let result = node
                    .execution
                    .scope(cell::transfer::receive_envelope(
                        node.cell.runtime(),
                        request,
                    ))
                    .await;
                match result {
                    Ok(response) => {
                        let receipt = serde_json::from_value(response)?;
                        self.emit(
                            &message.to,
                            cause.clone(),
                            Observation::TransferReceived {
                                message: message.id,
                                from: message.from.clone(),
                                receipt: false,
                            },
                        )?;
                        self.queue(
                            &message.to,
                            &message.from,
                            Payload::Receipt(receipt),
                            (0, 1, 0, false),
                            cause.clone(),
                        )?;
                    }
                    Err((refusal, detail))
                        if matches!(refusal, cell::transfer::Refusal::Internal) =>
                    {
                        return Err(detail.into());
                    }
                    Err((refusal, _)) => self.emit(
                        &message.to,
                        cause.clone(),
                        Observation::MessageRefused {
                            message: message.id,
                            from: message.from,
                            refusal: transfer_refusal(refusal)?,
                        },
                    )?,
                }
                self.observe(&message.to, cause.clone()).await?;
                return self.tick(&message.to, cause).await;
            }
            Payload::Receipt(receipt) => {
                let envelope = receipt.body.envelope_uid.clone();
                let cursor = receipt.body.cursor;
                let result = node
                    .execution
                    .scope(cell::transfer::receive_receipt(
                        node.cell.runtime(),
                        receipt,
                    ))
                    .await;
                match result {
                    Ok(_) => {
                        let uid: String = store::sqlx::query_scalar(
                            "SELECT uid FROM transfer_delivery_outbox WHERE envelope_uid = ?",
                        )
                        .bind(&envelope)
                        .fetch_one(&node.engine().store.pool)
                        .await?;
                        node.execution
                            .scope(store::transfer_delivery::outbox_mark_sent(
                                &node.engine().store.pool,
                                &uid,
                                cursor,
                                node.execution.now(),
                            ))
                            .await?;
                        self.emit(
                            &message.to,
                            cause.clone(),
                            Observation::TransferReceived {
                                message: message.id,
                                from: message.from,
                                receipt: true,
                            },
                        )?;
                    }
                    Err((refusal, _)) => self.emit(
                        &message.to,
                        cause.clone(),
                        Observation::MessageRefused {
                            message: message.id,
                            from: message.from,
                            refusal: transfer_refusal(refusal)?,
                        },
                    )?,
                }
                return self.observe(&message.to, cause).await;
            }
        };
        let sender = &self.nodes[&message.from];
        let authenticated = store::organs::local(&sender.engine().store.pool)
            .await?
            .ok_or("sender Organ missing")?
            .uid;
        let imported = node
            .execution
            .scope(node.engine().receive_sync_batch(&authenticated, &batch))
            .await;
        let imported = match imported {
            Ok(imported) => imported,
            Err(error) => {
                if let Some(refusal) = refusal(&error) {
                    self.emit(
                        &message.to,
                        cause.clone(),
                        Observation::MessageRefused {
                            message: message.id,
                            from: message.from,
                            refusal,
                        },
                    )?;
                    self.observe(&message.to, cause).await?;
                    return Ok(());
                }
                self.stop = Some(report::Stop::ExecutionError {
                    cell: message.to,
                    input: None,
                    category: error_category(&error),
                });
                return Err(error.into());
            }
        };
        self.emit(
            &message.to,
            cause.clone(),
            Observation::MessageDelivered {
                message: message.id,
                from: message.from,
                imported: imported as u64,
            },
        )?;
        self.observe(&message.to, cause.clone()).await?;
        self.tick(&message.to, cause).await
    }

    pub(crate) fn queue(
        &mut self,
        from: &str,
        to: &str,
        payload: Payload,
        delivery: (u64, u8, u64, bool),
        cause: Cause,
    ) -> Result<()> {
        let (delay, copies, spacing, drop) = delivery;
        if self
            .tasks
            .values()
            .filter(|task| matches!(task, Task::Deliver(_)))
            .count()
            + usize::from(copies)
            > self.scenario.limits.pending_messages
        {
            self.stop = Some(report::Stop::EventBudget {});
            return Ok(());
        }
        let payload_hash = digest(&payload)?;
        for copy in 0..copies {
            self.serial += 1;
            let id = self.serial;
            self.emit(
                from,
                cause.clone(),
                Observation::MessageQueued {
                    message: id,
                    to: to.into(),
                    payload_hash: payload_hash.clone(),
                },
            )?;
            if drop {
                self.emit(
                    from,
                    cause.clone(),
                    Observation::MessageDropped {
                        message: id,
                        reason: report::DropReason::ScriptedLoss,
                    },
                )?;
            } else {
                self.tasks.insert(
                    (
                        self.now_ms + (delay + spacing * u64::from(copy)) as i64,
                        1,
                        id,
                    ),
                    Task::Deliver(Message {
                        id,
                        from: from.into(),
                        to: to.into(),
                        payload: payload.clone(),
                    }),
                );
            }
        }
        Ok(())
    }

    pub async fn state_hash(&self) -> Result<CanonicalHash> {
        let mut hashes = Vec::new();
        for (name, node) in &self.nodes {
            hashes.push((
                name,
                node.state_hasher.hash().await?,
                node.execution.snapshot(),
                node.offset_ms,
                node.next_ms,
                node.online,
                node.person_key
                    .as_ref()
                    .map(|signer| (&signer.actor_uid, &signer.key_id, signer.public_key_b64())),
            ));
        }
        let pending = digest(
            &self
                .tasks
                .iter()
                .filter(|(_, task)| !matches!(task, Task::Check))
                .collect::<Vec<_>>(),
        )?;
        Ok(canonical_hash(
            "lince.simulation.world.v1",
            &(
                hashes,
                pending,
                &self.partitions,
                &self.captured,
                &self.assumed_transfers,
                self.serial,
            ),
        )?)
    }

    pub fn resolve_reference(&self, reference: &str) -> String {
        reference
            .strip_prefix('$')
            .and_then(|id| self.captured.get(id))
            .cloned()
            .unwrap_or_else(|| reference.into())
    }

    pub async fn quantity(
        &self,
        cell: &str,
        reference: &str,
    ) -> Result<Option<(TypedUid, Quantity)>> {
        self.quantity_with_basis(cell, reference, report::QuantityBasis::Stored)
            .await
    }

    pub async fn quantity_with_basis(
        &self,
        cell: &str,
        reference: &str,
        basis: report::QuantityBasis,
    ) -> Result<Option<(TypedUid, Quantity)>> {
        let node = self.nodes.get(cell).ok_or("unknown Cell")?;
        let Some(record) = store::records::resolve(
            &node.engine().store.pool,
            &self.resolve_reference(reference),
        )
        .await?
        else {
            return Ok(None);
        };
        if basis == report::QuantityBasis::Available && node.loan_unit_changes.contains(&record.uid)
        {
            return Ok(None);
        }
        let uid = TypedUid::new(ReferenceKind::Record, record.uid)?;
        let unit = record
            .unit_uid
            .map(|uid| TypedUid::new(ReferenceKind::Unit, uid))
            .transpose()?;
        let quantity = (basis == report::QuantityBasis::Available)
            .then(|| node.loan_quantities.get(uid.as_str()))
            .flatten()
            .cloned()
            .unwrap_or(Quantity {
                value: record.quantity,
                unit,
            });
        Ok(Some((uid, quantity)))
    }

    pub(crate) fn observed_quantity(
        &self,
        cell: &str,
        uid: &str,
        basis: report::QuantityBasis,
    ) -> Option<&Quantity> {
        let node = self.nodes.get(cell)?;
        if node.deleted_records.contains(uid)
            || basis == report::QuantityBasis::Available && node.loan_unit_changes.contains(uid)
        {
            None
        } else {
            (basis == report::QuantityBasis::Available)
                .then(|| node.loan_quantities.get(uid))
                .flatten()
                .or_else(|| node.quantities.get(uid))
        }
    }

    pub(crate) async fn available_unit_changed(&self, cell: &str, reference: &str) -> Result<bool> {
        let node = self.nodes.get(cell).ok_or("unknown Cell")?;
        if node.loan_unit_changes.is_empty() {
            return Ok(false);
        }
        Ok(store::records::resolve(
            &node.engine().store.pool,
            &self.resolve_reference(reference),
        )
        .await?
        .is_some_and(|record| node.loan_unit_changes.contains(&record.uid)))
    }

    pub(crate) fn has_loan_quantity(&self, cell: &str, uid: &str) -> bool {
        self.nodes
            .get(cell)
            .is_some_and(|node| node.loan_quantities.contains_key(uid))
    }
}

fn control_stop(limit: nucleus::execution::control::Limit) -> report::Stop {
    use nucleus::execution::control::Limit;
    match limit {
        Limit::Evaluations => report::Stop::RuleEvaluationBudget {},
        Limit::WallTime => report::Stop::WallTimeBudget {},
        Limit::Cancelled => report::Stop::Cancelled {},
        Limit::Restriction(check) => report::Stop::CheckFailed { check },
        Limit::Evidence => report::Stop::EvidenceBudget {},
    }
}

pub(crate) fn resolve(
    action: &engine::actions::Action,
    captured: &BTreeMap<String, String>,
) -> Result<engine::actions::Action> {
    let reference = |value: &str| -> Result<String> {
        match value.strip_prefix('$') {
            Some(key) => captured
                .get(key)
                .cloned()
                .ok_or_else(|| format!("unresolved seed reference {value}").into()),
            None => Ok(value.into()),
        }
    };
    if let engine::actions::Action::EditRecordText { target, head, body } = action {
        return Ok(engine::actions::Action::EditRecordText {
            target: reference(target)?,
            head: head.clone(),
            body: body.clone(),
        });
    }
    if let engine::actions::Action::ChangeRecord { request } = action {
        let mut request = request.clone();
        request.record_uid = reference(&request.record_uid)?;
        match &mut request.mutation {
            engine::record_change::Mutation::Assertion {
                predicate,
                object,
                unit,
                ..
            } => {
                *predicate = reference(predicate)?;
                *object = object.as_deref().map(&reference).transpose()?;
                *unit = unit.as_deref().map(&reference).transpose()?;
            }
            engine::record_change::Mutation::NumberAssertion { predicate, .. } => {
                *predicate = reference(predicate)?
            }
            engine::record_change::Mutation::RetractAssertion { assertion } => {
                *assertion = reference(assertion)?
            }
            _ => {}
        }
        return Ok(engine::actions::Action::ChangeRecord { request });
    }
    fn visit(value: &mut serde_json::Value, captured: &BTreeMap<String, String>) -> Result<()> {
        match value {
            serde_json::Value::String(text) if text.starts_with('$') => {
                *text = captured
                    .get(&text[1..])
                    .ok_or_else(|| format!("unresolved seed reference {text}"))?
                    .clone();
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, captured)?;
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    visit(value, captured)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut value = serde_json::to_value(action)?;
    visit(&mut value, captured)?;
    match action {
        engine::actions::Action::CreateRecurrence { .. }
        | engine::actions::Action::ReviseRecurrence { .. } => {
            if let Some(source) = value["condition"].as_str() {
                value["condition"] = resolve_source(source, captured)?.into();
            }
        }
        engine::actions::Action::ReviseKarmaField { .. } => {
            value["source"] = resolve_source(
                value["source"].as_str().ok_or("missing Rule source")?,
                captured,
            )?
            .into();
        }
        engine::actions::Action::SaveKarmaRule { .. } => {
            for field in value["fields"]
                .as_array_mut()
                .ok_or("missing Rule fields")?
            {
                if let Some(source) = field["source"].as_str() {
                    field["source"] = resolve_source(source, captured)?.into();
                }
            }
        }
        _ => {}
    }
    if let engine::actions::Action::CreateTransferMessage { body, .. } = action
        && let Some(shared) = report::sharing::Shared::parse(body)
    {
        let mut shared = serde_json::to_value(shared)?;
        visit(&mut shared, captured)?;
        value["body"] = serde_json::to_string(&shared)?.into();
    }
    Ok(serde_json::from_value(value)?)
}

fn resolve_source(source: &str, captured: &BTreeMap<String, String>) -> Result<String> {
    let mut result = String::new();
    let mut index = 0;
    let mut quoted = false;
    let mut escaped = false;
    while index < source.len() {
        let rest = &source[index..];
        if !quoted && rest.starts_with("@{") {
            let end = rest
                .find('}')
                .ok_or("unterminated captured Rule reference")?;
            let name = &rest[2..end];
            let uid = captured
                .get(name)
                .ok_or_else(|| format!("unresolved Rule reference {name}"))?;
            result.push('@');
            result.push_str(uid);
            index += end + 1;
            continue;
        }
        let character = rest.chars().next().ok_or("invalid Rule source")?;
        result.push(character);
        index += character.len_utf8();
        if quoted && escaped {
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        }
    }
    Ok(result)
}

async fn quantities(engine: &Engine) -> Result<(BTreeMap<String, Quantity>, BTreeSet<String>)> {
    let rows = store::sqlx::query(
        "SELECT uid, quantity_mantissa, quantity_scale, unit_uid, deleted_at FROM record",
    )
    .fetch_all(&engine.store.pool)
    .await?;
    let mut quantities = BTreeMap::new();
    let mut deleted = BTreeSet::new();
    for row in rows {
        let uid: String = row.try_get("uid")?;
        let unit: Option<String> = row.try_get("unit_uid")?;
        if row.try_get::<Option<String>, _>("deleted_at")?.is_some() {
            deleted.insert(uid.clone());
        }
        quantities.insert(
            uid,
            Quantity {
                value: store::exact::read_decimal(&row, "quantity")?,
                unit: unit
                    .map(|uid| TypedUid::new(ReferenceKind::Unit, uid))
                    .transpose()?,
            },
        );
    }
    Ok((quantities, deleted))
}

fn refusal(error: &EngineError) -> Option<Refusal> {
    match error {
        EngineError::Store(store::StoreError::Protocol(message))
            if message.starts_with("hard stock limit:") =>
        {
            Some(Refusal::Conflict {
                code: "hard_stock_limit".into(),
            })
        }
        EngineError::Store(store::StoreError::Protocol(message))
            if message.starts_with("karma_binding_") =>
        {
            Some(Refusal::Conflict {
                code: message
                    .split(':')
                    .next()
                    .unwrap_or("karma_binding_unavailable")
                    .into(),
            })
        }
        EngineError::Forbidden(_) => Some(Refusal::Forbidden {}),
        EngineError::UnknownRecord(reference) => Some(Refusal::UnknownRecord {
            reference: reference.clone(),
        }),
        EngineError::Conflict { code, .. } => Some(Refusal::Conflict {
            code: (*code).into(),
        }),
        EngineError::Consequence(_) | EngineError::Nucleus(_) => Some(Refusal::InvalidAction {}),
        _ => None,
    }
}

fn transfer_refusal(refusal: cell::transfer::Refusal) -> Result<Refusal> {
    Ok(match refusal {
        cell::transfer::Refusal::Forbidden => Refusal::Forbidden {},
        cell::transfer::Refusal::BadRequest => Refusal::InvalidAction {},
        cell::transfer::Refusal::NotFound
        | cell::transfer::Refusal::Conflict
        | cell::transfer::Refusal::Gone => Refusal::Conflict {
            code: format!("transfer_{refusal:?}").to_lowercase(),
        },
        cell::transfer::Refusal::Internal => {
            return Err("Transfer handler failed internally".into());
        }
    })
}

fn error_category(error: &EngineError) -> report::ExecutionError {
    match error {
        EngineError::Store(_) => report::ExecutionError::Storage,
        EngineError::Io(_) => report::ExecutionError::Io,
        EngineError::Json(_) => report::ExecutionError::Serialization,
        _ => report::ExecutionError::Domain,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleCommit {
    rule: String,
    revision: u64,
    event: String,
    intended_at: String,
    consequence: u32,
}

async fn fact_cause(engine: &Engine, fact: &nucleus::Fact) -> Result<Cause> {
    if fact.cause.kind != nucleus::CauseKind::Rule {
        if let Some(cause) = engine::projection::entry_cause(&engine.store.pool, &fact.uid).await? {
            return Ok(cause);
        }
        return Ok(Cause::Fact {
            cause_kind: fact.cause.kind,
            uid: fact.cause.uid.clone(),
        });
    }
    let cause: RuleCommit = serde_json::from_str(
        fact.payload
            .as_deref()
            .ok_or("rule Fact has no occurrence evidence")?,
    )?;
    let frequency: Option<String> = store::sqlx::query_scalar("SELECT frequency_uid FROM karma_rule_application WHERE event_id = ? AND rule_uid = ? AND rule_revision = ?")
        .bind(&cause.event).bind(&cause.rule).bind(cause.revision as i64).fetch_one(&engine.store.pool).await?;
    Ok(Cause::Rule {
        occurrence: report::RuleOccurrence {
            rule_uid: cause.rule,
            revision: cause.revision,
            event_id: cause.event,
            frequency: frequency
                .map(|uid| TypedUid::new(ReferenceKind::Frequency, uid))
                .transpose()?,
            intended_at_ms: Some(
                chrono::DateTime::parse_from_rfc3339(&cause.intended_at)?.timestamp_millis(),
            ),
        },
        consequence: cause.consequence,
    })
}

pub(crate) fn elapsed(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
