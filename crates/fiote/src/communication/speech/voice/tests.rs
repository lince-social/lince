use super::*;

async fn finished(service: &Service, job: &str) -> Job {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let result = service
                .handle(Request::Poll { job: job.into() })
                .await
                .unwrap()
                .job
                .unwrap();
            if !result.pending {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn voice_setup_is_explicit_bounded_and_cancelled_id_cannot_start_later() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::new(root.path().join("models")).unwrap();
    let status = service.handle(Request::Inspect).await.unwrap();
    assert!(!status.present && !status.busy);
    assert_eq!(status.required_bytes, 115_597_736);
    assert!(!root.path().join("models").exists());
    assert!(
        service
            .handle(Request::Speak {
                job: nucleus::new_uid("speech"),
                text: "Hello".into()
            })
            .await
            .unwrap_err()
            .contains("voice first")
    );
    assert!(
        service
            .handle(Request::Install {
                job: "../bad".into(),
                source: None
            })
            .await
            .unwrap_err()
            .contains("identity")
    );
    let job = nucleus::new_uid("speech");
    service
        .handle(Request::Cancel { job: job.clone() })
        .await
        .unwrap();
    assert!(
        service
            .handle(Request::Install { job, source: None })
            .await
            .unwrap_err()
            .contains("cancelled")
    );
    service.shutdown().await;
    assert!(
        service
            .handle(Request::Install {
                job: nucleus::new_uid("speech"),
                source: None
            })
            .await
            .unwrap_err()
            .contains("shutting down")
    );
}

#[tokio::test]
async fn incompatible_voice_directory_reports_failure_without_claiming_installation() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("kokoro.btm"), b"not a supported model").unwrap();
    let service = Service::new(root.path().join("models")).unwrap();
    let job = nucleus::new_uid("speech");
    service
        .handle(Request::Install {
            job: job.clone(),
            source: Some(source),
        })
        .await
        .unwrap();
    let result = finished(&service, &job).await;
    assert!(result.error.unwrap().contains("wrong type or size"));
    let status = service.handle(Request::Inspect).await.unwrap();
    assert!(!status.present && !status.busy);
    assert!(result.audio.is_none());
    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires LINCE_VOICE_TEST_ASSETS with the pinned Kokoro assets"]
async fn actual_english_voice_import_synthesis_and_cancel_use_the_registered_rust_engine() {
    let source = PathBuf::from(
        std::env::var("LINCE_VOICE_TEST_ASSETS").expect("Set LINCE_VOICE_TEST_ASSETS"),
    );
    let root = tempfile::tempdir().unwrap();
    let service = Service::new(root.path().join("models")).unwrap();
    let job = nucleus::new_uid("speech");
    service
        .handle(Request::Install {
            job: job.clone(),
            source: Some(source),
        })
        .await
        .unwrap();
    assert!(finished(&service, &job).await.error.is_none());
    assert!(service.handle(Request::Inspect).await.unwrap().present);
    for text in [
        "Hello. I am Fiote. Your local voice is working.",
        "Hey Fiote.",
        "Hey Fiote, go forward ten meters.",
    ] {
        let job = nucleus::new_uid("speech");
        let started = std::time::Instant::now();
        service
            .handle(Request::Speak {
                job: job.clone(),
                text: text.into(),
            })
            .await
            .unwrap();
        let result = finished(&service, &job).await;
        assert!(result.error.is_none(), "{:?}", result.error);
        let audio = result.audio.unwrap();
        let bytes = BASE64.decode(&audio.0).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(reader.spec().sample_rate, 24_000);
        assert_eq!(reader.spec().channels, 1);
        assert!(reader.duration() > 2400 && reader.duration() <= 24_000 * 45);
        let peak = reader
            .into_samples::<i16>()
            .map(|sample| sample.unwrap().unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 100);
        println!(
            "voice={text:?} bytes={} peak={peak} synthesis_ms={}",
            bytes.len(),
            started.elapsed().as_millis()
        );
        if let Ok(directory) = std::env::var("LINCE_VOICE_TEST_OUTPUT") {
            std::fs::create_dir_all(&directory).unwrap();
            let name = if text == "Hey Fiote." {
                "wake.wav"
            } else if text.starts_with("Hey") {
                "command.wav"
            } else {
                "reply.wav"
            };
            std::fs::write(PathBuf::from(directory).join(name), bytes).unwrap();
        }
    }
    let job = nucleus::new_uid("speech");
    service
        .handle(Request::Speak {
            job: job.clone(),
            text: "This cancelled synthesis must never be played.".into(),
        })
        .await
        .unwrap();
    service
        .handle(Request::Cancel { job: job.clone() })
        .await
        .unwrap();
    let result = finished(&service, &job).await;
    assert!(result.audio.is_none());
    assert!(result.error.unwrap().contains("cancelled"));
    service.shutdown().await;
}

#[test]
#[ignore = "Requires LINCE_VOICE_TEST_OUTPUT wake.wav; optional LINCE_SPEECH_TEST_CACHE reports ASR separately"]
fn actual_acoustic_reference_detects_synthetic_wake_without_requiring_its_asr_spelling() {
    let recordings = PathBuf::from(
        std::env::var("LINCE_VOICE_TEST_OUTPUT")
            .expect("Choose generated public synthesis recordings"),
    );
    let reader = hound::WavReader::open(recordings.join("wake.wav")).unwrap();
    assert_eq!(reader.spec().sample_rate, 24_000);
    assert_eq!(reader.spec().channels, 1);
    let input = reader
        .into_samples::<i16>()
        .map(|sample| f32::from(sample.unwrap()) / 32768.0)
        .collect::<Vec<_>>();
    let samples = (0..input.len() * 2 / 3)
        .map(|index| {
            let position = index as f64 * 1.5;
            let left = position as usize;
            let right = (left + 1).min(input.len() - 1);
            input[left] + (input[right] - input[left]) * position.fract() as f32
        })
        .collect::<Vec<_>>();
    let profile =
        crate::speech::wake::Profile::enroll(&[samples.clone(), samples.clone(), samples.clone()])
            .unwrap();
    let mut detector = crate::speech::wake::Detector::new(profile).unwrap();
    let mut detected = false;
    for part in samples.chunks(160) {
        detected |= detector.push(part).unwrap().is_some();
    }
    assert!(
        detected,
        "Synthetic self-reference smoke check; independent voice acceptance remains separate"
    );
    if let Ok(root) = std::env::var("LINCE_SPEECH_TEST_CACHE") {
        let recognizer = crate::speech::local::Recognizer::new(PathBuf::from(root)).unwrap();
        match recognizer.transcribe(&samples, &AtomicBool::new(false)) {
            Ok(transcript) => println!(
                "ASR diagnostic only; acoustic wake does not depend on its spelling: {:?}",
                transcript.text
            ),
            Err(error) => println!("ASR diagnostic only; acoustic wake was detected: {error}"),
        }
    }
}
