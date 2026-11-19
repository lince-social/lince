use nucleus::plugins::{Contribution, Manifest};
use std::path::Path;

fn package(root: &Path, directory: &str, id: &str, executable: &str) {
    let directory = root.join(directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("runner"), "installed executable").unwrap();
    let manifest = Manifest {
        format: nucleus::plugins::FORMAT.into(),
        id: id.into(),
        name: "Test package".into(),
        version: 1,
        compatibility: nucleus::plugins::Compatibility::native(),
        executable: executable.into(),
        arguments: Vec::new(),
        contributions: vec![Contribution::Action {
            id: "hello".into(),
            name: "Hello".into(),
            arguments: Vec::new(),
        }],
        licenses: vec![nucleus::sand_package::License {
            name: "Test license".into(),
            text: "Test license text".into(),
        }],
        credits: Vec::new(),
    };
    std::fs::write(
        directory.join("plugin.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn many_packages_are_registered_without_running_executables() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), "one", "one", "runner");
    package(root.path(), "two", "two", "runner");
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(host.packages().len(), 2);
    assert!(host.diagnostics().is_empty());
    assert!(host.contribution("two/hello").is_ok());
    assert!(host.packages().iter().all(|package| !package.enabled));
    assert!(!root.path().join(".state").exists());
}

#[tokio::test]
async fn duplicates_and_path_escapes_do_not_hide_healthy_packages() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), "one", "duplicate", "runner");
    package(root.path(), "two", "duplicate", "runner");
    package(root.path(), "three", "escape", "../one/runner");
    package(root.path(), "four", "healthy", "runner");
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(host.packages().len(), 1);
    assert_eq!(host.packages()[0].manifest.id, "healthy");
    assert_eq!(host.diagnostics().len(), 2);
}

#[tokio::test]
async fn changed_executables_and_unapproved_digests_cannot_run() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), "one", "one", "runner");
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    let package = host.packages().remove(0);
    let error = host
        .call("one", "different", "hello", "invoke", serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(error.contains("changed"));
    std::fs::write(root.path().join("one/runner"), "replacement executable").unwrap();
    let error = host
        .call(
            "one",
            &package.digest,
            "hello",
            "invoke",
            serde_json::json!({}),
        )
        .await
        .unwrap_err();
    assert!(error.contains("files changed"));
}

#[tokio::test]
async fn nested_identifiers_cannot_register_the_same_schema_namespace() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), "one", "example", "runner");
    package(root.path(), "two", "example.nostr", "runner");
    package(root.path(), "three", "healthy", "runner");
    for directory in ["one", "two"] {
        let path = root.path().join(directory).join("plugin.json");
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest.contributions.push(Contribution::Schema {
            id: "note".into(),
            name: "Note".into(),
            namespace: "plugin.example.nostr.note".into(),
            fields: Vec::new(),
        });
        manifest.validate().unwrap();
        std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(host.packages().len(), 1);
    assert_eq!(host.packages()[0].manifest.id, "healthy");
    assert!(host.diagnostics()[0].contains("Conflicting schema namespace"));
}

#[tokio::test]
async fn frames_are_bounded_even_when_a_plugin_never_sends_a_newline() {
    let bytes = vec![b'x'; nucleus::plugins::MAX_BYTES + 1];
    let error = plugins::read_frame(&mut &bytes[..]).await.unwrap_err();
    assert!(error.contains("size limit"));
    let mut valid = &b"{\"result\":1}\nsecond\n"[..];
    assert_eq!(
        plugins::read_frame(&mut valid).await.unwrap(),
        b"{\"result\":1}\n"
    );
    assert_eq!(plugins::read_frame(&mut valid).await.unwrap(), b"second\n");
    assert!(plugins::read_frame(&mut valid).await.is_err());
}

#[test]
fn schemas_and_field_types_are_checked_before_registration() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), "one", "one", "runner");
    let mut manifest: Manifest =
        serde_json::from_slice(&std::fs::read(root.path().join("one/plugin.json")).unwrap())
            .unwrap();
    manifest.contributions.push(Contribution::Sync {
        id: "notes".into(),
        name: "Notes".into(),
        incoming: true,
        outgoing: false,
        replay_safe: false,
        settings: Vec::new(),
        schema: "missing".into(),
        document: None,
    });
    assert!(manifest.validate().is_err());
    assert!(!nucleus::plugins::valid_id(".."));
    assert!(!nucleus::plugins::valid_id("../../state"));
    let fields = vec![nucleus::plugins::Field {
        id: "count".into(),
        name: "Count".into(),
        kind: nucleus::plugins::FieldType::Integer,
        required: true,
        default: serde_json::json!(1),
    }];
    assert!(nucleus::plugins::validate_values(&fields, &serde_json::json!({"count":1})).is_ok());
    assert!(nucleus::plugins::validate_values(&fields, &serde_json::json!({"count":"1"})).is_err());
    assert!(
        nucleus::plugins::validate_values(&fields, &serde_json::json!({"count":1,"secret":"no"}))
            .is_err()
    );
}

#[tokio::test]
async fn platform_interfaces_versions_and_dependency_cycles_are_reported_without_execution() {
    let root = tempfile::tempdir().unwrap();
    for id in [
        "base",
        "dependent",
        "wrong-os",
        "wrong-api",
        "missing",
        "cycle-a",
        "cycle-b",
    ] {
        package(root.path(), id, id, "runner");
    }
    for (id, dependencies, operating_systems, interfaces) in [
        (
            "dependent",
            vec![("base", 1)],
            vec![std::env::consts::OS],
            vec![nucleus::plugins::PROTOCOL],
        ),
        ("wrong-os", vec![], vec!["unsupported-os"], vec![]),
        ("wrong-api", vec![], vec![], vec!["lince.plugin-rpc.v999"]),
        ("missing", vec![("base", 2)], vec![], vec![]),
        ("cycle-a", vec![("cycle-b", 1)], vec![], vec![]),
        ("cycle-b", vec![("cycle-a", 1)], vec![], vec![]),
    ] {
        let path = root.path().join(id).join("plugin.json");
        let mut manifest: Manifest =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest.compatibility.dependencies = dependencies
            .into_iter()
            .map(|(id, version)| (id.into(), version))
            .collect();
        manifest.compatibility.operating_systems =
            operating_systems.into_iter().map(str::to_owned).collect();
        manifest.compatibility.interfaces = interfaces.into_iter().map(str::to_owned).collect();
        std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(host.packages().len(), 2);
    assert!(host.contribution("dependent/hello").is_ok());
    assert_eq!(host.diagnostics().len(), 5);
    assert!(!root.path().join(".state").exists());
}

#[tokio::test]
async fn local_install_update_reload_and_remove_preserve_private_plugin_state() {
    let source = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    package(source.path(), "package", "example", "runner");
    let host = plugins::Host::discover(root.path().to_path_buf())
        .await
        .unwrap();
    host.install(source.path().join("package")).await.unwrap();
    let first = host.packages().remove(0);
    let state = root.path().join(".state/example");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("identity"), "private saved identity").unwrap();
    std::fs::write(source.path().join("package/runner"), "updated executable").unwrap();
    host.install(source.path().join("package")).await.unwrap();
    let updated = host.packages().remove(0);
    assert_ne!(first.digest, updated.digest);
    assert!(host.remove("example", &first.digest).await.is_err());
    host.remove("example", &updated.digest).await.unwrap();
    assert!(host.packages().is_empty());
    assert_eq!(
        std::fs::read_to_string(state.join("identity")).unwrap(),
        "private saved identity"
    );
    assert!(host.contribution("example/hello").is_err());
}
