use simulation::{artifacts, fixtures, limits, proposal::Record};

#[tokio::test]
async fn oversized_cli_case_is_rejected_before_creating_a_run() {
    let directory = tempfile::tempdir().unwrap();
    let cases = directory.path().join("cases");
    std::fs::create_dir(&cases).unwrap();
    let file = std::fs::File::create(cases.join("huge.json")).unwrap();
    file.set_len(limits::JSON_BYTES + 1).unwrap();
    let output = directory.path().join("output");
    let error = match simulation::cli::case_results(&cases, &output).await {
        Ok(_) => panic!("oversized case accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("JSON file exceeds"), "{error}");
    assert!(!output.exists());
}

#[tokio::test]
async fn oversized_record_and_many_assertions_are_refused_before_capture() {
    let directory = tempfile::tempdir().unwrap();
    let world = simulation::world::World::open(fixtures::daily(), &directory.path().join("source"))
        .await
        .unwrap();
    let store = &world.nodes["a"].engine().store;
    let record = store::records::resolve(&store.pool, "stock")
        .await
        .unwrap()
        .unwrap();
    store::sqlx::query("UPDATE record SET body = CAST(zeroblob(?) AS TEXT) WHERE uid = ?")
        .bind(2 * limits::CAPTURE_BYTES as i64)
        .bind(&record.uid)
        .execute(&store.pool)
        .await
        .unwrap();
    let error = Record::read_bounded(store, &record.uid, limits::CAPTURE_BYTES)
        .await
        .unwrap_err();
    assert!(limits::is_exceeded(&error), "{error}");
    let proposal = simulation::proposal::Proposal::capture(
        world.nodes["a"].engine(),
        store,
        None,
        "a",
        world.scenario.start_ms,
        std::slice::from_ref(&record.uid),
    )
    .await
    .unwrap_err();
    assert!(limits::is_exceeded(&proposal), "{proposal}");
    store::sqlx::query("UPDATE record SET body = '' WHERE uid = ?")
        .bind(&record.uid)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("INSERT INTO record_extension(record_uid,namespace,fds) VALUES(?,'work',json_object('text',CAST(zeroblob(?) AS TEXT))) ON CONFLICT(record_uid,namespace) DO UPDATE SET fds = excluded.fds").bind(&record.uid).bind(2 * limits::CAPTURE_BYTES as i64).execute(&store.pool).await.unwrap();
    assert!(limits::is_exceeded(
        &Record::read_bounded(store, &record.uid, limits::CAPTURE_BYTES)
            .await
            .unwrap_err()
    ));
    store::sqlx::query("DELETE FROM record_extension WHERE record_uid = ? AND namespace = 'work'")
        .bind(&record.uid)
        .execute(&store.pool)
        .await
        .unwrap();
    for index in 0..600 {
        let predicate = nucleus::new_uid("c");
        store::sqlx::query(
            "INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,'2026-01-01T00:00:00Z')",
        )
        .bind(&predicate)
        .bind(format!("bounded-assertion-{index}"))
        .execute(&store.pool)
        .await
        .unwrap();
        store::sqlx::query("INSERT INTO record_assertion(uid,subject_uid,predicate_uid,role,created_at) VALUES(?,?,?,'ordinary','2026-01-01T00:00:00Z')").bind(nucleus::new_uid("a")).bind(&record.uid).bind(&predicate).execute(&store.pool).await.unwrap();
    }
    assert!(limits::is_exceeded(
        &Record::read_bounded(store, &record.uid, 4096)
            .await
            .unwrap_err()
    ));
    assert_eq!(
        Record::read_bounded(store, &record.uid, limits::CAPTURE_BYTES)
            .await
            .unwrap()
            .assertions
            .len(),
        600
    );
    store.pool.close().await;
}

#[tokio::test]
async fn unrelated_manifest_entry_is_rejected_before_opening_it() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("run");
    artifacts::execute(fixtures::daily(), &output)
        .await
        .unwrap();
    let bundle = artifacts::load(&output).unwrap();
    assert!(limits::is_exceeded(
        &simulation::record_evidence::occurrence_evidence(&bundle.events, 1).unwrap_err()
    ));
    let mut manifest: artifacts::Manifest =
        limits::read_json(&output.join("manifest.json"), limits::JSON_BYTES).unwrap();
    manifest.files.insert(
        "unrelated-huge-file.sqlite".into(),
        manifest.scenario.clone(),
    );
    artifacts::atomic(&output.join("manifest.json"), &manifest).unwrap();
    assert!(
        matches!(artifacts::load(&output),Err(nucleus::simulation::ArtifactError::Invalid {path,reason}) if path == "manifest.files" && reason.contains("unrelated"))
    );
}
