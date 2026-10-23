use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn recording(samples: &[i16]) -> ::fiote::config::Secret {
    let length = u32::try_from(samples.len() * 2).unwrap();
    let mut bytes = Vec::with_capacity(44 + length as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + length).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&length.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    ::fiote::config::Secret(STANDARD.encode(bytes))
}

async fn configured(path: PathBuf) -> (Host, tokio::net::TcpListener) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = Host::open(path).unwrap();
    host.handle(Request::ConfigureEndpoint {
        settings: endpoint::Settings {
            url: format!("http://{}/transcribe", listener.local_addr().unwrap()),
            model: "must-not-receive-local-audio".into(),
            requires_key: false,
            allow_cloud: false,
        },
        credential: None,
        password: None,
    })
    .await
    .unwrap();
    host.handle(Request::SelectMode { mode: Mode::Local })
        .await
        .unwrap();
    (host, listener)
}

async fn finished(host: &Host, job: &str) -> Job {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let status = host
                .handle(Request::Poll { job: job.into() })
                .await
                .unwrap();
            let result = status.job.unwrap();
            if !result.pending {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

mod speech_reference {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/support/speech_reference.rs"
    ));
}

use speech_reference::words as reference_words;

#[tokio::test]
async fn local_model_selection_persists_without_enabling_listening_or_downloading() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    let host = Host::open(path.clone()).unwrap();
    for model in ModelId::ALL {
        let selected = host.handle(Request::SelectLocal { model }).await.unwrap();
        assert_eq!(selected.local_selection, model);
        assert!(!selected.ready);
        assert!(!selected.listening.enabled);
        assert!(!selected.local.unwrap().present);
        let reopened = Host::open(path.clone()).unwrap();
        assert_eq!(
            reopened
                .handle(Request::Inspect)
                .await
                .unwrap()
                .local_selection,
            model
        );
    }
}

#[tokio::test]
#[ignore = "Requires LINCE_MOONSHINE_TEST_IMPORT containing the pinned Tiny or Small model files"]
async fn actual_moonshine_import_dictation_and_silence_use_normal_speech_service() {
    let root = tempfile::tempdir().unwrap();
    let (host, endpoint) = configured(root.path().join("speech.json")).await;
    let model = std::env::var("LINCE_SPEECH_TEST_SELECTION")
        .map(|value| serde_json::from_str::<ModelId>(&value).unwrap())
        .unwrap_or(ModelId::MoonshineTiny);
    assert_ne!(model, ModelId::Phonon);
    host.handle(Request::SelectLocal { model }).await.unwrap();
    let installation = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: installation.clone(),
        source: Some(
            std::env::var_os("LINCE_MOONSHINE_TEST_IMPORT")
                .unwrap()
                .into(),
        ),
    })
    .await
    .unwrap();
    let setup = finished(&host, &installation).await;
    assert!(setup.error.is_none(), "{setup:?}");
    let installed = host.handle(Request::Inspect).await.unwrap().local.unwrap();
    assert_eq!(installed.acquired_bytes, installed.required_bytes);
    assert!(installed.present);
    let id = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: id.clone(),
        audio: ::fiote::config::Secret(STANDARD.encode(include_bytes!(
            "../../../fiote/tests/fixtures/speech-goforward.wav"
        ))),
    })
    .await
    .unwrap();
    let result = finished(&host, &id).await;
    assert!(result.error.is_none(), "{result:?}");
    let text = result.text.unwrap();
    println!(
        "{}",
        serde_json::json!({"model":model,"transcript":text,"strict_reference_match":reference_words(&text) == reference_words("go forward ten meters")})
    );
    assert!(
        reference_words(&text).starts_with(&reference_words("go forward ten")),
        "{text}"
    );
    let mut long = speech::decode_audio(&STANDARD.encode(include_bytes!(
        "../../../fiote/tests/fixtures/speech-goforward.wav"
    )))
    .unwrap()
    .into_iter()
    .map(|sample| (sample * 32768.0).round() as i16)
    .collect::<Vec<_>>();
    long.extend(std::iter::repeat_n(0, 640_000));
    let id = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: id.clone(),
        audio: recording(&long),
    })
    .await
    .unwrap();
    let result = finished(&host, &id).await;
    assert!(result.error.is_none(), "{result:?}");
    let text = result.text.unwrap();
    println!(
        "{}",
        serde_json::json!({"model":model,"long_recording_transcript":text,"silent_tail_seconds":40})
    );
    let words = reference_words(&text);
    assert!(
        words.starts_with(&reference_words("go forward ten")),
        "{text}"
    );
    assert!(
        !words.iter().any(|word| word == "you"),
        "Silence added a hallucination: {text}"
    );
    let id = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: id.clone(),
        audio: recording(&[0; 16_000]),
    })
    .await
    .unwrap();
    assert!(
        finished(&host, &id)
            .await
            .error
            .unwrap()
            .contains("No speech")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), endpoint.accept())
            .await
            .is_err()
    );
    host.shutdown().await;
}

#[test]
fn reference_comparison_ignores_formatting_without_hiding_negation_or_reordered_quantities() {
    assert_eq!(
        reference_words("Mister John, ten."),
        reference_words("Mr. John: 10")
    );
    assert_eq!(
        reference_words("ill-disposed"),
        reference_words("ill disposed")
    );
    assert_ne!(
        reference_words("do not remove 7"),
        reference_words("do remove seven")
    );
    assert_ne!(
        reference_words("eight four seven"),
        reference_words("8 7 4")
    );
    assert_ne!(
        reference_words("John Dashwood"),
        reference_words("John Dashward")
    );
}

#[tokio::test]
#[ignore = "Requires installed Phonon and independently referenced English recordings prepared with speech_fixtures --references"]
async fn actual_local_numbers_proper_name_and_negation_match_independent_recordings_without_cloud_fallback()
 {
    verify_recorded_references(include_str!(
        "../../../fiote/tests/fixtures/english-asr-references.json"
    ))
    .await;
}

#[tokio::test]
#[ignore = "Requires installed Phonon and recorded-noise fixtures prepared with speech_fixtures --noise"]
async fn actual_local_recorded_noise_and_speech_mixes_preserve_reference_without_cloud_fallback() {
    verify_recorded_references(include_str!(
        "../../../fiote/tests/fixtures/english-asr-noise-references.json"
    ))
    .await;
}

async fn verify_recorded_references(manifest: &str) {
    let root = tempfile::tempdir().unwrap();
    let (host, listener) = configured(root.path().join("speech.json")).await;
    let source = speech::local::Recognizer::new(
        std::env::var_os("LINCE_SPEECH_TEST_CACHE")
            .expect("Choose installed model cache")
            .into(),
    )
    .unwrap()
    .cache
    .path();
    let setup = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: setup.clone(),
        source: Some(source),
    })
    .await
    .unwrap();
    assert!(finished(&host, &setup).await.error.is_none());
    let directory = PathBuf::from(
        std::env::var_os("LINCE_SPEECH_TEST_REFERENCES_DIR")
            .expect("Choose prepared independent reference recordings"),
    );
    let manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
    let mut mismatches = Vec::new();
    for case in manifest["cases"].as_array().unwrap() {
        let name = case["file"].as_str().unwrap();
        let wav = std::fs::read(directory.join(name)).unwrap();
        assert_eq!(
            fiote::communication::models::digest(&wav),
            case["sha256"].as_str().unwrap()
        );
        let expected = case["expected"].as_str();
        let job = nucleus::new_uid("speech");
        let started = Instant::now();
        host.handle(Request::StartLocal {
            job: job.clone(),
            audio: fiote::config::Secret(STANDARD.encode(wav)),
        })
        .await
        .unwrap();
        let result = finished(&host, &job).await;
        let actual = result.text.as_deref().unwrap_or_default();
        let matched = match expected {
            Some(expected) => {
                result.error.is_none() && reference_words(expected) == reference_words(actual)
            }
            None => {
                result.text.is_none()
                    && result
                        .error
                        .as_deref()
                        .is_some_and(|error| error.contains("No speech"))
            }
        };
        println!(
            "{}",
            serde_json::json!({"fixture": name, "expected": expected, "actual": actual, "error": result.error, "elapsed_ms": started.elapsed().as_millis(), "matches": matched})
        );
        if !matched {
            mismatches.push(name.to_owned());
        }
        assert!(!host.local.busy());
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
    host.shutdown().await;
    assert!(
        mismatches.is_empty(),
        "Independent reference mismatches: {mismatches:?}"
    );
}

#[tokio::test]
#[ignore = "Requires installed Phonon and the independently referenced goforward recording"]
async fn actual_local_shutdown_drains_cpu_unloads_weights_and_keeps_verified_assets_for_restart() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    let host = Host::open(path.clone()).unwrap();
    let source = speech::local::Recognizer::new(
        std::env::var_os("LINCE_SPEECH_TEST_CACHE")
            .expect("Choose installed model cache")
            .into(),
    )
    .unwrap()
    .cache
    .path();
    let setup = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: setup.clone(),
        source: Some(source),
    })
    .await
    .unwrap();
    assert!(finished(&host, &setup).await.error.is_none());
    let audio = ::fiote::config::Secret(
        STANDARD.encode(
            std::fs::read(
                std::env::var_os("LINCE_SPEECH_TEST_WAV")
                    .expect("Choose referenced goforward recording"),
            )
            .unwrap(),
        ),
    );
    let job = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: job.clone(),
        audio: audio.clone(),
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(180), async {
        while !host.local.loaded() || !host.local.busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(60), host.shutdown())
        .await
        .unwrap();
    let status = host.handle(Request::Inspect).await.unwrap();
    assert!(!status.ready);
    assert!(!status.local.as_ref().unwrap().busy);
    assert!(!status.local.unwrap().loaded);
    assert!(host.local.primary_cache().verified_file().is_ok());
    assert!(host.handle(Request::Poll { job }).await.is_err());
    assert!(
        host.handle(Request::StartLocal {
            job: nucleus::new_uid("speech"),
            audio: audio.clone()
        })
        .await
        .unwrap_err()
        .contains("shutting down")
    );
    let restarted = Host::open(path).unwrap();
    let job = nucleus::new_uid("speech");
    restarted
        .handle(Request::StartLocal {
            job: job.clone(),
            audio,
        })
        .await
        .unwrap();
    let result = finished(&restarted, &job).await;
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(
        reference_words(result.text.as_deref().unwrap()),
        reference_words("go forward ten meters")
    );
    restarted.shutdown().await;
}

#[tokio::test]
async fn malformed_and_unavailable_local_audio_preserve_configuration_without_endpoint_fallback() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    let (host, listener) = configured(path.clone()).await;
    let valid = recording(&[500; 160]);
    let mut truncated = STANDARD.decode(&valid.0).unwrap();
    truncated.truncate(truncated.len() - 2);
    let job = nucleus::new_uid("speech");
    for audio in [
        ::fiote::config::Secret("invalid base64!".into()),
        ::fiote::config::Secret(STANDARD.encode(b"not a WAV")),
        ::fiote::config::Secret(STANDARD.encode(truncated)),
    ] {
        assert!(
            host.handle(Request::StartLocal {
                job: job.clone(),
                audio,
            })
            .await
            .is_err()
        );
        let status = host.handle(Request::Inspect).await.unwrap();
        assert_eq!(status.mode, Mode::Local);
        assert!(status.endpoint.is_some());
        assert!(status.job.is_none());
        assert!(!status.local.unwrap().loaded);
        assert!(!host.state.lock().await.used.contains(&job));
    }
    assert!(
        host.handle(Request::StartLocal { job, audio: valid })
            .await
            .unwrap_err()
            .contains("Download or import")
    );
    assert!(!root.path().join("models").exists());
    assert_eq!(
        Host::open(path)
            .unwrap()
            .handle(Request::Inspect)
            .await
            .unwrap()
            .mode,
        Mode::Local
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "Requires LINCE_SPEECH_TEST_CACHE and LINCE_SPEECH_TEST_WAV with installed Phonon weights and an independently referenced English recording"]
async fn real_local_failures_recover_without_endpoint_fallback_or_duplicate_transcription() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.json");
    let (host, listener) = configured(path).await;
    let source = speech::local::Recognizer::new(
        std::env::var_os("LINCE_SPEECH_TEST_CACHE")
            .expect("Choose an installed model cache")
            .into(),
    )
    .unwrap()
    .cache
    .path();
    let wav = std::fs::read(
        std::env::var_os("LINCE_SPEECH_TEST_WAV")
            .expect("Choose an independently referenced English recording"),
    )
    .unwrap();
    let expected = std::env::var("LINCE_SPEECH_TEST_EXPECTED")
        .expect("Choose the independent reference phrase");
    let audio = ::fiote::config::Secret(STANDARD.encode(wav));
    std::fs::create_dir_all(host.local.primary_cache().directory()).unwrap();
    std::fs::File::create(host.local.primary_cache().path())
        .unwrap()
        .set_len(speech::local::MANIFEST.artifact.bytes)
        .unwrap();
    for samples in [vec![0; 16_000], vec![10; 16_000]] {
        let job = nucleus::new_uid("speech");
        host.handle(Request::StartLocal {
            job: job.clone(),
            audio: recording(&samples),
        })
        .await
        .unwrap();
        let result = finished(&host, &job).await;
        assert!(result.error.unwrap().contains("No speech"));
        assert!(result.text.is_none());
        assert!(!host.local.loaded());
        assert!(!host.local.busy());
    }
    let corrupt = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: corrupt.clone(),
        audio: audio.clone(),
    })
    .await
    .unwrap();
    let result = finished(&host, &corrupt).await;
    assert!(result.error.unwrap().contains("checksum"));
    assert!(result.text.is_none());
    assert!(!host.local.loaded());
    assert!(!host.local.busy());
    let installation = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: installation.clone(),
        source: Some(source.clone()),
    })
    .await
    .unwrap();
    assert!(finished(&host, &installation).await.error.is_none());
    let invalid = root.path().join("invalid-model");
    std::fs::File::create(&invalid)
        .unwrap()
        .set_len(speech::local::MANIFEST.artifact.bytes)
        .unwrap();
    let refused = nucleus::new_uid("speech");
    host.handle(Request::InstallLocal {
        job: refused.clone(),
        source: Some(invalid),
    })
    .await
    .unwrap();
    assert!(finished(&host, &refused).await.error.is_some());
    assert_eq!(
        host.local
            .primary_cache()
            .verified_file()
            .unwrap()
            .metadata()
            .unwrap()
            .len(),
        std::fs::metadata(source).unwrap().len()
    );
    let recovered = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: recovered.clone(),
        audio: audio.clone(),
    })
    .await
    .unwrap();
    let result = finished(&host, &recovered).await;
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .text
            .unwrap()
            .to_lowercase()
            .contains(&expected.to_lowercase())
    );
    let cancelled = nucleus::new_uid("speech");
    let request = Request::StartLocal {
        job: cancelled.clone(),
        audio: audio.clone(),
    };
    host.handle(request.clone()).await.unwrap();
    host.handle(Request::Cancel {
        job: cancelled.clone(),
    })
    .await
    .unwrap();
    assert!(host.handle(Request::Poll { job: cancelled }).await.is_err());
    assert!(host.handle(request).await.is_err());
    tokio::time::timeout(Duration::from_secs(60), async {
        while host.local.busy() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let retry = nucleus::new_uid("speech");
    host.handle(Request::StartLocal {
        job: retry.clone(),
        audio,
    })
    .await
    .unwrap();
    let result = finished(&host, &retry).await;
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .text
            .unwrap()
            .to_lowercase()
            .contains(&expected.to_lowercase())
    );
    assert!(!host.local.busy());
    let status = host.handle(Request::Inspect).await.unwrap();
    assert_eq!(status.mode, Mode::Local);
    assert!(status.endpoint.is_some());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
