// non-lean-yet
// Scope: mod endpoint, mod evaluation, mod input, mod local.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 236.
// - [Clippy: clippy::cast_possible_truncation] Prove the bounds or use a checked conversion. Sites
//   236.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 16, 23, 32, 45, 92.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   215, 217, 235.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 205, 209.
// - [Clippy: clippy::struct_excessive_bools] more than 3 bools in a struct. Sites 53.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   3, 4, 5, 6, 7, 8, 9, 16, 18, 19, 23, 25, 26, 27, 28, 32, 34, 35, 36, 37, 38, 39, 40, 41, 45,
//   47, 48, 49, 53, 54, 55, 56, 57, 58, 60, 62, 64, 65, 66, 67, 68, 72, 73, 74, 75, 76, 77, 81, 82,
//   83, 84, 85, 86, 87, 88, 92, 93, 94, 96, 97, 98, 99, 100, 102, 103, 105, 106, 107, 108, 109,
//   111, 112, 113, 115, 116, 117, 119, 120, 121, 123, 124, 125, 126, 127, 129, 130, 132, 133, 135,
//   136, 137, 139, 140, 141, 143, 144, 145, 147, 148, 150, 151, 152, 154, 155, 156, 158, 159, 160,
//   161, 162, 164, 165, 167, 168, 173, 175, 177, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188,
//   189, 190, 194, 196, 197, 198, 199, 201, 202, 205, 209.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites decode_audio:
//   String (209), validate_audio: String (205).

pub mod endpoint;
pub mod evaluation;
pub mod input;
pub mod local;
pub mod moonshine;
pub mod recognition;
pub mod segment;
pub mod voice;
pub mod wake;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use crate::config::Secret;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Local,
    Endpoint,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Purpose {
    #[default]
    Transcription,
    ModelSetup,
    Decision,
    DecisionSetup,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListeningStage {
    #[default]
    Off,
    Waiting,
    RequestWindow,
    Transcribing,
    Deciding,
    Working,
    Ignored,
    Degraded,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListeningMode {
    #[default]
    Addressed,
    Wake,
    Model,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListeningStatus {
    pub mode: ListeningMode,
    pub owner: Option<String>,
    pub generation: u64,
    pub enabled: bool,
    pub pending: bool,
    #[serde(default)]
    pub finishing: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub conversation: Option<crate::assistant::Conversation>,
    pub stage: ListeningStage,
    pub detail: String,
    pub receipt: Option<crate::assistant::EntryReceipt>,
    pub unsent: Option<VoiceRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRequest {
    pub owner: String,
    pub generation: u64,
    pub segment: u64,
    pub request: crate::assistant::EntryRequest,
    pub uncertain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStatus {
    pub model: String,
    pub revision: String,
    pub present: bool,
    pub loaded: bool,
    pub busy: bool,
    pub acquired_bytes: u64,
    pub required_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    ConfigureWake {
        profile: Option<wake::Profile>,
    },
    WokenSegment {
        owner: String,
        generation: u64,
        segment: u64,
        audio: Secret,
    },
    Voice {
        request: voice::Request,
    },
    StartListening {
        mode: ListeningMode,
        owner: String,
        target: Option<String>,
        routing: bool,
    },
    StopListening {
        owner: String,
        generation: u64,
    },
    PauseListening {
        owner: String,
        generation: u64,
    },
    ResumeListening {
        owner: String,
        generation: u64,
    },
    ListeningSegment {
        owner: String,
        generation: u64,
        segment: u64,
        audio: Secret,
    },
    SelectMode {
        mode: Mode,
    },
    SelectLocal {
        model: recognition::ModelId,
    },
    InstallLocal {
        job: String,
        source: Option<std::path::PathBuf>,
    },
    StartLocal {
        job: String,
        audio: Secret,
    },
    InstallDecision {
        job: String,
        source: Option<std::path::PathBuf>,
    },
    SelectDecision {
        model: crate::communication::decision::ModelId,
    },
    Decide {
        job: String,
        input: crate::communication::decision::Input,
    },
    ConfigureEndpoint {
        settings: endpoint::Settings,
        credential: Option<Secret>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub wake_profile: Option<wake::Profile>,
    #[serde(default)]
    pub wake_error: Option<String>,
    #[serde(default)]
    pub voice: voice::Status,
    pub mode: Mode,
    pub local_selection: recognition::ModelId,
    pub decision_selection: crate::communication::decision::ModelId,
    pub local: Option<LocalStatus>,
    pub decision_model: Option<LocalStatus>,
    pub decision_capabilities: Option<crate::communication::decision::Information>,
    pub listening: ListeningStatus,
    pub endpoint: Option<endpoint::Settings>,
    pub ready: bool,
    pub detail: String,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default)]
    pub purpose: Purpose,
    pub id: String,
    pub pending: bool,
    pub text: Option<String>,
    #[serde(default)]
    pub decision: Option<crate::communication::decision::Output>,
    pub error: Option<String>,
}

pub fn validate_audio(audio: &str) -> Result<(), String> {
    decode_audio(audio).map(|_| ())
}

pub fn decode_audio(audio: &str) -> Result<Vec<f32>, String> {
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
    let samples = reader
        .into_samples::<i16>()
        .map(|sample| sample.map(|sample| f32::from(sample) / 32768.0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "The microphone recording is incomplete.")?;
    if samples.len() as u32 != expected {
        return Err("The microphone recording is incomplete.".into());
    }
    Ok(samples)
}
