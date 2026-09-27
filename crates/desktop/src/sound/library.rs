use super::dsp::Effects;
use std::path::{Path, PathBuf};

pub const MAX_SECONDS: u32 = 120;
const MAX_SAMPLES: usize = 23_040_000;

#[derive(Clone)]
pub struct Clip {
    pub samples: Vec<f32>,
    pub rate: u32,
}

pub fn valid_path(path: &str) -> bool {
    let Some(name) = path.strip_prefix("recordings/") else {
        return false;
    };
    name.len() <= 180
        && name.ends_with(".wav")
        && name.len() > 4
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        && !name.starts_with('.')
}

pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(error)?;
        let base = directory.canonicalize().map_err(error)?;
        let root = directory.join("recordings");
        std::fs::create_dir_all(&root).map_err(error)?;
        let root = root.canonicalize().map_err(error)?;
        if !root.starts_with(&base) {
            return Err("Recordings directory is outside Lince".into());
        }
        let originals = root.join(".originals");
        std::fs::create_dir_all(&originals).map_err(error)?;
        if !originals.canonicalize().map_err(error)?.starts_with(&root) {
            return Err("Original recordings directory is outside Lince".into());
        }
        Ok(Self { root })
    }

    fn path(&self, path: &str, original: bool) -> Result<PathBuf, String> {
        if !valid_path(path) {
            return Err("Choose a WAV path from recordings/".into());
        }
        let directory = if original {
            self.root.join(".originals")
        } else {
            self.root.clone()
        };
        if !directory
            .canonicalize()
            .map_err(error)?
            .starts_with(&self.root)
        {
            return Err("Sound path is outside recordings".into());
        }
        let result = directory.join(path.trim_start_matches("recordings/"));
        if std::fs::symlink_metadata(&result).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Sound paths cannot be symbolic links".into());
        }
        Ok(result)
    }

    pub fn list(&self) -> Result<Vec<String>, String> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(error)? {
            let entry = entry.map_err(error)?;
            let path = format!("recordings/{}", entry.file_name().to_string_lossy());
            if valid_path(&path) && entry.file_type().map_err(error)?.is_file() {
                paths.push(path);
            }
            if paths.len() >= 2000 {
                break;
            }
        }
        paths.sort();
        Ok(paths)
    }

    pub fn read(&self, path: &str, original: bool) -> Result<Clip, String> {
        let target = self.path(path, original)?;
        let target = if original && !target.exists() {
            self.path(path, false)?
        } else {
            target
        };
        let metadata = std::fs::metadata(&target).map_err(error)?;
        if !metadata.is_file() || metadata.len() > (MAX_SAMPLES as u64 * 4 + 65_536) {
            return Err("Choose a regular WAV file of at most 88 MiB".into());
        }
        let mut reader = hound::WavReader::open(target).map_err(error)?;
        let spec = reader.spec();
        if spec.channels == 0
            || spec.channels > 8
            || !(8000..=192000).contains(&spec.sample_rate)
            || reader.duration() > spec.sample_rate * MAX_SECONDS
            || reader.len() as usize > MAX_SAMPLES
            || !(1..=32).contains(&spec.bits_per_sample)
        {
            return Err("Use a WAV of up to 120 seconds, 8 channels, and 192 kHz".into());
        }
        let samples: Result<Vec<f32>, _> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().collect(),
            hound::SampleFormat::Int => {
                let scale = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|s| s as f32 / scale))
                    .collect()
            }
        };
        let samples = samples.map_err(error)?;
        if samples.is_empty() || samples.iter().any(|s| !s.is_finite()) {
            return Err("The recording is empty or contains invalid samples".into());
        }
        Ok(Clip {
            rate: spec.sample_rate,
            samples: samples
                .chunks_exact(spec.channels as usize)
                .map(|frame| (frame.iter().sum::<f32>() / spec.channels as f32).clamp(-1.0, 1.0))
                .collect(),
        })
    }

    fn write(&self, path: &str, clip: &Clip, original: bool) -> Result<(), String> {
        let target = self.path(path, original)?;
        let mut file = tempfile::NamedTempFile::new_in(target.parent().unwrap()).map_err(error)?;
        let mut writer = hound::WavWriter::new(
            file.as_file_mut(),
            hound::WavSpec {
                channels: 1,
                sample_rate: clip.rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(error)?;
        for sample in &clip.samples {
            writer.write_sample(*sample).map_err(error)?;
        }
        writer.finalize().map_err(error)?;
        file.as_file().sync_all().map_err(error)?;
        file.persist(target).map_err(error)?;
        Ok(())
    }

    pub fn save_recording(&self, path: &str, clip: &Clip) -> Result<(), String> {
        if clip.samples.is_empty() {
            return Err("No microphone samples were captured".into());
        }
        if !(8000..=192000).contains(&clip.rate)
            || clip.samples.len() > clip.rate as usize * MAX_SECONDS as usize
            || clip
                .samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err("Invalid microphone samples".into());
        }
        if self.path(path, false)?.exists() || self.path(path, true)?.exists() {
            return Err("A recording with that name already exists".into());
        }
        self.write(path, clip, true)?;
        self.write(path, clip, false)
    }

    pub fn apply(&self, path: &str, effects: Effects) -> Result<(), String> {
        if !effects.valid() {
            return Err("Invalid effect settings".into());
        }
        let mut clip = self.read(path, true)?;
        if !self.path(path, true)?.exists() {
            self.write(path, &clip, true)?;
        }
        clip.samples = effects.process(&clip.samples, clip.rate);
        self.write(path, &clip, false)
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn suggestions<'a>(paths: &'a [String], query: &str) -> Vec<&'a str> {
    let query = query.trim().to_ascii_lowercase();
    paths
        .iter()
        .filter(|p| p.to_ascii_lowercase().contains(&query))
        .take(8)
        .map(String::as_str)
        .collect()
}

pub fn recording_path(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .take(60)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!(
        "recordings/{}-{}.wav",
        if stem.is_empty() { "take" } else { &stem },
        nucleus::new_uid("audio")
    )
}
