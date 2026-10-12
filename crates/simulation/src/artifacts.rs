use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

use nucleus::karma::CanonicalHash;
use nucleus::simulation::{self as report, ArtifactError, ReplayStatus, Version};
use serde::{Deserialize, Serialize};

use crate::checks::Checks;
use crate::scenario::Scenario;
use crate::world::{World, digest};
use crate::{BUILD_HASH, Result};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: Version,
    pub build: CanonicalHash,
    pub scenario: CanonicalHash,
    pub files: BTreeMap<String, CanonicalHash>,
    pub comparison: Comparison,
    pub sources: BTreeMap<String, SourceVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceVersion {
    pub database: CanonicalHash,
    pub state: CanonicalHash,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Matching,
    Changed,
    Generated,
}

pub async fn source_status(
    directory: &Path,
    cell: &str,
    current: &store::Store,
) -> Result<SourceStatus> {
    let bundle = load(directory).map_err(|error| format!("invalid saved run: {error:?}"))?;
    let Some(source) = bundle.manifest.sources.get(cell) else {
        return Ok(SourceStatus::Generated);
    };
    let database = bundle
        .scenario
        .cells
        .iter()
        .find(|candidate| candidate.name == cell)
        .and_then(|candidate| candidate.database.as_ref())
        .ok_or("source database is absent")?;
    if snapshot_state(&directory.join(&database.file)).await? != source.state {
        return Err("saved source version does not match its database snapshot".into());
    }
    Ok(if current.state_hash().await? == source.state {
        SourceStatus::Matching
    } else {
        SourceStatus::Changed
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Comparison {
    #[serde(rename = "domain-events-and-world-state-v1")]
    V1,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub version: Version,
    pub build: CanonicalHash,
    pub scenario: CanonicalHash,
    pub status: ReplayStatus,
}

pub struct Run {
    pub directory: PathBuf,
    pub result: report::Result,
    pub findings: Vec<report::Finding>,
    pub cost: report::RunCost,
    pub records: Vec<crate::proposal::Outcome>,
}

pub fn atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension("pending");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn json_lines<T: Serialize>(path: &Path, values: &[T]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    for value in values {
        serde_json::to_writer(&mut file, value)?;
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    Ok(())
}

pub async fn execute(scenario: Scenario, directory: &Path) -> Result<Run> {
    execute_with_sources(scenario, directory, Path::new(".")).await
}

pub async fn execute_with_sources(
    scenario: Scenario,
    directory: &Path,
    sources: &Path,
) -> Result<Run> {
    let mut session = Session::open(scenario, directory, sources).await?;
    while session.step().await? {}
    session.finish().await
}

pub struct Session {
    pub world: World,
    pub checks: Checks,
    directory: PathBuf,
    manifest: Manifest,
    cost: report::RunCost,
}

impl Session {
    pub async fn open(scenario: Scenario, directory: &Path, sources: &Path) -> Result<Self> {
        Self::open_controlled(scenario, directory, sources, None, None, None).await
    }

    pub async fn open_interruptible(scenario: Scenario, directory: &Path, sources: &Path, control: nucleus::execution::control::Control) -> Result<Self> {
        Self::open_controlled(scenario, directory, sources, None, Some(control), None).await
    }

    pub(crate) async fn open_karma_preview(scenario: Scenario, directory: &Path, sources: &Path, signer: Option<engine::trust::Signer>) -> Result<Self> {
        Self::open_controlled(scenario, directory, sources, None, None, signer).await
    }

    async fn open_controlled(mut scenario: Scenario, directory: &Path, sources: &Path, replay_checkpoint: Option<u64>, control: Option<nucleus::execution::control::Control>, signer: Option<engine::trust::Signer>) -> Result<Self> {
        scenario.validate()?;
        std::fs::create_dir(directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::create_dir(directory.join("seed"))?;
        let source_root = sources.canonicalize()?;
        let mut source_bytes = 0u64;
        let mut source_versions = BTreeMap::new();
        for cell in &mut scenario.cells {
            if let Some(database) = &mut cell.database {
                let original = source_root.join(&database.file).canonicalize()?;
                if !original.starts_with(&source_root) {
                    return Err("seed database escapes the case directory".into());
                }
                source_bytes = source_bytes.saturating_add(std::fs::metadata(&original)?.len());
                if source_bytes > scenario.limits.evidence_bytes {
                    return Err("seed database exceeds the evidence budget".into());
                }
                if std::fs::metadata(format!("{}-wal", original.display()))
                    .is_ok_and(|metadata| metadata.len() != 0)
                {
                    return Err(
                        "seed must be a consistent SQLite snapshot without a live WAL".into(),
                    );
                }
                if file_hash(&original)? != database.hash {
                    return Err("seed database hash does not match".into());
                }
                database.file = format!("seed/{}.input.sqlite", cell.name);
                std::fs::copy(original, directory.join(&database.file))?;
                if file_hash(&directory.join(&database.file))? != database.hash {
                    return Err("seed database changed while copying".into());
                }
                source_versions.insert(
                    cell.name.clone(),
                    SourceVersion {
                        database: database.hash.clone(),
                        state: snapshot_state(&directory.join(&database.file)).await?,
                        at_ms: scenario.start_ms,
                    },
                );
            }
            cell.lingua
                .sort_by(|left, right| left.name.cmp(&right.name));
            for source in &mut cell.lingua {
                let original = source_root.join(&source.file).canonicalize()?;
                if !original.starts_with(&source_root) || !original.is_file() {
                    return Err("Lingua seed must be a file inside the case directory".into());
                }
                source_bytes = source_bytes.saturating_add(std::fs::metadata(&original)?.len());
                if source_bytes > scenario.limits.evidence_bytes {
                    return Err("seed files exceed the evidence budget".into());
                }
                source.file = format!("seed/{}.lingua/{}", cell.name, source.name);
                let destination = directory.join(&source.file);
                std::fs::create_dir_all(
                    destination
                        .parent()
                        .ok_or("Lingua destination has no parent")?,
                )?;
                std::fs::copy(original, &destination)?;
                if file_hash(&destination)? != source.hash {
                    return Err("Lingua seed hash does not match".into());
                }
            }
        }
        atomic(&directory.join("scenario.json"), &scenario)?;
        atomic(
            &directory.join("schema.json"),
            &serde_json::json!({
                "scenario": schemars::schema_for!(Scenario),
                "event": schemars::schema_for!(report::Event),
                "finding": schemars::schema_for!(report::Finding),
                "result": schemars::schema_for!(report::Result),
                "manifest": schemars::schema_for!(Manifest),
                "replay": schemars::schema_for!(Replay)
            }),
        )?;
        json_lines(&directory.join("inputs.jsonl"), &scenario.effective_inputs()?)?;
        let manifest = Manifest {
            version: Version::V1,
            build: CanonicalHash::parse(BUILD_HASH)?,
            scenario: digest(&scenario)?,
            files: BTreeMap::new(),
            comparison: Comparison::V1,
            sources: source_versions,
        };
        atomic(&directory.join("manifest.json"), &manifest)?;
        let started = std::time::Instant::now();
        let mut world = World::open_controlled(scenario, directory, replay_checkpoint, control, signer).await?;
        let execution_micros = crate::world::elapsed(started).saturating_sub(world.evidence_micros);
        let started = std::time::Instant::now();
        let mut checks = Checks::new(&world).await?;
        checks.observe(&mut world).await?;
        Ok(Self {
            world,
            checks,
            directory: directory.into(),
            manifest,
            cost: report::RunCost {
                execution_micros,
                checking_micros: crate::world::elapsed(started),
                ..Default::default()
            },
        })
    }

    pub async fn step(&mut self) -> Result<bool> {
        let previous_steps = self.world.steps;
        while self.step_boundary().await? {
            if self.world.steps > previous_steps {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn step_boundary(&mut self) -> Result<bool> {
        let started = std::time::Instant::now();
        let evidence_before = self.world.evidence_micros;
        let step = self.world.step().await;
        self.cost.execution_micros += crate::world::elapsed(started)
            .saturating_sub(self.world.evidence_micros - evidence_before);
        match step {
            Ok(true) => {
                let started = std::time::Instant::now();
                self.checks.observe(&mut self.world).await?;
                self.cost.checking_micros += crate::world::elapsed(started);
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(error) => {
                atomic(&self.directory.join("diagnostic.json"), &error.to_string())?;
                if self.world.stop.is_none() {
                    return Err(error);
                }
                self.checks.observe(&mut self.world).await?;
                Ok(false)
            }
        }
    }

    pub fn cancel(&mut self) {
        self.world.stop.get_or_insert(report::Stop::Cancelled {});
    }

    pub async fn finish(self) -> Result<Run> {
        if self.world.stop.is_none() {
            return Err("finish requires a completed or stopped run".into());
        }
        let Self {
            mut world,
            mut checks,
            directory,
            mut manifest,
            mut cost,
        } = self;
        let mut selected = std::collections::BTreeSet::new();
        if let Some(proposal) = &world.scenario.proposal {
            selected.extend(proposal.records.iter().map(|record| (proposal.cell.clone(), record.uid.clone())));
        }
        for event in &world.trace {
            if matches!(event.caused_by, report::Cause::Seed {}) {
                continue;
            }
            match &event.observation {
                report::Observation::CommittedQuantity { record, .. }
                | report::Observation::LoanQuantity { record, .. } => { selected.insert((event.cell.clone(), record.as_str().to_owned())); }
                report::Observation::ActionAccepted { created: Some(record), .. } if nucleus::valid_uid(record, "r") => { selected.insert((event.cell.clone(), record.clone())); }
                _ => {}
            }
        }
        let mut records = Vec::new();
        let mut record_bytes = 3u64;
        for (cell, record) in selected {
            let outcome = crate::proposal::Outcome::read(&world.nodes[&cell].engine().store, &cell, &record, world.scenario.start_ms, world.scenario.end_ms).await?;
            let next_bytes = record_bytes.saturating_add(serde_json::to_vec(&outcome)?.len() as u64 + 1);
            if next_bytes > 16 * 1024 * 1024
                || world.evidence_bytes.saturating_add(next_bytes) > world.scenario.limits.evidence_bytes
            {
                world.stop = Some(report::Stop::EvidenceBudget {});
                break;
            }
            record_bytes = next_bytes;
            records.push(outcome);
        }
        let record_bytes = serde_json::to_vec(&records)?.len() as u64;
        world.evidence_bytes = world.evidence_bytes.saturating_add(record_bytes);
        if world.evidence_bytes > world.scenario.limits.evidence_bytes {
            world.stop = Some(report::Stop::EvidenceBudget {});
        }
        atomic(&directory.join("records.json"), &records)?;
        let started = std::time::Instant::now();
        let result = checks.finish(&mut world).await?;
        cost.checking_micros += crate::world::elapsed(started);
        cost.evidence_micros = world.evidence_micros;
        cost.checks = checks.costs.clone();
        atomic(&directory.join("cost.json"), &cost)?;
        atomic(&directory.join("checks.json"), &checks.specifications)?;
        json_lines(&directory.join("findings.jsonl"), &checks.findings)?;
        atomic(
            &directory.join("replay.json"),
            &Replay {
                version: Version::V1,
                build: manifest.build.clone(),
                scenario: manifest.scenario.clone(),
                status: ReplayStatus::Unverified {},
            },
        )?;
        for name in [
            "scenario.json",
            "schema.json",
            "inputs.jsonl",
            "trace.jsonl",
            "checks.json",
            "cost.json",
            "findings.jsonl",
            "records.json",
        ] {
            std::fs::File::open(directory.join(name))?.sync_all()?;
            manifest
                .files
                .insert(name.into(), file_hash(&directory.join(name))?);
        }
        for seed in &world.scenario.cells {
            for source in &seed.lingua {
                std::fs::File::open(directory.join(&source.file))?.sync_all()?;
                let mut parent = directory.join(&source.file);
                parent.pop();
                while parent != directory {
                    std::fs::File::open(&parent)?.sync_all()?;
                    parent.pop();
                }
                manifest
                    .files
                    .insert(source.file.clone(), source.hash.clone());
            }
            if let Some(database) = &seed.database {
                std::fs::File::open(directory.join(&database.file))?.sync_all()?;
                manifest
                    .files
                    .insert(database.file.clone(), database.hash.clone());
            }
            for extension in ["sqlite", "environment.json"] {
                let name = format!("seed/{}.{}", seed.name, extension);
                std::fs::File::open(directory.join(&name))?.sync_all()?;
                manifest
                    .files
                    .insert(name.clone(), file_hash(&directory.join(&name))?);
            }
        }
        std::fs::File::open(directory.join("seed"))?.sync_all()?;
        let mut result_bytes = serde_json::to_vec(&result)?;
        result_bytes.push(b'\n');
        manifest
            .files
            .insert("result.json".into(), bytes_hash(&result_bytes)?);
        atomic(&directory.join("manifest.json"), &manifest)?;
        atomic(&directory.join("result.json"), &result)?;
        for node in world.nodes.values() {
            node.cell.runtime().store.pool.close().await;
        }
        Ok(Run {
            directory: directory.into(),
            result,
            findings: checks.findings,
            cost,
            records,
        })
    }
}

pub fn file_hash(path: &Path) -> Result<CanonicalHash> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(CanonicalHash::parse(format!(
        "sha256:{}",
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))?)
}

fn bytes_hash(bytes: &[u8]) -> Result<CanonicalHash> {
    Ok(CanonicalHash::parse(format!(
        "sha256:{}",
        nucleus::fact::sha256_hex(bytes)
    ))?)
}

fn read<T: serde::de::DeserializeOwned>(
    directory: &Path,
    name: &str,
) -> std::result::Result<T, ArtifactError> {
    bounded_file(directory, name, 16 * 1024 * 1024)?;
    let bytes = std::fs::read(directory.join(name)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ArtifactError::Incomplete { path: name.into() }
        } else {
            ArtifactError::Invalid {
                path: name.into(),
                reason: error.to_string(),
            }
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|error| ArtifactError::Invalid {
        path: name.into(),
        reason: error.to_string(),
    })
}

fn lines<T: serde::de::DeserializeOwned>(
    directory: &Path,
    name: &str,
) -> std::result::Result<Vec<T>, ArtifactError> {
    bounded_file(directory, name, 1024 * 1024 * 1024)?;
    let file = std::fs::File::open(directory.join(name))
        .map_err(|_| ArtifactError::Incomplete { path: name.into() })?;
    let mut reader = std::io::BufReader::new(file);
    let mut rows = Vec::new();
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| ArtifactError::Invalid {
                path: name.into(),
                reason: error.to_string(),
            })?;
        if count == 0 {
            break;
        }
        if count > 16 * 1024 * 1024 {
            return Err(ArtifactError::Invalid {
                path: name.into(),
                reason: "line exceeds evidence limit".into(),
            });
        }
        if line.last() != Some(&b'\n') {
            return Err(ArtifactError::Incomplete { path: name.into() });
        }
        rows.push(
            serde_json::from_slice(&line).map_err(|error| ArtifactError::Invalid {
                path: format!("{name}:{}", rows.len() + 1),
                reason: error.to_string(),
            })?,
        );
    }
    Ok(rows)
}

fn bounded_file(
    directory: &Path,
    name: &str,
    limit: u64,
) -> std::result::Result<(), ArtifactError> {
    let metadata = std::fs::symlink_metadata(directory.join(name))
        .map_err(|_| ArtifactError::Incomplete { path: name.into() })?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(ArtifactError::Invalid {
            path: name.into(),
            reason: "artifact is not a bounded regular file".into(),
        });
    }
    Ok(())
}

pub struct Bundle {
    pub manifest: Manifest,
    pub scenario: Scenario,
    pub events: Vec<report::Event>,
    pub findings: Vec<report::Finding>,
    pub result: report::Result,
    pub cost: report::RunCost,
    pub records: Vec<crate::proposal::Outcome>,
}

pub fn load(directory: &Path) -> std::result::Result<Bundle, ArtifactError> {
    let result: report::Result = read(directory, "result.json")?;
    let manifest: Manifest = read(directory, "manifest.json")?;
    for required in [
        "result.json",
        "scenario.json",
        "inputs.jsonl",
        "trace.jsonl",
        "checks.json",
        "cost.json",
        "records.json",
        "findings.jsonl",
    ] {
        if !manifest.files.contains_key(required) {
            return Err(ArtifactError::Incomplete {
                path: required.into(),
            });
        }
    }
    for (name, expected) in &manifest.files {
        if name.is_empty()
            || Path::new(name)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(ArtifactError::Invalid {
                path: "manifest.files".into(),
                reason: "artifact paths must stay inside the bundle".into(),
            });
        }
        let mut path = directory.to_path_buf();
        for component in Path::new(name).components() {
            path.push(component);
            if std::fs::symlink_metadata(&path)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err(ArtifactError::Invalid {
                    path: name.clone(),
                    reason: "artifact symlinks are not accepted".into(),
                });
            }
        }
        bounded_file(directory, name, 1024 * 1024 * 1024)?;
        let observed =
            file_hash(&path).map_err(|_| ArtifactError::Incomplete { path: name.clone() })?;
        if &observed != expected {
            return Err(ArtifactError::Corrupt {
                path: name.clone(),
                expected: expected.as_str().into(),
                observed: observed.as_str().into(),
            });
        }
    }
    let scenario: Scenario = read(directory, "scenario.json")?;
    scenario
        .validate()
        .map_err(|error| ArtifactError::Invalid {
            path: "scenario.json".into(),
            reason: error.to_string(),
        })?;
    let observed = digest(&scenario).map_err(|error| ArtifactError::Invalid {
        path: "scenario.json".into(),
        reason: error.to_string(),
    })?;
    if observed != manifest.scenario {
        return Err(ArtifactError::Corrupt {
            path: "scenario.json".into(),
            expected: manifest.scenario.as_str().into(),
            observed: observed.as_str().into(),
        });
    }
    let inputs: Vec<crate::scenario::Input> = lines(directory, "inputs.jsonl")?;
    let expected_inputs = scenario.effective_inputs().map_err(|error| ArtifactError::Invalid {
        path: "scenario.json".into(), reason: error.to_string(),
    })?;
    if serde_json::to_value(&inputs).ok() != serde_json::to_value(&expected_inputs).ok() {
        return Err(ArtifactError::Invalid {
            path: "inputs.jsonl".into(),
            reason: "inputs do not match the saved scenario".into(),
        });
    }
    let events: Vec<report::Event> = lines(directory, "trace.jsonl")?;
    let findings: Vec<report::Finding> = lines(directory, "findings.jsonl")?;
    if result.events != events.len() as u64 || result.findings != findings.len() as u64 {
        return Err(ArtifactError::Incomplete {
            path: "result counts".into(),
        });
    }
    let mut messages = BTreeMap::<u64, (&str, &str)>::new();
    let mut completed_messages = std::collections::BTreeSet::new();
    let mut actions = std::collections::BTreeSet::new();
    let mut imported = std::collections::BTreeSet::new();
    let mut interrupted_imports = std::collections::BTreeSet::new();
    let mut cycles = BTreeMap::new();
    for (index, event) in events.iter().enumerate() {
        if event.sequence != index as u64
            || !scenario.cells.iter().any(|cell| cell.name == event.cell)
            || event.virtual_ms < scenario.start_ms
            || event.virtual_ms > scenario.end_ms
            || index > 0 && event.virtual_ms < events[index - 1].virtual_ms
        {
            return Err(ArtifactError::Invalid {
                path: format!("trace.jsonl:{index}"),
                reason: "invalid event sequence, Cell or clock".into(),
            });
        }
        if let report::Cause::Input { id } = &event.caused_by
            && !expected_inputs.iter().any(|input| &input.id == id)
        {
            return Err(ArtifactError::Invalid {
                path: format!("trace.jsonl:{index}.caused_by"),
                reason: "input is missing".into(),
            });
        }
        let invalid = |reason: &str| ArtifactError::Invalid {
            path: format!("trace.jsonl:{index}"),
            reason: reason.into(),
        };
        if let report::Cause::Delivery { message } = event.caused_by
            && !messages.contains_key(&message)
        {
            return Err(invalid("delivery refers to an unqueued message"));
        }
        match &event.observation {
            report::Observation::RuleCycle { cycle } => {
                let contributors: std::collections::BTreeSet<_> = cycle.steps.iter().map(|step| &step.occurrence.rule_uid).collect();
                if cycle.id.is_empty()
                    || cycle.cell != event.cell
                    || cycle.repetitions == 0
                    || cycle.steps.len() < 2
                    || cycle.steps.len() > 65
                    || cycle.rules.is_empty()
                    || cycle.rules.windows(2).any(|pair| pair[0] >= pair[1])
                    || contributors.iter().any(|rule| !cycle.rules.contains(rule))
                    || !cycle.truncated && contributors.len() != cycle.rules.len()
                    || cycle.steps.first().map(|step| &step.occurrence.rule_uid) != cycle.steps.last().map(|step| &step.occurrence.rule_uid)
                    || cycle.steps.iter().any(|step| {
                        !scenario.cells.iter().any(|cell| cell.name == step.cell)
                            || !(scenario.start_ms..=event.virtual_ms).contains(&step.virtual_ms)
                            || step.changes.iter().any(|change| change.record.kind() != nucleus::karma::ReferenceKind::Record)
                            || step.transfer_changes.iter().any(|change| {
                                !scenario.cells.iter().any(|cell| cell.name == change.cell)
                                    || !(step.virtual_ms..=event.virtual_ms).contains(&change.virtual_ms)
                                    || change.before.transfer != change.after.transfer
                                    || change.before == change.after
                                    || change.before.validate().is_err()
                                    || change.after.validate().is_err()
                            })
                    })
                {
                    return Err(invalid("cycle evidence has inconsistent Rules, Cells or times"));
                }
                cycles.insert(cycle.id.clone(), cycle.clone());
            }
            report::Observation::LinguaImported { files, .. }
            | report::Observation::LinguaInterrupted { files } => {
                let sources: BTreeMap<_, _> = scenario
                    .cells
                    .iter()
                    .find(|cell| cell.name == event.cell)
                    .unwrap()
                    .lingua
                    .iter()
                    .map(|file| (file.name.clone(), file.hash.clone()))
                    .collect();
                if sources.is_empty()
                    || *files != sources
                    || !imported.insert(event.cell.as_str())
                    || !matches!(event.caused_by, report::Cause::Seed {})
                    || event.virtual_ms != scenario.start_ms
                {
                    return Err(invalid("Lingua import does not match the authored seed"));
                }
                if matches!(event.observation, report::Observation::LinguaInterrupted { .. }) {
                    interrupted_imports.insert(event.cell.as_str());
                }
            }
            report::Observation::MessageQueued { message, to, .. } => {
                if *message == 0
                    || !scenario.cells.iter().any(|cell| &cell.name == to)
                    || messages.insert(*message, (&event.cell, to)).is_some()
                {
                    return Err(invalid("invalid or repeated message identity"));
                }
            }
            report::Observation::MessageDelivered { message, from, .. }
            | report::Observation::MessageRefused { message, from, .. }
            | report::Observation::TransferReceived { message, from, .. } => {
                if messages.get(message).copied() != Some((from.as_str(), event.cell.as_str()))
                    || !completed_messages.insert(*message)
                {
                    return Err(invalid("message result does not match its queued route"));
                }
            }
            report::Observation::MessageDropped { message, .. } => {
                if !messages.contains_key(message) || !completed_messages.insert(*message) {
                    return Err(invalid("message drop does not match a pending message"));
                }
            }
            report::Observation::ActionAccepted { input, .. }
            | report::Observation::ActionRefused { input, .. }
            | report::Observation::ActionInterrupted { input } => {
                if !actions.insert(input)
                    || !(expected_inputs
                        .iter()
                        .any(|invocation| &invocation.id == input && invocation.cell == event.cell)
                        || scenario.cells.iter().any(|cell| {
                            cell.name == event.cell
                                && cell.seed.iter().any(|invocation| &invocation.id == input)
                        }))
                {
                    return Err(invalid(
                        "Action result refers to a missing or repeated invocation",
                    ));
                }
            }
            report::Observation::LoanQuantity {record,before,after,physical,..} => {
                if record.kind() != nucleus::karma::ReferenceKind::Record || before.unit != after.unit || after.unit != physical.unit {
                    return Err(invalid("loan projection has inconsistent identities or units"));
                }
            }
            report::Observation::CommittedQuantity {
                record,
                before,
                after,
                delta,
                ..
            } => {
                if record.kind() != nucleus::karma::ReferenceKind::Record
                    || before.unit != after.unit
                    || store::exact::sum_exact([before.value, *delta]).ok() != Some(after.value)
                {
                    return Err(invalid(
                        "quantity evidence has inconsistent identities, units or arithmetic",
                    ));
                }
            }
            _ => {}
        }
    }
    let checks: Vec<report::Check> = read(directory, "checks.json")?;
    let check_ids: std::collections::BTreeSet<_> = checks.iter().map(|check| &check.id).collect();
    let coverage_ids: std::collections::BTreeSet<_> = result
        .coverage
        .iter()
        .map(|coverage| &coverage.check)
        .collect();
    if checks.len() != scenario.checks.len()
        || check_ids.len() != checks.len()
        || coverage_ids != check_ids
        || result.coverage.len() != checks.len()
        || result.coverage.iter().any(|coverage| {
            let Some(check) = checks.iter().find(|check| check.id == coverage.check) else {
                return true;
            };
            let failed = findings.iter().any(|finding| finding.check.id == check.id);
            (coverage.status == report::CheckStatus::Skipped) == check.options.enabled
                || (coverage.status == report::CheckStatus::Failed) != failed
                || (coverage.status == report::CheckStatus::Passed
                    && (!coverage.complete
                        || coverage.observations == 0
                        || coverage.reason.is_some()))
                || (coverage.complete && (coverage.observations == 0 || coverage.reason.is_some()))
        })
        || result.coverage.iter().fold(0u64, |total, coverage| {
            total.saturating_add(coverage.observations)
        }) > scenario.checking.evaluations
        || checks.iter().any(|check| {
            !scenario.checks.iter().any(|input| {
                input.id == check.id
                    && input.predicate == check.predicate
                    && input.options == check.options
            })
        })
        || !(scenario.start_ms..=scenario.end_ms).contains(&result.stopped_at_ms)
        || result.inputs > expected_inputs.len() as u64
        || result.rule_evaluations > scenario.limits.rule_evaluations
        || result.execution_checkpoints < result.rule_evaluations
        || (result.stop == report::Stop::RuleEvaluationBudget {}
            && result.rule_evaluations != scenario.limits.rule_evaluations)
        || (result.stop == report::Stop::WallTimeBudget {}
            && (scenario.limits.wall_time_ms.is_none() || result.execution_checkpoints == 0))
        || result.cycles.len() != cycles.len()
        || result.cycles.iter().any(|cycle| cycles.get(&cycle.id) != Some(cycle))
        || !interrupted_imports.is_empty() && !matches!(result.stop, report::Stop::RuleEvaluationBudget {} | report::Stop::WallTimeBudget {} | report::Stop::CheckFailed { .. } | report::Stop::EvidenceBudget {} | report::Stop::Cancelled {})
        || result.execution_interrupted && (result.execution_checkpoints == 0 || !matches!(result.stop, report::Stop::RuleEvaluationBudget {} | report::Stop::WallTimeBudget {} | report::Stop::CheckFailed { .. } | report::Stop::EvidenceBudget {} | report::Stop::Cancelled {}))
        || result.steps > scenario.limits.steps
        || events
            .last()
            .is_some_and(|event| event.virtual_ms > result.stopped_at_ms)
        || (result.stop == report::Stop::HorizonReached {}
            && result.stopped_at_ms != scenario.end_ms)
        || (!findings.is_empty() && result.verdict != report::Verdict::Failed)
        || (findings.is_empty() && result.verdict == report::Verdict::Failed)
        || (result.verdict == report::Verdict::Unverified
            && checks.iter().any(|check| check.options.enabled))
        || (result.verdict == report::Verdict::Passed
            && checks.iter().all(|check| !check.options.enabled))
        || (result.verdict == report::Verdict::Passed
            && (result.stop != report::Stop::HorizonReached {}
                || result.coverage.iter().any(|coverage| {
                    coverage.status != report::CheckStatus::Skipped
                        && (!coverage.complete
                            || coverage.observations == 0
                            || coverage.status != report::CheckStatus::Passed)
                })))
    {
        return Err(ArtifactError::Invalid {
            path: "result.json".into(),
            reason: "result, required checks and coverage disagree".into(),
        });
    }
    for cell in &scenario.cells {
        match (&cell.database, manifest.sources.get(&cell.name)) {
            (Some(database), Some(source))
                if source.database == database.hash && source.at_ms == scenario.start_ms => {}
            (None, None) => {}
            _ => {
                return Err(ArtifactError::Invalid {
                    path: "manifest.sources".into(),
                    reason: "source versions do not match scenario inputs".into(),
                });
            }
        }
        if !cell.lingua.is_empty() && !imported.contains(cell.name.as_str()) {
            return Err(ArtifactError::Incomplete {
                path: format!("trace.jsonl:{}:lingua", cell.name),
            });
        }
        for source in &cell.lingua {
            if manifest.files.get(&source.file) != Some(&source.hash) {
                return Err(ArtifactError::Invalid {
                    path: source.file.clone(),
                    reason: "Lingua seed is not pinned by the manifest".into(),
                });
            }
        }
        if let Some(database) = &cell.database
            && manifest.files.get(&database.file) != Some(&database.hash)
        {
            return Err(ArtifactError::Invalid {
                path: database.file.clone(),
                reason: "seed database is not pinned by the manifest".into(),
            });
        }
        for extension in ["sqlite", "environment.json"] {
            let path = format!("seed/{}.{}", cell.name, extension);
            if !manifest.files.contains_key(&path) {
                return Err(ArtifactError::Incomplete { path });
            }
        }
        let path = format!("seed/{}.environment.json", cell.name);
        let environment: nucleus::execution::State = read(directory, &path)?;
        if environment.now_ms != scenario.start_ms || environment.hlc < 0 {
            return Err(ArtifactError::Invalid {
                path,
                reason: "seed environment has an invalid initial clock".into(),
            });
        }
    }
    if manifest.sources.len()
        != scenario
            .cells
            .iter()
            .filter(|cell| cell.database.is_some())
            .count()
    {
        return Err(ArtifactError::Invalid {
            path: "manifest.sources".into(),
            reason: "source versions contain an unknown Cell".into(),
        });
    }
    for finding in &findings {
        if !checks.contains(&finding.check)
            || (finding.sequence >= events.len() as u64
                && !(events.is_empty() && finding.sequence == 0))
            || finding
                .evidence
                .iter()
                .any(|sequence| *sequence >= events.len() as u64)
        {
            return Err(ArtifactError::Invalid {
                path: "findings.jsonl".into(),
                reason: "finding references missing evidence".into(),
            });
        }
    }
    Ok(Bundle {
        manifest,
        scenario,
        events,
        findings,
        result,
        cost: read(directory, "cost.json")?,
        records: read(directory, "records.json")?,
    })
}

pub async fn replay(directory: &Path, destination: &Path) -> Result<ReplayStatus> {
    let bundle = match load(directory) {
        Ok(bundle) => bundle,
        Err(reason) => return Ok(ReplayStatus::Unavailable { reason }),
    };
    if bundle.manifest.build.as_str() != BUILD_HASH {
        return Ok(ReplayStatus::Unavailable {
            reason: ArtifactError::BuildMismatch {
                expected: bundle.manifest.build.as_str().into(),
                observed: BUILD_HASH.into(),
            },
        });
    }
    if bundle.result.stop == (report::Stop::WallTimeBudget {}) {
        let mut session = Session::open_controlled(bundle.scenario, destination, directory, Some(bundle.result.execution_checkpoints), None, None).await?;
        while session.step().await? {}
        session.finish().await?;
    } else if bundle.result.stop == (report::Stop::Cancelled {}) {
        if bundle.result.execution_interrupted {
            let control = nucleus::execution::control::Control::new(bundle.scenario.limits.rule_evaluations, bundle.scenario.limits.wall_time_ms, None);
            control.set_replay_stop(bundle.result.execution_checkpoints, nucleus::execution::control::Limit::Cancelled);
            let mut session = Session::open_interruptible(bundle.scenario, destination, directory, control).await?;
            while session.step().await? {}
            session.finish().await?;
        } else {
        let mut session = Session::open(bundle.scenario, destination, directory).await?;
        while (session.world.steps < bundle.result.steps
            || session.world.now_ms < bundle.result.stopped_at_ms)
            && session.step().await?
        {}
        session.cancel();
        session.finish().await?;
        }
    } else {
        execute_with_sources(bundle.scenario, destination, directory).await?;
    }
    let repeated =
        load(destination).map_err(|error| format!("new replay bundle is invalid: {error:?}"))?;
    let mut expected_seeds = BTreeMap::new();
    let mut observed_seeds = BTreeMap::new();
    let mut expected_environments = BTreeMap::new();
    let mut observed_environments = BTreeMap::new();
    for cell in &repeated.scenario.cells {
        let filename = format!("seed/{}.sqlite", cell.name);
        expected_seeds.insert(
            cell.name.clone(),
            snapshot_state(&directory.join(&filename)).await?,
        );
        observed_seeds.insert(
            cell.name.clone(),
            snapshot_state(&destination.join(&filename)).await?,
        );
        let environment = format!("seed/{}.environment.json", cell.name);
        let expected: nucleus::execution::State = read(directory, &environment)
            .map_err(|error| format!("invalid seed environment: {error:?}"))?;
        let observed: nucleus::execution::State = read(destination, &environment)
            .map_err(|error| format!("invalid replay environment: {error:?}"))?;
        expected_environments.insert(cell.name.clone(), expected);
        observed_environments.insert(cell.name.clone(), observed);
    }
    let expected = serde_json::json!({"events": bundle.events, "result": bundle.result, "findings": bundle.findings, "records":bundle.records, "seeds": expected_seeds, "environments": expected_environments});
    let observed = serde_json::json!({"events": repeated.events, "result": repeated.result, "findings": repeated.findings, "records":repeated.records, "seeds": observed_seeds, "environments": observed_environments});
    let status = match difference("", &expected, &observed) {
        Some((field, expected, observed)) => {
            let sequence = field
                .strip_prefix(".events[")
                .and_then(|tail| tail.split(']').next())
                .and_then(|number| number.parse().ok())
                .unwrap_or_else(|| {
                    if field.starts_with(".environments.") || field.starts_with(".seeds.") {
                        0
                    } else {
                        bundle.result.events
                    }
                });
            ReplayStatus::Diverged {
                sequence,
                field,
                expected,
                observed,
            }
        }
        None => ReplayStatus::Verified {
            trace: digest(&repeated.events)?,
            reproduced_checks: repeated
                .findings
                .iter()
                .map(|finding| finding.check.id.clone())
                .collect(),
        },
    };
    if matches!(status, ReplayStatus::Diverged { .. }) {
        let mut databases = BTreeMap::new();
        for cell in &repeated.scenario.cells {
            for stage in ["seed", "working"] {
                let file = format!("{stage}/{}.sqlite", cell.name);
                let diagnostic = match database_difference(
                    &directory.join(&file),
                    &destination.join(&file),
                ).await {
                    Ok(value) => value,
                    Err(error) => Some(serde_json::json!({"error":error.to_string()})),
                };
                if let Some(diagnostic) = diagnostic {
                    databases.insert(file, diagnostic);
                }
            }
        }
        atomic(&destination.join("replay-diagnostic.json"), &serde_json::json!({
            "status":status,
            "databases":databases,
            "expected_environments":expected["environments"],
            "observed_environments":observed["environments"],
        }))?;
    }
    atomic(
        &directory.join("replay.json"),
        &Replay {
            version: Version::V1,
            build: bundle.manifest.build,
            scenario: bundle.manifest.scenario,
            status: status.clone(),
        },
    )?;
    Ok(status)
}

async fn database_difference(expected: &Path, observed: &Path) -> Result<Option<serde_json::Value>> {
    async fn tables(path: &Path) -> Result<serde_json::Value> {
        let options = store::sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .read_only(true);
        let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let store = store::Store { pool };
        let result = store.logical_snapshot().await;
        store.pool.close().await;
        let tables: BTreeMap<_, _> = result?.into_iter()
            .map(|table| (table.name.clone(), table))
            .collect();
        Ok(serde_json::to_value(tables)?)
    }
    Ok(difference("", &tables(expected).await?, &tables(observed).await?)
        .map(|(field, expected, observed)| serde_json::json!({
            "field":field,
            "expected":expected.chars().take(4096).collect::<String>(),
            "observed":observed.chars().take(4096).collect::<String>(),
        })))
}

async fn snapshot_state(path: &Path) -> Result<CanonicalHash> {
    let options = store::sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true);
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let store = store::Store { pool };
    let hash = store.state_hash().await?;
    store.pool.close().await;
    Ok(hash)
}

fn difference(
    path: &str,
    expected: &serde_json::Value,
    observed: &serde_json::Value,
) -> Option<(String, String, String)> {
    if expected == observed {
        return None;
    }
    match (expected, observed) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            for key in left.keys().chain(right.keys()) {
                if let Some(difference) = difference(
                    &format!("{path}.{key}"),
                    left.get(key).unwrap_or(&serde_json::Value::Null),
                    right.get(key).unwrap_or(&serde_json::Value::Null),
                ) {
                    return Some(difference);
                }
            }
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                if let Some(difference) = difference(&format!("{path}[{index}]"), left, right) {
                    return Some(difference);
                }
            }
            let index = left.len().min(right.len());
            if left.len() != right.len() {
                return Some((
                    format!("{path}[{index}]"),
                    left.get(index)
                        .map_or_else(|| "<missing>".into(), ToString::to_string),
                    right
                        .get(index)
                        .map_or_else(|| "<missing>".into(), ToString::to_string),
                ));
            }
        }
        _ => {}
    }
    Some((path.into(), expected.to_string(), observed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{database_difference, difference};
    use serde_json::json;

    #[test]
    fn replay_reports_the_first_change_even_when_event_counts_differ() {
        let expected = json!([{"quantity":"10"}, {"quantity":"7"}]);
        let changed = json!([{"quantity":"9"}]);
        assert_eq!(
            difference(".events", &expected, &changed).unwrap().0,
            ".events[0].quantity"
        );
        let truncated = json!([{"quantity":"10"}]);
        let missing = difference(".events", &expected, &truncated).unwrap();
        assert_eq!(missing.0, ".events[1]");
        assert_eq!(missing.2, "<missing>");
    }

    #[tokio::test]
    async fn database_diagnostics_name_changed_authority_rows() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.sqlite");
        let copied = directory.path().join("copied.sqlite");
        let store = store::Store::open(&format!("sqlite://{}", original.display())).await.unwrap();
        store.snapshot_into(&copied).await.unwrap();
        store.pool.close().await;
        let changed = store::Store::open_existing_durable(&format!("sqlite://{}", copied.display())).await.unwrap();
        store::sqlx::query("UPDATE signal_endpoint_authority SET audience = ? WHERE id = 1")
            .bind("different-authority")
            .execute(&changed.pool)
            .await
            .unwrap();
        changed.pool.close().await;
        let diagnostic = database_difference(&original, &copied).await.unwrap().unwrap();
        assert!(diagnostic["field"].as_str().unwrap().starts_with(".signal_endpoint_authority.rows[0]"));
        assert!(diagnostic["observed"].as_str().unwrap().contains("different-authority"));
        assert!(database_difference(&original, &original).await.unwrap().is_none());
    }
}

pub fn next_directory(root: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    for index in 1u64.. {
        let path = root.join(format!("run{index:04}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("run directory counter exhausted".into())
}
