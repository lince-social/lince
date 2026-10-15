use std::process::Command;

fn command(config: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lince"));
    command
        .args(["lingua"])
        .env_remove("LINCE_DATA_DIR")
        .env("XDG_CONFIG_HOME", config);
    command
}

#[test]
fn lince_runs_pure_lingua_without_initializing_an_instance() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("unconfigured");
    let output = command(&config)
        .args(["eval", "0.1 + 0.2 == 0.3"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "true\n");
    let output = command(&config).args(["eval", "-1 + 2"]).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n");
    assert!(!config.exists());
}

#[test]
fn default_missing_database_is_a_clear_error_without_creating_files() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("unconfigured");
    let output = command(&config)
        .args(["eval", "quantity(@apples)"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("Cannot query Lince database"), "{error}");
    assert!(
        error.contains(&config.join("lince/lince.db").display().to_string()),
        "{error}"
    );
    assert!(!config.exists());
}

#[test]
fn passing_a_file_runs_it_and_checking_it_does_not_execute_it() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("unconfigured");
    let path = directory.path().join("calculation.lingua");
    std::fs::write(
        &path,
        "dir = \"absent\"; let amount = 10; print(amount + 2); print(false && quantity(@missing));",
    )
    .unwrap();
    let output = command(&config).arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "12\nfalse\n");
    std::fs::write(&path, "print(quantity(@missing));").unwrap();
    let output = command(&config).arg("check").arg(&path).output().unwrap();
    assert!(output.status.success());
    assert!(!config.exists());
    assert!(!directory.path().join("absent").exists());
}

#[test]
fn control_flow_collects_an_array_without_initializing_an_instance() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("unconfigured");
    let path = directory.path().join("collection.lingua");
    let source = "dir = \"missing\"; let result = []; for value in [1, 2, 3, 4] { if value % 2 == 0 { let next = value; next = next * 10; result = push(result, next); } else { if false { print(quantity(@missing)); } } } if len(result) == 0 { print(\"empty\"); } else { print(result); }";
    std::fs::write(&path, source).unwrap();
    let output = command(&config).arg("check").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = command(&config).arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "[20, 40]\n");
    let output = command(&config).arg("fmt").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let formatted = String::from_utf8(output.stdout).unwrap();
    assert_eq!(anicca::canonicalize(&formatted).unwrap(), formatted);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    assert!(!directory.path().join("missing").exists());
    assert!(!config.exists());
}

#[test]
fn query_filter_transform_and_print_use_the_selected_readonly_database() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("unconfigured");
    let instance = directory.path().join("instance");
    std::fs::create_dir(&instance).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let store =
            store::Store::open(&format!("sqlite://{}", instance.join("lince.db").display()))
                .await
                .unwrap();
        for (slug, head, amount) in [
            ("apples", "Apples", "9007199254740993"),
            ("bananas", "Bananas", "-1"),
        ] {
            store::records::create(
                &store.pool,
                store::records::NewRecord {
                    slug: Some(slug),
                    kind: nucleus::RecordKind::Plain,
                    head,
                    body: "",
                    quantity: nucleus::DecimalValue::parse_inferred(amount).unwrap(),
                },
            )
            .await
            .unwrap();
        }
        store.pool.close().await;
    });
    let path = directory.path().join("query-loop.lingua");
    let source = "dir = \"instance\"; let result = []; for row in query({source: \"record\", where: [{any: [{slug_eq: \"apples\"}, {slug_eq: \"bananas\"}]}], fields: [\"uid\", \"head\"], limit: 10}) { let value = quantity(get(row, \"uid\")); if value > 0 { value = value + 2; result = push(result, {head: get(row, \"head\"), quantity: value}); } } print(result);";
    std::fs::write(&path, source).unwrap();
    let output = command(&config).arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[{\"head\": \"Apples\", \"quantity\": 9007199254740995}]\n"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    assert!(!config.exists());
    runtime.block_on(async {
        let store = store::Store::open_readonly(&instance.join("lince.db"))
            .await
            .unwrap();
        let row = store::records::resolve(&store.pool, "apples")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.quantity.to_string(), "9007199254740993");
        store.pool.close().await;
    });
}
