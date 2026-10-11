pub mod endpoint;

use crate::config::Secret;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    ConfigureEndpoint {
        settings: endpoint::Settings,
        credential: Option<Secret>,
        password: Option<Secret>,
    },
    UnlockEndpoint {
        password: Secret,
    },
    Inspect,
    StartEndpoint {
        job: String,
        settings: endpoint::Settings,
        audio: Secret,
    },
    Poll {
        job: String,
    },
    Cancel {
        job: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub endpoint: Option<endpoint::Settings>,
    pub ready: bool,
    pub detail: String,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub pending: bool,
    pub text: Option<String>,
    pub error: Option<String>,
}

pub fn validate_audio(audio: &str) -> Result<(), String> {
    if audio.len() > 5_200_000 {
        return Err("Dictation is limited to two minutes of mono speech.".into());
    }
    let bytes = BASE64
        .decode(audio)
        .map_err(|_| "Invalid dictation audio.")?;
    let reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|_| "Dictation requires a PCM WAV recording.")?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > 1_920_000
    {
        return Err(
            "Dictation requires 16 kHz mono PCM, between one sample and two minutes.".into(),
        );
    }
    let expected = reader.duration();
    let count = reader
        .into_samples::<i16>()
        .try_fold(0u32, |count, sample| sample.map(|_| count + 1))
        .map_err(|_| "The microphone recording is incomplete.")?;
    if count != expected {
        return Err("The microphone recording is incomplete.".into());
    }
    Ok(())
}
