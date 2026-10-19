use std::{io::Cursor, path::Path, time::Instant};

fn wav(samples: &[f32], rate: u32) -> Result<Vec<u8>, String> {
    let mut bytes = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut bytes,
        hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| error.to_string())?;
    for sample in samples {
        if !sample.is_finite() {
            return Err("Invalid synthesized sample".into());
        }
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}

fn main() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "Use voice_request_fixture <verified-voice-assets> <new-output-directory>".into(),
        );
    }
    let assets = Path::new(&args[0]);
    let output = Path::new(&args[1]);
    std::fs::create_dir(output).map_err(|error| error.to_string())?;
    let text = include_str!("../tests/fixtures/long-request.txt").trim();
    std::fs::write(output.join("text.txt"), text).map_err(|error| error.to_string())?;
    for artifact in fiote::speech::voice::ARTIFACTS {
        let bytes = std::fs::read(assets.join(artifact.name)).map_err(|error| error.to_string())?;
        if bytes.len() as u64 != artifact.bytes
            || fiote::communication::models::digest(&bytes) != artifact.sha256
        {
            return Err(format!(
                "Voice asset failed verification: {}",
                artifact.name
            ));
        }
    }
    let engine = best_tts::Engine::from_dir(assets)
        .map_err(|error| error.to_string())?
        .with_threads(4);
    let mut source = Vec::new();
    let mut sentences = Vec::new();
    for (index, sentence) in text
        .split_inclusive('.')
        .filter(|part| !part.trim().is_empty())
        .enumerate()
    {
        let started = Instant::now();
        let audio = engine
            .say(sentence.trim(), "af_heart")
            .map_err(|error| error.to_string())?;
        if audio.sample_rate != 24_000 {
            return Err("Unexpected synthesis sample rate".into());
        }
        std::fs::write(output.join(format!("sentence-{index}.wav")), audio.to_wav())
            .map_err(|error| error.to_string())?;
        let first = audio
            .samples
            .iter()
            .position(|sample| sample.abs() > 0.001)
            .ok_or("No synthesized speech")?;
        let last = audio
            .samples
            .iter()
            .rposition(|sample| sample.abs() > 0.001)
            .unwrap();
        source.extend_from_slice(
            &audio.samples[first.saturating_sub(4800)..(last + 4801).min(audio.samples.len())],
        );
        let result = serde_json::json!({"sentence":index,"text":sentence.trim(),"samples":audio.samples.len(),"synthesis_ms":started.elapsed().as_millis()});
        println!("{result}");
        sentences.push(result);
        std::fs::write(
            output.join("sentences.json"),
            serde_json::to_vec_pretty(&sentences).unwrap(),
        )
        .map_err(|error| error.to_string())?;
    }
    if source.is_empty() {
        return Err("No generated request".into());
    }
    std::fs::write(output.join("source.wav"), wav(&source, 24_000)?)
        .map_err(|error| error.to_string())?;
    let samples = (0..291_200)
        .map(|index| {
            let position = index as f64 * (source.len() - 1) as f64 / 291_199.0;
            let left = position as usize;
            let right = (left + 1).min(source.len() - 1);
            source[left] + (source[right] - source[left]) * position.fract() as f32
        })
        .collect::<Vec<_>>();
    let bytes = wav(&samples, 16_000)?;
    std::fs::write(output.join("long-request.wav"), &bytes).map_err(|error| error.to_string())?;
    let reference = serde_json::json!({
        "text":text,"sha256":fiote::communication::models::digest(&bytes),"input_ms":18_200,
        "source_ms":source.len()*1000/24_000,"model_revision":fiote::speech::voice::REVISION,
        "model_weights_sha256":fiote::speech::voice::ARTIFACTS[0].sha256,
        "voice":"af_heart","model_license":"Apache-2.0; attribution under crates/fiote/assets/kokoro",
        "method":"Rust Kokoro sentence synthesis with 200ms quiet context at boundaries; linear resampling to mono 16kHz 16-bit PCM, 18.2 seconds",
        "scope":"Synthetic long-request framing and performance regression; no human/acoustic accuracy claim"
    });
    std::fs::write(
        output.join("long-request-reference.json"),
        serde_json::to_vec_pretty(&reference).unwrap(),
    )
    .map_err(|error| error.to_string())?;
    println!("{reference}");
    Ok(())
}
