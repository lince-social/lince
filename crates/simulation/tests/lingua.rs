use nucleus::simulation::{ArtifactError, Observation, ReplayStatus, Verdict};
use simulation::{artifacts, fixtures, lingua};

fn package(root: &std::path::Path) -> simulation::scenario::Scenario {
    std::fs::create_dir_all(root.join("records")).unwrap();
    std::fs::write(root.join("records/stock.lingua"), "Stock (@stock: 10, is #task) {\nAvailable stock.\n}\nRelated (@related: 0, #related @stock) {\n}\n").unwrap();
    std::fs::write(root.join("frequency.lingua"), "Frequency daily {\n title \"Daily\"\n quantity 1\n every 1 day\n timezone \"UTC\"\n next_at 2030-01-02T00:00:00Z\n}\n").unwrap();
    let consequences = serde_json::to_string(&vec![nucleus::karma::Consequence::AddQuantity {
        delta: Some(nucleus::DecimalValue::parse_inferred("-3").unwrap()),
    }])
    .unwrap();
    std::fs::write(root.join("automation.lingua"), format!("Karma local {{\n Rules {{\n Rule spend {{\n quantity 1\n record @stock\n condition \"\"\"freq(@daily)\"\"\"\n gate !=0\n carry value\n consequences \"\"\"{consequences}\"\"\"\n }}\n }}\n}}\n")).unwrap();
    let mut scenario = fixtures::daily();
    scenario.cells[0].seed.clear();
    scenario.cells[0].lingua = lingua::package(root).unwrap();
    scenario
}

#[tokio::test]
async fn lingua_packages_ingest_real_rules_preserve_sources_and_replay_without_the_originals() {
    let sources = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut scenario = package(sources.path());
    let originals: Vec<_> = scenario.cells[0]
        .lingua
        .iter()
        .map(|file| {
            (
                file.name.clone(),
                std::fs::read(sources.path().join(&file.file)).unwrap(),
            )
        })
        .collect();
    scenario.cells[0].lingua.reverse();
    std::fs::write(
        sources.path().join("daily.json"),
        serde_json::to_vec(&scenario).unwrap(),
    )
    .unwrap();
    let runs = simulation::cli::case_results(sources.path(), output.path())
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    let path = run.run.clone();
    assert_eq!(run.result.verdict, Verdict::Passed, "{:?}", run.result);
    let bundle = artifacts::load(&path).unwrap();
    assert!(bundle.events.iter().any(|event| matches!(&event.observation,
        Observation::LinguaImported { files, created, .. } if files.len() == 3 && created.len() == 4)));
    for (name, bytes) in originals {
        assert_eq!(std::fs::read(sources.path().join(&name)).unwrap(), bytes);
        let file = bundle.scenario.cells[0]
            .lingua
            .iter()
            .find(|file| file.name == name)
            .unwrap();
        assert_eq!(std::fs::read(path.join(&file.file)).unwrap(), bytes);
        assert_eq!(bundle.manifest.files.get(&file.file), Some(&file.hash));
    }
    assert!(path.join("seed/a.sqlite").is_file());
    sources.close().unwrap();
    assert!(matches!(
        artifacts::replay(&path, &output.path().join("replay"))
            .await
            .unwrap(),
        ReplayStatus::Verified { .. }
    ));
    let file = &bundle.scenario.cells[0].lingua[0].file;
    std::fs::write(path.join(file), "Changed").unwrap();
    assert!(matches!(
        artifacts::load(&path),
        Err(ArtifactError::Corrupt { .. })
    ));
}

#[tokio::test]
async fn invalid_or_unsupported_packages_never_record_a_passing_run() {
    let sources = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut scenario = package(sources.path());
    scenario.cells[0].lingua[0].hash =
        artifacts::file_hash(&sources.path().join("frequency.lingua")).unwrap();
    assert!(
        artifacts::execute_with_sources(scenario, &output.path().join("hash"), sources.path())
            .await
            .is_err()
    );
    assert!(!output.path().join("hash/result.json").exists());
    for (index, source) in ["Broken (@broken: nope) {\n}\n", "Extension @stock \"example\" { field 1 }\n", "Karma local { Rules { Rule missing { quantity 1 record @missing consequences \"\"\"[]\"\"\" } } }\n"].iter().enumerate() {
        std::fs::write(sources.path().join("invalid.lingua"), source).unwrap();
        let mut scenario = package(sources.path());
        scenario.cells[0].lingua = lingua::package(sources.path()).unwrap();
        let run = output.path().join(format!("invalid-{index}"));
        let error = artifacts::execute_with_sources(scenario, &run, sources.path()).await.err().unwrap();
        assert!(error.to_string().contains("Lingua seed refused"), "{error}");
        assert!(!run.join("result.json").exists());
    }
}

#[test]
fn packages_reject_escaping_paths_duplicates_and_empty_directories() {
    let directory = tempfile::tempdir().unwrap();
    assert!(lingua::package(directory.path()).is_err());
    let original = package(directory.path());
    for field in ["name", "file"] {
        let mut scenario = serde_json::to_value(&original).unwrap();
        scenario["cells"][0]["lingua"][0][field] = "../escape.lingua".into();
        assert!(
            serde_json::from_value::<simulation::scenario::Scenario>(scenario)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut scenario = original;
    let duplicate = scenario.cells[0].lingua[0].clone();
    scenario.cells[0].lingua.push(duplicate);
    assert!(scenario.validate().is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            directory.path().join("records"),
            directory.path().join("link"),
        )
        .unwrap();
        assert!(lingua::package(directory.path()).is_err());
    }
}
