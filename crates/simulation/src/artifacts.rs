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
}

impl Session {
    pub async fn open(mut scenario: Scenario, directory: &Path, sources: &Path) -> Result<Self> {
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
        json_lines(&directory.join("inputs.jsonl"), &scenario.inputs)?;
        let manifest = Manifest {
            version: Version::V1,
            build: CanonicalHash::parse(BUILD_HASH)?,
            scenario: digest(&scenario)?,
            files: BTreeMap::new(),
            comparison: Comparison::V1,
        };
        atomic(&directory.join("manifest.json"), &manifest)?;
        let world = World::open(scenario, directory).await?;
        let mut checks = Checks::new(&world).await?;
        checks.observe(&world).await?;
        Ok(Self {
            world,
            checks,
            directory: directory.into(),
            manifest,
        })
    }

    pub async fn step(&mut self) -> Result<bool> {
        match self.world.step().await {
            Ok(true) => {
                self.checks.observe(&self.world).await?;
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(error) => {
                atomic(&self.directory.join("diagnostic.json"), &error.to_string())?;
                if self.world.stop.is_none() {
                    return Err(error);
                }
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
            world,
            mut checks,
            directory,
            mut manifest,
        } = self;
        let result = checks.finish(&world).await?;
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
            "findings.jsonl",
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
    if serde_json::to_value(&inputs).ok() != serde_json::to_value(&scenario.inputs).ok() {
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
            && !scenario.inputs.iter().any(|input| &input.id == id)
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
            report::Observation::LinguaImported { files, .. } => {
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
            | report::Observation::ActionRefused { input, .. } => {
                if !actions.insert(input)
                    || !(scenario
                        .inputs
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
        || checks.iter().any(|check| {
            !scenario
                .checks
                .iter()
                .any(|input| input.id == check.id && input.predicate == check.predicate)
        })
        || !(scenario.start_ms..=scenario.end_ms).contains(&result.stopped_at_ms)
        || result.inputs > scenario.inputs.len() as u64
        || result.steps > scenario.limits.steps
        || events
            .last()
            .is_some_and(|event| event.virtual_ms > result.stopped_at_ms)
        || (result.stop == report::Stop::HorizonReached {}
            && result.stopped_at_ms != scenario.end_ms)
        || (!findings.is_empty() && result.verdict != report::Verdict::Failed)
        || (findings.is_empty() && result.verdict == report::Verdict::Failed)
        || (result.verdict == report::Verdict::Passed
            && (result.stop != report::Stop::HorizonReached {}
                || result
                    .coverage
                    .iter()
                    .any(|coverage| !coverage.complete || coverage.observations == 0)))
    {
        return Err(ArtifactError::Invalid {
            path: "result.json".into(),
            reason: "result, required checks and coverage disagree".into(),
        });
    }
    for cell in &scenario.cells {
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
    for finding in &findings {
        if !checks.contains(&finding.check)
            || finding.sequence >= events.len() as u64
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
    if bundle.result.stop == (report::Stop::Cancelled {}) {
        let mut session = Session::open(bundle.scenario, destination, directory).await?;
        while session.world.steps < bundle.result.steps && session.step().await? {}
        session.cancel();
        session.finish().await?;
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
    let expected = serde_json::json!({"events": bundle.events, "result": bundle.result, "findings": bundle.findings, "seeds": expected_seeds, "environments": expected_environments});
    let observed = serde_json::json!({"events": repeated.events, "result": repeated.result, "findings": repeated.findings, "seeds": observed_seeds, "environments": observed_environments});
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
    use super::difference;
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
