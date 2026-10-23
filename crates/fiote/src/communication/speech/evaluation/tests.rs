use super::*;

#[test]
fn scoring_keeps_wrong_values_names_negation_order_and_hallucinations() {
    assert_eq!(
        Score::compare("Go forward ten meters.", "go forward10 meters").edits,
        0
    );
    for (reference, hypothesis) in [
        ("Do not delete the file", "delete the file"),
        ("Alice eight Bob four", "Alice four Bob eight"),
        ("four pears", "four pairs"),
        ("Maya Chen", "Mya Chen"),
        ("", "Thank you"),
    ] {
        assert!(Score::compare(reference, hypothesis).edits > 0);
    }
    let mut score = Score::compare("one two three four", "one two");
    score.merge(&Score::compare("five", "five"));
    assert_eq!(score.reference_words, 5);
    assert_eq!(score.edits, 2);
    assert_eq!(score.exact_utterances, 1);
    assert_eq!(score.wer(), 0.4);
}

fn fixture() -> Fixture {
    Fixture {
        id: "test".into(),
        audio: "fixture.wav".into(),
        sha256: "0".repeat(64),
        expected: "Do not delete the file".into(),
        provenance: "synthetic unit test".into(),
        require_exact: false,
    }
}

#[test]
fn changed_fixture_bytes_are_rejected_before_decoding() -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let mut fixture = fixture();
    fixture.audio = temporary.path().join("fixture.wav");
    std::fs::write(&fixture.audio, b"not audio").map_err(|error| error.to_string())?;
    assert!(audio(&fixture).unwrap_err().contains("SHA-256 mismatch"));
    Ok(())
}

struct FakeBackend(bool);

impl Backend for FakeBackend {
    fn recognize(&mut self, _: &[f32], _: &AtomicBool) -> Result<Recognition, String> {
        if self.0 {
            return Err("runtime failure".into());
        }
        Ok(Recognition {
            text: "Delete the file".into(),
            cold_load_ms: 15,
            transcription_ms: 500,
        })
    }
}

#[test]
fn common_backend_contract_preserves_errors_transcripts_and_speed() {
    let samples = vec![0.0; 16_000];
    let sample = evaluate(&mut FakeBackend(false), &fixture(), &samples, 1);
    assert_eq!(sample.recognition.unwrap().text, "Delete the file");
    assert_eq!(sample.score.unwrap().edits, 2);
    assert_eq!(sample.real_time_factor, Some(0.5));
    let failed = evaluate(&mut FakeBackend(true), &fixture(), &samples, 0);
    assert!(failed.recognition.is_none());
    assert!(failed.score.is_none());
    assert_eq!(failed.error.as_deref(), Some("runtime failure"));
}

#[test]
fn suite_rejects_traversal_duplicate_ids_and_unpinned_downloads() {
    let mut suite = Suite {
        models: vec![Model {
            id: "../escape".into(),
            engine: Engine::Phonon { cache: ".".into() },
        }],
        fixtures: vec![fixture()],
        repetitions: 1,
        timeout_seconds: 60,
        threads: 2,
        maximum_wer: None,
    };
    assert!(suite.validate().is_err());
    suite.models[0].id = "phonon".into();
    assert!(suite.validate().is_ok());
    suite.models.push(suite.models[0].clone());
    assert!(suite.validate().is_err());
    suite.models.pop();
    suite.models[0].engine = Engine::MoonshineStreaming {
        directory: ".".into(),
        repository: "moonshine-ai/moonshine-streaming-tiny".into(),
        revision: "main".into(),
        license: "MIT".into(),
        artifacts: Vec::new(),
    };
    assert!(suite.validate().is_err());
}

#[test]
fn model_loading_failure_keeps_a_machine_readable_failed_report() -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let audio =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech-goforward.wav");
    let suite = Suite {
        models: vec![Model {
            id: "missing".into(),
            engine: Engine::Phonon {
                cache: temporary.path().join("missing"),
            },
        }],
        fixtures: vec![Fixture {
            id: "natural".into(),
            audio,
            sha256: "97ec638609b9c7eeb616764b4e5713d7ed2d0a8e1a2f8246631d9074d47aaf11".into(),
            expected: "go forward ten meters".into(),
            provenance: "PocketSphinx natural voice".into(),
            require_exact: false,
        }],
        repetitions: 1,
        timeout_seconds: 60,
        threads: 1,
        maximum_wer: None,
    };
    let path = temporary.path().join("report.json");
    worker(&suite, 0, &path)?;
    let report: ModelReport = read_json(&path)?;
    assert!(!report.passed);
    assert!(!report.completed);
    assert_eq!(report.samples.len(), 1);
    assert!(report.samples[0].error.is_some());
    assert_eq!(report.score.utterances, 0);
    Ok(())
}

#[test]
fn relative_paths_resolve_against_the_suite_directory() -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = temporary.path().join("suite.json");
    let suite = Suite {
        models: vec![Model {
            id: "phonon".into(),
            engine: Engine::Phonon {
                cache: "models".into(),
            },
        }],
        fixtures: vec![fixture()],
        repetitions: 1,
        timeout_seconds: 60,
        threads: 1,
        maximum_wer: None,
    };
    save(&path, &suite)?;
    let resolved = Suite::read(&path)?;
    assert_eq!(
        resolved.fixtures[0].audio,
        temporary.path().join("fixture.wav")
    );
    assert!(
        matches!(&resolved.models[0].engine, Engine::Phonon { cache } if cache == &temporary.path().join("models"))
    );
    Ok(())
}

#[test]
fn shipped_comparison_keeps_all_original_fixture_bytes() -> Result<(), String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evaluations/english.json");
    let suite = Suite::read(&path)?;
    for fixture in &suite.fixtures {
        let samples = audio(fixture)?;
        assert!(!samples.is_empty());
    }
    Ok(())
}

#[test]
fn timeout_stops_and_reaps_only_the_model_worker() -> Result<(), String> {
    if std::env::var_os("LINCE_SPEECH_EVALUATION_SLEEP").is_some() {
        std::thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    let child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
        .args(["--exact", "communication::speech::evaluation::tests::timeout_stops_and_reaps_only_the_model_worker"])
        .env("LINCE_SPEECH_EVALUATION_SLEEP", "1")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|error| error.to_string())?;
    let result = supervise(child, Duration::from_millis(100));
    assert!(result.timed_out);
    assert!(result.error.unwrap().contains("timeout"));
    assert!(result.status.is_some_and(|status| !status.success()));
    Ok(())
}

#[test]
fn failed_worker_launch_keeps_reports_and_continues_to_the_next_option() -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let suite = Suite {
        models: ["first", "second"]
            .into_iter()
            .map(|id| Model {
                id: id.into(),
                engine: Engine::Phonon {
                    cache: temporary.path().join("unused"),
                },
            })
            .collect(),
        fixtures: vec![fixture()],
        repetitions: 1,
        timeout_seconds: 1,
        threads: 1,
        maximum_wer: None,
    };
    let output = temporary.path().join("results");
    let report = run(suite, &output, &temporary.path().join("missing-worker"))?;
    assert!(!report.passed);
    assert_eq!(report.models.len(), 2);
    for model in report.models {
        assert!(!model.completed);
        assert!(model.error.unwrap().contains("Cannot launch"));
        assert!(output.join(format!("{}.json", model.model.id)).exists());
    }
    assert!(output.join("report.json").exists());
    Ok(())
}

#[test]
fn mandatory_fixture_accuracy_fails_independently_of_aggregate_word_error() {
    let mut fixture = fixture();
    fixture.expected.clear();
    fixture.require_exact = true;
    let hallucinated = evaluate(&mut FakeBackend(false), &fixture, &vec![0.0; 16_000], 0);
    assert!(hallucinated.recognition.is_some());
    assert!(hallucinated.error.is_none());
    assert!(!hallucinated.passed);
    assert!(hallucinated.score.unwrap().edits > 0);
}
