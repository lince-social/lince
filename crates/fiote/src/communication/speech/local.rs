use crate::communication::models::{self, Artifact, Cache, Manifest};
use phonon::engine::{Engine, Precision};
use serde::Serialize;
use std::{
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub const MANIFEST: Manifest = Manifest {
    id: "phonon-2",
    repository: "FermionResearch/Phonon-2",
    revision: "7e153e4054c0a10db6d47d6fff5c062d4c77c154",
    license: "CC-BY-4.0",
    artifact: Artifact {
        name: "phonon-2.bps.tar.zst",
        bytes: 163_515_201,
        sha256: "98125795b6dda72f5c6eee9ba33d19815df65dcb18b50a357bf9f73c9935309e",
    },
};

pub const CODE_LICENSE: &str = include_str!("../../../assets/phonon/LICENSE-CODE-Apache-2.0.txt");
pub const MODEL_LICENSE: &str =
    include_str!("../../../assets/phonon/LICENSE-WEIGHTS-CC-BY-4.0.txt");
pub const NOTICE: &str = include_str!("../../../assets/phonon/NOTICE");

const CONTAINER: Artifact = Artifact {
    name: "model.fermion",
    bytes: 177_438_361,
    sha256: "4b6bfa3a12cc3c4e0a54f2ab3ec4ca7a842b09e5c7ecfc8e7ca0ac6cc8c11468",
};
const CONFIG: Artifact = Artifact {
    name: "config.json",
    bytes: 277_493,
    sha256: "d0daad3b2a182893844f4abdc11e4f5b7083f7d42c8ad7e8203f71559785a31b",
};

#[derive(Debug, Serialize)]
pub struct Measurements {
    pub model_revision: &'static str,
    pub cold_load_ms: u128,
    pub transcription_ms: u128,
    pub audio_ms: usize,
    pub frontend_threads: usize,
    pub compute_threads: usize,
}

#[derive(Debug, Serialize)]
pub struct Transcript {
    pub text: String,
    pub measurements: Measurements,
}

pub struct Recognizer {
    pub cache: Cache,
    engine: Mutex<Option<Engine>>,
    pool: rayon::ThreadPool,
    permit: Arc<tokio::sync::Semaphore>,
    loaded: AtomicBool,
}

impl Recognizer {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let threads = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(4);
        Self::with_threads(root, threads)
    }

    pub(crate) fn with_threads(root: PathBuf, threads: usize) -> Result<Self, String> {
        if !(1..=16).contains(&threads) {
            return Err("Local speech supports between one and sixteen compute threads.".into());
        }
        Ok(Self {
            cache: Cache::new(root, MANIFEST)?,
            engine: Mutex::new(None),
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|index| format!("fiote-speech-{index}"))
                .build()
                .map_err(|error| error.to_string())?,
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
            loaded: AtomicBool::new(false),
        })
    }

    pub fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    pub fn busy(&self) -> bool {
        self.permit.available_permits() == 0
    }

    pub fn unload(&self) {
        *self
            .engine
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
        self.loaded.store(false, Ordering::Release);
    }

    pub fn submit(
        self: &Arc<Self>,
        audio: crate::config::Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<tokio::task::JoinHandle<Result<String, String>>, String> {
        let permit = self.permit.clone().try_acquire_owned().map_err(
            |_| "Local speech is still finishing a previous operation. Try again when it is idle.",
        )?;
        let recognizer = self.clone();
        Ok(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let samples = super::decode_audio(&audio.0)?;
            recognizer
                .transcribe(&samples, &cancelled)
                .map(|result| result.text)
        }))
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<Transcript, String> {
        self.transcribe_checked(samples, cancelled, true)
    }

    pub(crate) fn transcribe_unfiltered(
        &self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<Transcript, String> {
        self.transcribe_checked(samples, cancelled, false)
    }

    fn transcribe_checked(
        &self,
        samples: &[f32],
        cancelled: &AtomicBool,
        require_speech: bool,
    ) -> Result<Transcript, String> {
        if samples.is_empty()
            || samples.len() > 1_920_000
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err(
                "Local dictation requires finite mono 16 kHz samples, up to two minutes.".into(),
            );
        }
        if require_speech && samples.iter().all(|sample| sample.abs() < 0.001) {
            return Err("No speech was recognized. Your draft is unchanged.".into());
        }
        models::check_cancel(cancelled)?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut result = self.run(samples, cancelled)?;
            if require_speech {
                result.text =
                    super::endpoint::transcript(&serde_json::json!({"text": result.text}))?;
            }
            Ok(result)
        })) {
            Ok(result) => result,
            Err(_) => {
                let mut engine = self
                    .engine
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                *engine = None;
                self.engine.clear_poison();
                self.loaded.store(false, Ordering::Release);
                Err("Local speech failed while loading or recognizing audio. Retry to reload it; your draft is unchanged.".into())
            }
        }
    }

    fn run(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<Transcript, String> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| "Local speech must be reloaded after an inference failure.")?;
        let started = Instant::now();
        let cold_load_ms = if engine.is_none() {
            let files = model_files(&self.cache)?;
            models::check_cancel(cancelled)?;
            *engine = Some(
                self.pool
                    .install(|| Engine::from_files(files, false, Precision::F32))
                    .map_err(|error| format!("Cannot load local English speech: {error}"))?,
            );
            self.loaded.store(true, Ordering::Release);
            started.elapsed().as_millis()
        } else {
            0
        };
        let engine = engine.as_ref().ok_or("Local speech did not load.")?;
        models::check_cancel(cancelled)?;
        let started = Instant::now();
        let mut words = Vec::new();
        for chunk in phonon::audio::chunk(0, samples, 20.0) {
            models::check_cancel(cancelled)?;
            let output = self
                .pool
                .install(|| engine.run(&[chunk], 1))
                .map_err(|error| format!("Local speech recognition failed: {error}"))?;
            models::check_cancel(cancelled)?;
            for word in output.into_iter().flatten() {
                words.push(word.word);
            }
        }
        let text = words.join(" ");
        Ok(Transcript {
            text,
            measurements: Measurements {
                model_revision: MANIFEST.revision,
                cold_load_ms,
                transcription_ms: started.elapsed().as_millis(),
                audio_ms: samples.len() / 16,
                frontend_threads: self.pool.current_num_threads(),
                compute_threads: candle_core::utils::get_num_threads(),
            },
        })
    }
}

fn model_files(cache: &Cache) -> Result<phonon::fermion::ModelFiles, String> {
    let reader = ruzstd::decoding::StreamingDecoder::new(cache.verified_file()?)
        .map_err(|error| format!("Invalid speech archive: {error}"))?;
    let mut archive = tar::Archive::new(reader.take(268_435_456));
    let mut container = None;
    let mut config = None;
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|error| error.to_string())?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let (artifact, target) = match name {
            "model.fermion" => (CONTAINER, &mut container),
            "config.json" => (CONFIG, &mut config),
            _ => continue,
        };
        if target.is_some() || entry.size() != artifact.bytes {
            return Err(format!(
                "Speech archive file {} is duplicated or has size {}, expected {}.",
                artifact.name,
                entry.size(),
                artifact.bytes
            ));
        }
        let mut bytes = Vec::with_capacity(artifact.bytes as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 != artifact.bytes || models::digest(&bytes) != artifact.sha256 {
            return Err("Speech model content does not match the registered checksum.".into());
        }
        *target = Some(bytes);
    }
    phonon::fermion::from_parts(
        container.ok_or("Speech archive has no model container.")?,
        &config.ok_or("Speech archive has no model configuration.")?,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_silent_and_cancelled_audio_never_loads_or_downloads_weights() {
        let root = tempfile::tempdir().unwrap();
        let recognizer = Recognizer::new(root.path().to_path_buf()).unwrap();
        let cancelled = AtomicBool::new(false);
        assert!(recognizer.transcribe(&[], &cancelled).is_err());
        assert!(recognizer.transcribe(&[f32::NAN], &cancelled).is_err());
        assert!(
            recognizer
                .transcribe(&vec![0.0; 16_000], &cancelled)
                .is_err()
        );
        cancelled.store(true, Ordering::Release);
        assert!(
            recognizer
                .transcribe(&[0.2], &cancelled)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(!recognizer.loaded());
        assert!(!recognizer.cache.present());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "Requires LINCE_SPEECH_TEST_CACHE and LINCE_SPEECH_TEST_WAV with installed Phonon weights and the public JFK fixture"]
    fn real_offline_english_fixture_recognizes_reference_words_and_reuses_loaded_engine() {
        let cache =
            std::env::var_os("LINCE_SPEECH_TEST_CACHE").expect("Choose an installed model cache");
        let wav =
            std::env::var_os("LINCE_SPEECH_TEST_WAV").expect("Choose the mono 16 kHz JFK fixture");
        let reader = hound::WavReader::open(wav).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        let samples = reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / 32768.0)
            .collect::<Vec<_>>();
        let recognizer = Recognizer::new(cache.into()).unwrap();
        for attempt in 0..2 {
            let result = recognizer
                .transcribe(&samples, &AtomicBool::new(false))
                .unwrap();
            let normalized = result
                .text
                .to_ascii_lowercase()
                .chars()
                .map(|character| {
                    if character.is_ascii_alphabetic() || character.is_whitespace() {
                        character
                    } else {
                        ' '
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(normalized.contains("my fellow americans"), "{normalized}");
            assert!(
                normalized.contains("ask not what your country can do for you"),
                "{normalized}"
            );
            assert!(
                normalized.contains("ask what you can do for your country"),
                "{normalized}"
            );
            assert!(recognizer.loaded());
            if attempt == 1 {
                assert_eq!(result.measurements.cold_load_ms, 0);
            }
            println!("{}", serde_json::to_string(&result).unwrap());
        }
    }
}
