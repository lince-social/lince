use fiote::{communication::models, speech::local};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments
        .next()
        .ok_or("Use install <model-cache> [archive] or recognize <model-cache> <wav>...")?;
    let root = PathBuf::from(
        arguments
            .next()
            .ok_or("Choose an explicit model cache directory.")?,
    );
    let recognizer = local::Recognizer::new(root)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    if command == "install" {
        let source = arguments.next().map(PathBuf::from);
        if arguments.next().is_some() {
            return Err("Too many installation arguments.".into());
        }
        recognizer
            .cache
            .acquire(source, cancelled, Arc::new(AtomicU64::new(0)))
            .await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&local::MANIFEST).map_err(|error| error.to_string())?
        );
    } else if command == "inspect" {
        use std::io::Read;
        let reader = ruzstd::decoding::StreamingDecoder::new(recognizer.cache.verified_file()?)
            .map_err(|error| error.to_string())?;
        let mut archive = tar::Archive::new(reader.take(268_435_456));
        for entry in archive.entries().map_err(|error| error.to_string())? {
            let mut entry = entry.map_err(|error| error.to_string())?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            if entry.size() > 200_000_000 {
                return Err("Probe archive file exceeds inspection bounds.".into());
            }
            let path = entry
                .path()
                .map_err(|error| error.to_string())?
                .to_path_buf();
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::json!({"path":path,"bytes":bytes.len(),"sha256":models::digest(&bytes)})
            );
        }
    } else if command == "recognize" {
        let mut count = 0;
        for path in arguments {
            let path = PathBuf::from(path);
            let reader = hound::WavReader::open(&path).map_err(|error| error.to_string())?;
            let specification = reader.spec();
            if specification.sample_rate != 16_000
                || specification.channels != 1
                || specification.bits_per_sample != 16
                || specification.sample_format != hound::SampleFormat::Int
                || reader.duration() == 0
                || reader.duration() > 1_920_000
            {
                return Err(
                    "Probe fixtures must be mono 16 kHz 16-bit PCM WAV, up to two minutes.".into(),
                );
            }
            let samples = reader
                .into_samples::<i16>()
                .map(|sample| sample.map(|sample| f32::from(sample) / 32768.0))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            let result = recognizer.transcribe(&samples, &cancelled)?;
            let memory = std::fs::read_to_string("/proc/self/status")
                .ok()
                .map(|status| {
                    status
                        .lines()
                        .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                });
            println!(
                "{}",
                serde_json::to_string(
                    &serde_json::json!({"fixture":path,"result":result,"memory":memory})
                )
                .map_err(|error| error.to_string())?
            );
            count += 1;
        }
        if count == 0 {
            return Err("Choose at least one recorded English fixture.".into());
        }
        models::check_cancel(&cancelled)?;
    } else {
        return Err("Use install or recognize.".into());
    }
    Ok(())
}
