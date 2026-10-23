mod score;

pub use score::{Score, words};

use super::{local, moonshine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suite {
    pub models: Vec<Model>,
    pub fixtures: Vec<Fixture>,
    pub repetitions: usize,
    pub timeout_seconds: u64,
    pub threads: usize,
    pub maximum_wer: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub engine: Engine,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Engine {
    Phonon {
        cache: PathBuf,
    },
    MoonshineStreaming {
        directory: PathBuf,
        repository: String,
        revision: String,
        license: String,
        artifacts: Vec<Artifact>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub id: String,
    pub audio: PathBuf,
    pub sha256: String,
    pub expected: String,
    pub provenance: String,
    #[serde(default)]
    pub require_exact: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Recognition {
    pub text: String,
    pub cold_load_ms: u128,
    pub transcription_ms: u128,
}

pub trait Backend {
    fn recognize(&mut self, samples: &[f32], cancelled: &AtomicBool)
    -> Result<Recognition, String>;
}

impl Backend for local::Recognizer {
    fn recognize(
        &mut self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<Recognition, String> {
        let transcript = self.transcribe_unfiltered(samples, cancelled)?;
        Ok(Recognition {
            text: transcript.text,
            cold_load_ms: transcript.measurements.cold_load_ms,
            transcription_ms: transcript.measurements.transcription_ms,
        })
    }
}

impl Backend for moonshine::Recognizer {
    fn recognize(
        &mut self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<Recognition, String> {
        let started = Instant::now();
        let text = self.transcribe(samples, cancelled)?;
        Ok(Recognition {
            text,
            cold_load_ms: 0,
            transcription_ms: started.elapsed().as_millis(),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct Sample {
    pub fixture: String,
    pub repetition: usize,
    pub audio_ms: u128,
    pub recognition: Option<Recognition>,
    pub score: Option<Score>,
    pub real_time_factor: Option<f64>,
    pub process_rss_bytes: Option<u64>,
    pub passed: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ModelReport {
    pub model: Model,
    pub provenance: serde_json::Value,
    pub initialization_ms: u128,
    pub cold_load_ms: u128,
    pub elapsed_ms: u128,
    pub initialization_rss_bytes: Option<u64>,
    pub peak_process_rss_bytes: Option<u64>,
    pub memory_scope: String,
    pub samples: Vec<Sample>,
    pub score: Score,
    pub completed: bool,
    pub passed: bool,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub suite: Suite,
    pub threads: usize,
    pub normalization: String,
    pub scope: String,
    pub models: Vec<ModelReport>,
    pub passed: bool,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(8_388_609)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 8_388_608 {
        return Err("Evaluation JSON exceeds eight MiB.".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Suite {
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut suite: Self = read_json(path)?;
        suite.validate()?;
        let parent = path.parent().ok_or("Suite needs a parent directory.")?;
        for model in &mut suite.models {
            let path = match &mut model.engine {
                Engine::Phonon { cache } => cache,
                Engine::MoonshineStreaming { directory, .. } => directory,
            };
            *path = parent.join(&*path);
        }
        for fixture in &mut suite.fixtures {
            fixture.audio = parent.join(&fixture.audio);
        }
        Ok(suite)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.models.is_empty()
            || self.models.len() > 32
            || self.fixtures.is_empty()
            || self.fixtures.len() > 256
            || !(1..=10).contains(&self.repetitions)
            || !(1..=3600).contains(&self.timeout_seconds)
            || !(1..=16).contains(&self.threads)
            || self
                .maximum_wer
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(
                "Invalid evaluation size, repeat, timeout, thread or accuracy bounds.".into(),
            );
        }
        let mut identifiers = std::collections::HashSet::new();
        for model in &self.models {
            if !identifier(&model.id) || !identifiers.insert(&model.id) {
                return Err("Model identifiers must be unique safe names.".into());
            }
            if let Engine::MoonshineStreaming {
                repository,
                revision,
                artifacts,
                license,
                ..
            } = &model.engine
            {
                let parts: Vec<_> = repository.split('/').collect();
                if parts.len() != 2
                    || parts.iter().any(|part| {
                        part.is_empty()
                            || !part
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
                    })
                    || revision.len() != 40
                    || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || license.is_empty()
                    || license.len() > 100
                    || artifacts.len() != 3
                {
                    return Err("Moonshine requires a pinned repository revision, license and three artifacts.".into());
                }
                let mut files = std::collections::HashSet::new();
                for artifact in artifacts {
                    if !["config.json", "tokenizer.json", "model.safetensors"]
                        .contains(&artifact.file.as_str())
                        || !files.insert(&artifact.file)
                        || artifact.bytes == 0
                        || artifact.bytes > 2_147_483_648
                        || !digest_valid(&artifact.sha256)
                    {
                        return Err("Invalid Moonshine artifact size, filename or SHA-256.".into());
                    }
                }
            }
        }
        identifiers.clear();
        for fixture in &self.fixtures {
            if !identifier(&fixture.id)
                || !identifiers.insert(&fixture.id)
                || !digest_valid(&fixture.sha256)
                || fixture.expected.len() > 32_768
                || fixture.provenance.is_empty()
                || fixture.provenance.len() > 4096
            {
                return Err(
                    "Fixtures need unique names, SHA-256, bounded reference text and provenance."
                        .into(),
                );
            }
        }
        Ok(())
    }
}

fn verify(path: &Path, expected: &str, size: Option<u64>) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() || size.is_some_and(|size| size != metadata.len()) {
        return Err(format!(
            "{}: expected a regular file with the registered size.",
            path.display()
        ));
    }
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let actual: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "{}: SHA-256 mismatch; no inference was attempted.",
            path.display()
        ));
    }
    Ok(())
}

pub async fn fetch(suite: &Suite) -> Result<(), String> {
    suite.validate()?;
    for model in &suite.models {
        match &model.engine {
            Engine::Phonon { cache } => {
                let recognizer = local::Recognizer::new(cache.clone())?;
                if recognizer.cache.verified_file().is_err() {
                    recognizer
                        .cache
                        .acquire(
                            None,
                            Arc::new(AtomicBool::new(false)),
                            Arc::new(AtomicU64::new(0)),
                        )
                        .await?;
                }
            }
            Engine::MoonshineStreaming {
                directory,
                repository,
                revision,
                artifacts,
                ..
            } => {
                std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                for artifact in artifacts {
                    let path = directory.join(&artifact.file);
                    if verify(&path, &artifact.sha256, Some(artifact.bytes)).is_ok() {
                        continue;
                    }
                    let mut temporary = tempfile::NamedTempFile::new_in(directory)
                        .map_err(|error| error.to_string())?;
                    let url = format!(
                        "https://huggingface.co/{repository}/resolve/{revision}/{}",
                        artifact.file
                    );
                    let client = reqwest::Client::builder()
                        .connect_timeout(Duration::from_secs(20))
                        .timeout(Duration::from_secs(1800))
                        .build()
                        .map_err(|error| error.to_string())?;
                    let mut response = client
                        .get(url)
                        .send()
                        .await
                        .map_err(|error| error.to_string())?
                        .error_for_status()
                        .map_err(|error| error.to_string())?;
                    let mut bytes = 0;
                    while let Some(chunk) =
                        response.chunk().await.map_err(|error| error.to_string())?
                    {
                        bytes += chunk.len() as u64;
                        if bytes > artifact.bytes {
                            return Err("Model download exceeds registered size.".into());
                        }
                        temporary
                            .write_all(&chunk)
                            .map_err(|error| error.to_string())?;
                    }
                    verify(temporary.path(), &artifact.sha256, Some(artifact.bytes))?;
                    temporary
                        .as_file()
                        .sync_all()
                        .map_err(|error| error.to_string())?;
                    temporary
                        .persist(&path)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }
    Ok(())
}

fn load(model: &Model, threads: usize) -> Result<Box<dyn Backend>, String> {
    match &model.engine {
        Engine::Phonon { cache } => Ok(Box::new(local::Recognizer::with_threads(
            cache.clone(),
            threads,
        )?)),
        Engine::MoonshineStreaming {
            directory,
            artifacts,
            ..
        } => {
            for artifact in artifacts {
                verify(
                    &directory.join(&artifact.file),
                    &artifact.sha256,
                    Some(artifact.bytes),
                )?;
            }
            Ok(Box::new(moonshine::Recognizer::load(directory)?))
        }
    }
}

fn audio(fixture: &Fixture) -> Result<Vec<f32>, String> {
    verify(&fixture.audio, &fixture.sha256, None)?;
    if std::fs::metadata(&fixture.audio)
        .map_err(|error| error.to_string())?
        .len()
        > 1_000_000
    {
        return Err("Evaluation audio files are limited to one MiB.".into());
    }
    let reader = hound::WavReader::open(&fixture.audio).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000
        || spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > 480_000
    {
        return Err(
            "Evaluation fixtures must be mono PCM16 WAV at 16 kHz, up to 30 seconds.".into(),
        );
    }
    reader
        .into_samples::<i16>()
        .map(|sample| sample.map(|sample| f32::from(sample) / 32768.0))
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())
}

pub fn evaluate(
    backend: &mut dyn Backend,
    fixture: &Fixture,
    samples: &[f32],
    repetition: usize,
) -> Sample {
    let audio_ms = samples.len() as u128 * 1000 / 16_000;
    match backend.recognize(samples, &AtomicBool::new(false)) {
        Ok(recognition) => {
            let score = Score::compare(&fixture.expected, &recognition.text);
            let passed = !fixture.require_exact || score.edits == 0;
            Sample {
                fixture: fixture.id.clone(),
                repetition,
                audio_ms,
                score: Some(score),
                real_time_factor: Some(
                    recognition.transcription_ms as f64 / audio_ms.max(1) as f64,
                ),
                recognition: Some(recognition),
                process_rss_bytes: memory("VmRSS:"),
                passed,
                error: None,
            }
        }
        Err(error) => Sample {
            fixture: fixture.id.clone(),
            repetition,
            audio_ms,
            score: None,
            recognition: None,
            real_time_factor: None,
            process_rss_bytes: memory("VmRSS:"),
            passed: false,
            error: Some(error),
        },
    }
}

fn empty_report(model: &Model) -> ModelReport {
    let provenance = match &model.engine {
        Engine::Phonon { .. } => serde_json::json!({
            "manifest": local::MANIFEST,
            "implementation": "phonon-rs; Candle CPU F32; evaluation bypasses the production silence and draft filters",
        }),
        Engine::MoonshineStreaming {
            repository,
            revision,
            artifacts,
            ..
        } => serde_json::json!({
            "repository": repository, "revision": revision, "artifacts": artifacts,
            "implementation": "Lince Moonshine Streaming Candle CPU F32 port; offline segments; no reference tensor-parity or incremental streaming claim",
        }),
    };
    ModelReport {
        model: model.clone(), provenance, initialization_ms: 0, cold_load_ms: 0, elapsed_ms: 0, initialization_rss_bytes: None,
        peak_process_rss_bytes: None,
        memory_scope: "Linux process VmHWM; CPU-only inference; includes runtime, weights and allocator peak, excludes other processes and kernel file cache. Unavailable on other systems.".into(),
        samples: Vec::new(), score: Score::default(), completed: false, passed: false,
        timed_out: false, exit_code: None, error: None,
    }
}

fn memory(field: &str) -> Option<u64> {
    process_memory(std::process::id(), field)
}

fn process_memory(pid: u32, field: &str) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })?
        .checked_mul(1024)
}

fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Report needs a parent directory.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(file.as_file_mut(), value).map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn worker(suite: &Suite, index: usize, output: &Path) -> Result<(), String> {
    suite.validate()?;
    let model = suite
        .models
        .get(index)
        .ok_or("Invalid evaluation model index.")?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(suite.threads)
        .build()
        .map_err(|error| error.to_string())?;
    pool.install(|| {
        let mut report = empty_report(model);
        let started = Instant::now();
        let result = (|| {
            let audio = suite
                .fixtures
                .iter()
                .map(audio)
                .collect::<Result<Vec<_>, _>>()?;
            let loading = Instant::now();
            let mut backend = load(model, suite.threads)?;
            report.initialization_ms = loading.elapsed().as_millis();
            report.initialization_rss_bytes = memory("VmRSS:");
            report.cold_load_ms = report.initialization_ms;
            for repetition in 0..suite.repetitions {
                for (fixture, samples) in suite.fixtures.iter().zip(&audio) {
                    let sample = evaluate(backend.as_mut(), fixture, samples, repetition);
                    if let Some(recognition) = &sample.recognition {
                        report.cold_load_ms += recognition.cold_load_ms;
                    }
                    if let Some(score) = &sample.score {
                        report.score.merge(score);
                    }
                    report.samples.push(sample);
                    report.elapsed_ms = started.elapsed().as_millis();
                    report.peak_process_rss_bytes = memory("VmHWM:");
                    save(output, &report)?;
                }
            }
            report.completed = report.samples.iter().all(|sample| sample.error.is_none());
            report.passed = report.completed
                && report.samples.iter().all(|sample| sample.passed)
                && suite
                    .maximum_wer
                    .is_none_or(|maximum| report.score.wer() <= maximum);
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            report.error = Some(error);
        }
        report.elapsed_ms = started.elapsed().as_millis();
        report.peak_process_rss_bytes = memory("VmHWM:");
        save(output, &report)
    })
}

struct Supervision {
    status: Option<ExitStatus>,
    peak_rss_bytes: Option<u64>,
    timed_out: bool,
    error: Option<String>,
}

fn supervise(mut child: Child, timeout: Duration) -> Supervision {
    let started = Instant::now();
    let mut result = Supervision {
        status: None,
        peak_rss_bytes: None,
        timed_out: false,
        error: None,
    };
    loop {
        if let Some(bytes) = process_memory(child.id(), "VmHWM:") {
            result.peak_rss_bytes = Some(result.peak_rss_bytes.unwrap_or(0).max(bytes));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                result.status = Some(status);
                break;
            }
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(50))
            }
            status => {
                result.timed_out = status.is_ok();
                result.error = Some(match status {
                    Err(error) => error.to_string(),
                    _ => "Model worker exceeded its configured timeout.".into(),
                });
                let _ = child.kill();
                result.status = child.wait().ok();
                break;
            }
        }
    }
    result
}

pub fn run(suite: Suite, output: &Path, worker_executable: &Path) -> Result<Report, String> {
    suite.validate()?;
    if output
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("Choose an output directory without parent traversal.".into());
    }
    std::fs::create_dir(output)
        .map_err(|error| format!("Choose a new output directory: {error}"))?;
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let suite_path = output.join("suite.json");
    save(&suite_path, &suite)?;
    let mut report = Report {
        schema: 1, threads: suite.threads,
        normalization: "Lowercase alphanumeric words, punctuation separators, split adjacent letters and digits; zero through ten and mister normalized. Raw hypotheses and references retained. Aggregate word edit distance divided by reference words; silence insertions counted.".into(),
        scope: "Sequential isolated model processes on the same fixture bytes and repeat order. Cache reuse is explicit; cold load includes integrity checks. This small corpus is a regression comparison, not a general accuracy benchmark or a Fiote wake/routing/account/UI test.".into(),
        suite, models: Vec::new(), passed: false,
    };
    for (index, model) in report.suite.models.iter().enumerate() {
        let worker_path = output.join(format!("{}.json", model.id));
        let stdout = File::create(output.join(format!("{}.stdout.log", model.id)))
            .map_err(|error| error.to_string())?;
        let stderr = File::create(output.join(format!("{}.stderr.log", model.id)))
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let child = Command::new(worker_executable)
            .arg("worker")
            .arg(&suite_path)
            .arg(index.to_string())
            .arg(&worker_path)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .env("RAYON_NUM_THREADS", report.suite.threads.to_string())
            .spawn();
        let mut model_report = empty_report(model);
        match child {
            Err(error) => model_report.error = Some(format!("Cannot launch model worker: {error}")),
            Ok(child) => {
                let supervision =
                    supervise(child, Duration::from_secs(report.suite.timeout_seconds));
                if worker_path.exists() {
                    model_report = read_json(&worker_path)?;
                }
                model_report.timed_out = supervision.timed_out;
                if supervision.error.is_some() {
                    model_report.error = supervision.error;
                }
                model_report.exit_code = supervision.status.and_then(|status| status.code());
                model_report.peak_process_rss_bytes = match (
                    model_report.peak_process_rss_bytes,
                    supervision.peak_rss_bytes,
                ) {
                    (Some(saved), Some(observed)) => Some(saved.max(observed)),
                    (saved, observed) => saved.or(observed),
                };
                if supervision.status.is_none_or(|status| !status.success())
                    || model_report.timed_out
                {
                    model_report.completed = false;
                    model_report.passed = false;
                    model_report.error.get_or_insert_with(|| {
                        "Model worker failed; inspect its stderr log.".into()
                    });
                }
            }
        }
        model_report.elapsed_ms = started.elapsed().as_millis();
        eprintln!(
            "{}: completed={}, passed={}, peak_rss={:?}, WER={:.2}%",
            model.id,
            model_report.completed,
            model_report.passed,
            model_report.peak_process_rss_bytes,
            model_report.score.wer() * 100.0
        );
        save(&worker_path, &model_report)?;
        report.models.push(model_report);
        save(&output.join("report.json"), &report)?;
    }
    report.passed = report.models.iter().all(|model| model.passed);
    save(&output.join("report.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests;
