#[tokio::test]
async fn importing_a_script_rejects_the_file_before_importing_its_records() {
    let engine = engine::Engine::open_memory().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("script.lingua");
    let source = "print(query_command(@never-run));\nNever import (@never-import-script: 7) {\nData mixed with executable statements.\n}\n";
    std::fs::write(&path, source).unwrap();
    let report = engine
        .import_lingua_directory(directory.path())
        .await
        .unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert!(report.conflicts[0].reason.contains("do not execute"));
    assert!(
        store::records::resolve(&engine.store.pool, "never-import-script")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}
