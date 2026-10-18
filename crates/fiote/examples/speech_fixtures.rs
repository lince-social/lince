use fiote::communication::models;
use serde_json::json;
use std::{io::Write, path::Path, time::Duration};

const REVISION: &str = "f20ff1b7a5db64c5e892a798479e20df86e79a35";

async fn download(client: &reqwest::Client, path: &str) -> Result<Vec<u8>, String> {
    download_url(
        client,
        &format!("https://raw.githubusercontent.com/cmusphinx/pocketsphinx/{REVISION}/{path}"),
    )
    .await
}

async fn download_url(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > 4_000_000)
    {
        return Err("The recorded fixture exceeds four megabytes.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len() + chunk.len() > 4_000_000 {
            return Err("The recorded fixture exceeds four megabytes.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn save(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(directory.join(name))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn samples(bytes: &[u8]) -> Result<Vec<f64>, String> {
    let reader =
        hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > 1_920_000
    {
        return Err("Recorded fixtures require bounded mono 16 kHz PCM.".into());
    }
    reader
        .into_samples::<i16>()
        .map(|sample| {
            sample
                .map(|sample| f64::from(sample) / 32768.0)
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn wav(samples: &[f64]) -> Result<Vec<u8>, String> {
    let mut output = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut output,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| error.to_string())?;
    for sample in samples {
        if !sample.is_finite() || sample.abs() > 1.0 {
            return Err("Noise mixing produced invalid or clipping samples.".into());
        }
        writer
            .write_sample((sample * 32767.0).round() as i16)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(output.into_inner())
}

async fn noise_fixtures(client: &reqwest::Client, directory: &Path) -> Result<(), String> {
    const NOISE_REVISION: &str = "ff7d61f219e4eb41489963c4bb5f57bea5bc2c69";
    let base = format!("https://raw.githubusercontent.com/LCAV/pyroomacoustics/{NOISE_REVISION}");
    let url = format!("{base}/examples/input_samples/doing_the_dishes.wav");
    let noise_bytes = download_url(client, &url).await?;
    if noise_bytes.len() != 3_045_984
        || models::digest(&noise_bytes)
            != "099eafcd7c4c266612012b5622b97157042e5288acaf4cbf3dc475814bfcdaa8"
    {
        return Err("The recorded noise source failed verification.".into());
    }
    let source_readme =
        download_url(client, &format!("{base}/examples/input_samples/README.md")).await?;
    save(directory, "NOISE-PROVENANCE.txt", &source_readme)?;
    let noise = samples(&noise_bytes)?;
    let speech_bytes =
        std::fs::read(directory.join("goforward.wav")).map_err(|error| error.to_string())?;
    let speech = samples(&speech_bytes)?;
    let raw = speech
        .iter()
        .flat_map(|sample| ((*sample * 32768.0).round() as i16).to_le_bytes())
        .collect::<Vec<_>>();
    if models::digest(&raw) != "f15c60ec54059d8b66e410d0064945a0b0a04ea56e1ddca1958e493c0cf70e71" {
        return Err("Use the verified goforward reference prepared in this directory.".into());
    }
    let mut cases = Vec::new();
    let noise_only = wav(noise
        .get(..160_000)
        .ok_or("Need ten seconds of recorded noise")?)?;
    save(directory, "recorded-noise-only.wav", &noise_only)?;
    cases.push(json!({"file":"recorded-noise-only.wav", "sha256":models::digest(&noise_only), "expected":null, "samples":160000, "noise_gain":1.0}));
    for snr in [10.0_f64, 0.0] {
        let background = noise
            .get(..speech.len())
            .ok_or("Noise recording is shorter than the speech reference")?;
        let speech_power =
            speech.iter().map(|sample| sample * sample).sum::<f64>() / speech.len() as f64;
        let noise_power =
            background.iter().map(|sample| sample * sample).sum::<f64>() / background.len() as f64;
        if speech_power <= 0.0 || noise_power <= 0.0 {
            return Err("Mixing requires non-silent recorded sources.".into());
        }
        let gain = (speech_power / noise_power / 10.0_f64.powf(snr / 10.0)).sqrt();
        let mixed = speech
            .iter()
            .zip(background)
            .map(|(speech, noise)| speech + noise * gain)
            .collect::<Vec<_>>();
        let peak = mixed
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f64, f64::max);
        let scale = if peak > 0.98 { 0.98 / peak } else { 1.0 };
        let mixed = mixed
            .iter()
            .map(|sample| sample * scale)
            .collect::<Vec<_>>();
        let bytes = wav(&mixed)?;
        let file = format!("goforward-recorded-noise-{}db.wav", snr as i32);
        save(directory, &file, &bytes)?;
        cases.push(json!({"file":file,"sha256":models::digest(&bytes),"expected":"go forward ten meters","samples":speech.len(),"snr_db":snr,"noise_gain":gain,"common_scale":scale}));
    }
    let metadata = json!({
        "noise_source":url,"noise_sha256":models::digest(&noise_bytes),"noise_repository_revision":NOISE_REVISION,
        "noise_origin":"Google Speech Commands Dataset; source attribution in NOISE-PROVENANCE.txt",
        "noise_license":"CC-BY-4.0","license_url":"https://creativecommons.org/licenses/by/4.0/",
        "dataset_origin":"https://research.google/blog/launching-the-speech-commands-dataset/",
        "speech_source":format!("https://github.com/cmusphinx/pocketsphinx/blob/{REVISION}/test/data/goforward.raw"),
        "speech_reference":format!("https://github.com/cmusphinx/pocketsphinx/blob/{REVISION}/test/unit/test_fsg/test_fsg.c"),
        "speech_license":"See LICENSE-PocketSphinx.txt",
        "conversion":"Rust hound; crop recorded noise, mix independently recorded speech/noise at stated power ratio, common scaling only to avoid clipping; no synthetic noise, denoising or transcript changes",
        "sample_rate":16000,"cases":cases,
    });
    save(
        directory,
        "noise-references.json",
        &serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn trimmed(samples: &[f64]) -> Result<&[f64], String> {
    let voiced = samples
        .chunks(320)
        .enumerate()
        .filter(|(_, frame)| {
            (frame.iter().map(|sample| sample * sample).sum::<f64>() / frame.len() as f64).sqrt()
                > 0.003
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let first = voiced
        .first()
        .ok_or("The command source has no voiced frames")?;
    let last = voiced.last().unwrap();
    Ok(&samples[(first * 320).saturating_sub(1600)..((last + 1) * 320 + 1600).min(samples.len())])
}

async fn command_fixture(client: &reqwest::Client, directory: &Path) -> Result<(), String> {
    let mut manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/english-addressed-command-reference.json"
    ))
    .map_err(|error| error.to_string())?;
    let prefix = &manifest["prefix"];
    let field = |name: &str| {
        prefix[name]
            .as_str()
            .ok_or_else(|| format!("Missing command prefix {name}"))
    };
    let base = format!(
        "https://raw.githubusercontent.com/{}/{}",
        field("repository")?,
        field("revision")?
    );
    let source = field("source")?;
    let bytes = download_url(client, &format!("{base}/{source}")).await?;
    let license = download_url(client, &format!("{base}/{}", field("license_source")?)).await?;
    if models::digest(&bytes) != field("sha256")?
        || models::digest(&license) != manifest["license_sha256"].as_str().unwrap_or_default()
        || !std::str::from_utf8(&license)
            .map_err(|error| error.to_string())?
            .lines()
            .any(|line| line == source)
    {
        return Err("The command prefix or its contributor license failed verification.".into());
    }
    let command = std::fs::read(
        directory.join(
            manifest["command"]["file"]
                .as_str()
                .ok_or("Missing prepared command recording")?,
        ),
    )
    .map_err(|_| "Prepare the base goforward fixture in this directory first.")?;
    if models::digest(&command)
        != manifest["command"]["wav_sha256"]
            .as_str()
            .unwrap_or_default()
    {
        return Err("The independently referenced goforward recording failed verification.".into());
    }
    let prefix_samples = samples(&bytes)?;
    let command_samples = samples(&command)?;
    let joined = trimmed(&prefix_samples)?
        .iter()
        .chain(trimmed(&command_samples)?)
        .copied()
        .collect::<Vec<_>>();
    let output = wav(&joined)?;
    let checksum = models::digest(&output);
    if manifest["sha256"]
        .as_str()
        .is_some_and(|expected| expected != checksum)
    {
        return Err("The reconstructed command differs from its frozen checksum.".into());
    }
    let reference = download_url(client, &format!("{base}/{}", field("reference")?)).await?;
    save(
        directory,
        manifest["file"]
            .as_str()
            .ok_or("Missing command output name")?,
        &output,
    )?;
    save(
        directory,
        manifest["license_file"].as_str().unwrap(),
        &license,
    )?;
    save(
        directory,
        "addressed-command.prefix.reference.txt",
        &reference,
    )?;
    manifest["sha256"] = checksum.into();
    manifest["audio_ms"] = (joined.len() / 16).into();
    save(
        directory,
        "addressed-command-reference.json",
        &serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )?;
    println!("{manifest}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let directory = std::path::PathBuf::from(
        arguments
            .next()
            .ok_or("Choose a fixture output directory.")?,
    );
    let mode = arguments
        .next()
        .map(|mode| mode.into_string().map_err(|_| "Use a valid fixture flag."))
        .transpose()?
        .unwrap_or_default();
    if arguments.next().is_some()
        || !["", "--references", "--noise", "--addressed", "--command"].contains(&mode.as_str())
    {
        return Err(
            "Use speech_fixtures <output-directory> [--references|--noise|--addressed|--command]."
                .into(),
        );
    }
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    if mode == "--command" {
        return command_fixture(&client, &directory).await;
    }
    if mode == "--addressed" {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/english-addressed-reference.json"
        ))
        .map_err(|error| error.to_string())?;
        let field = |name: &str| {
            manifest[name]
                .as_str()
                .ok_or_else(|| format!("Missing addressed fixture {name}"))
        };
        let base = format!(
            "https://raw.githubusercontent.com/{}/{}",
            field("repository")?,
            field("revision")?
        );
        let bytes = download_url(&client, &format!("{base}/{}", field("source")?)).await?;
        let license =
            download_url(&client, &format!("{base}/{}", field("license_source")?)).await?;
        if models::digest(&bytes) != field("sha256")?
            || models::digest(&license) != field("license_sha256")?
            || !std::str::from_utf8(&license)
                .map_err(|error| error.to_string())?
                .lines()
                .any(|line| line == field("source").unwrap_or_default())
        {
            return Err(
                "The addressed recording or its exact contributor license failed verification."
                    .into(),
            );
        }
        samples(&bytes)?;
        let reference = download_url(&client, &format!("{base}/{}", field("reference")?)).await?;
        save(&directory, field("file")?, &bytes)?;
        save(&directory, "LICENSE-Computer-Kris-Gesling.txt", &license)?;
        save(&directory, "addressed-computer.reference.txt", &reference)?;
        save(
            &directory,
            "addressed-reference.json",
            &serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
        )?;
        println!("{manifest}");
        return Ok(());
    }
    let license = download(&client, "LICENSE").await?;
    save(&directory, "LICENSE-PocketSphinx.txt", &license)?;
    if mode == "--noise" {
        return noise_fixtures(&client, &directory).await;
    }
    if mode == "--references" {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/english-asr-references.json"
        ))
        .map_err(|error| error.to_string())?;
        if manifest["revision"].as_str() != Some(REVISION) {
            return Err("Reference recordings must use the pinned source revision.".into());
        }
        for case in manifest["cases"]
            .as_array()
            .ok_or("Missing reference recordings")?
        {
            let path = case["source"].as_str().ok_or("Missing recording source")?;
            let bytes = download(&client, path).await?;
            if models::digest(&bytes)
                != case["sha256"]
                    .as_str()
                    .ok_or("Missing recording checksum")?
            {
                return Err(format!("The pinned {path} recording failed verification."));
            }
            let reader = hound::WavReader::new(std::io::Cursor::new(&bytes))
                .map_err(|error| error.to_string())?;
            let spec = reader.spec();
            if spec.sample_rate != 16000
                || spec.channels != 1
                || spec.bits_per_sample != 16
                || spec.sample_format != hound::SampleFormat::Int
                || reader.duration() == 0
                || reader.duration() > 1_920_000
            {
                return Err("Reference recordings must be bounded mono 16 kHz PCM WAV.".into());
            }
            save(
                &directory,
                case["file"].as_str().ok_or("Missing recording name")?,
                &bytes,
            )?;
            let reference = case["reference"]
                .as_str()
                .ok_or("Missing independent transcript source")?;
            let transcript = download(&client, reference).await?;
            save(
                &directory,
                &format!("{}.reference.txt", case["file"].as_str().unwrap()),
                &transcript,
            )?;
        }
        save(
            &directory,
            "references.json",
            &serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let mut fixtures = Vec::new();
    for (name, size, checksum) in [
        (
            "goforward",
            89_160,
            "f15c60ec54059d8b66e410d0064945a0b0a04ea56e1ddca1958e493c0cf70e71",
        ),
        (
            "numbers",
            128_742,
            "45b5f6d3f88e36dd55b025cc9e8f1cae567583ea93f554ecdff6d09e6dd4577d",
        ),
        (
            "something",
            95_958,
            "eb95b74ce3f3037487e49dcfd935bbcf8c158b5f4083dd19451729c3ee694f5f",
        ),
    ] {
        let path = format!("test/data/{name}.raw");
        let bytes = download(&client, &path).await?;
        if bytes.len() != size || models::digest(&bytes) != checksum {
            return Err(format!("The pinned {name} recording failed verification."));
        }
        if bytes.is_empty() || bytes.len() % 2 != 0 || bytes.len() / 2 > 1_920_000 {
            return Err("Expected bounded little-endian 16-bit mono PCM.".into());
        }
        let mut container = std::io::Cursor::new(Vec::new());
        let mut wav = hound::WavWriter::new(
            &mut container,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|error| error.to_string())?;
        for sample in bytes.chunks_exact(2) {
            wav.write_sample(i16::from_le_bytes([sample[0], sample[1]]))
                .map_err(|error| error.to_string())?;
        }
        wav.finalize().map_err(|error| error.to_string())?;
        let wav = container.into_inner();
        save(&directory, &format!("{name}.wav"), &wav)?;
        fixtures.push(json!({
            "name":name,"source":format!("https://github.com/cmusphinx/pocketsphinx/blob/{REVISION}/{path}"),
            "source_bytes":bytes.len(),"source_sha256":models::digest(&bytes),
            "wav_sha256":models::digest(&wav),"sample_rate":16000,"samples":bytes.len()/2,
            "expected":if name == "goforward" { Some("go forward ten meters") } else { None },
        }));
    }
    let provenance = json!({
        "repository":"cmusphinx/pocketsphinx","revision":REVISION,
        "license":"BSD-2-Clause; full upstream notices in LICENSE-PocketSphinx.txt",
        "conversion":"Rust hound WAV container around the original PCM; samples unchanged",
        "fixtures":fixtures,
    });
    let bytes = serde_json::to_vec_pretty(&provenance).map_err(|error| error.to_string())?;
    save(&directory, "provenance.json", &bytes)?;
    println!(
        "{}",
        String::from_utf8(bytes).map_err(|error| error.to_string())?
    );
    Ok(())
}
