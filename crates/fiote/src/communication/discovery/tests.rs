use super::*;

fn registry() -> Registry {
    Registry::parse(br#"{"version":"1.0.0","agents":[{"id":"codex-acp","name":"Codex","version":"2.1.1","distribution":{"npx":{"package":"@agentclientprotocol/codex-acp@2.1.1"}}}]}"#).unwrap()
}

#[cfg(unix)]
fn program(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn bundled_registry_uses_ecosystem_metadata_and_rejects_unbounded_or_ambiguous_entries() {
    let bundled = Registry::bundled();
    assert!(bundled.agents.iter().any(|agent| agent.id == "codex-acp"));
    assert!(Registry::parse(&vec![b' '; MAX_REGISTRY + 1]).is_err());
    let mut duplicate = registry();
    duplicate.agents.push(duplicate.agents[0].clone());
    assert!(Registry::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    duplicate.agents[0].id = "../executable".into();
    assert!(Registry::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut injected = registry();
    injected.agents[0].distribution["npx"]["package"] = "adapter; touch file".into();
    assert!(Registry::parse(&serde_json::to_vec(&injected).unwrap()).is_err());
    injected.agents[0].distribution["npx"]["package"] = "adapter@1.0.0".into();
    injected.agents[0].distribution["npx"]["args"] = serde_json::json!(["valid", 7]);
    assert!(Registry::parse(&serde_json::to_vec(&injected).unwrap()).is_err());
}

#[cfg(unix)]
#[test]
fn discovery_distinguishes_missing_bridge_and_builds_a_profile_without_running_or_reading_credentials()
 {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("must-not-run");
    program(
        &root.path().join("codex"),
        &format!("#!/bin/sh\ntouch {}\n", marker.display()),
    );
    let search = Search {
        paths: vec![root.path().into()],
        directory: root.path().into(),
    };
    let missing = search.scan(&registry());
    assert_eq!(
        missing.candidates[0].availability,
        Availability::BridgeMissing
    );
    assert!(missing.candidates[0].config.is_none());
    program(&root.path().join("codex-acp"), "#!/bin/sh\nexit 0\n");
    let discovered = search.scan(&registry());
    let candidate = &discovered.candidates[0];
    assert_eq!(candidate.availability, Availability::Installed);
    let mut config = candidate.config.clone().unwrap();
    config.validate().unwrap();
    assert_eq!(
        config.environment["CODEX_PATH"],
        root.path().join("codex").to_string_lossy()
    );
    assert_eq!(config.directory, root.path());
    assert_eq!(config.environment["PATH"], root.path().to_string_lossy());
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn package_binary_name_can_differ_and_registry_launch_arguments_and_environment_are_retained() {
    let root = tempfile::tempdir().unwrap();
    program(&root.path().join("my-harness"), "#!/bin/sh\nexit 0\n");
    let registry = Registry::parse(br#"{"version":"1.0.0","agents":[{"id":"my-harness","name":"Custom","version":"1","distribution":{"npx":{"package":"@example/package-cli@1","args":["--acp"],"env":{"DISABLE_UPDATE":"1"}}}}]}"#).unwrap();
    let discovered = Search {
        paths: vec![root.path().into()],
        directory: root.path().into(),
    }
    .scan(&registry);
    let config = discovered.candidates[0].config.as_ref().unwrap();
    assert_eq!(config.args, ["--acp"]);
    assert_eq!(config.environment["DISABLE_UPDATE"], "1");
    let invalid = serde_json::json!({"version":"1.0.0","agents":[{"id":"my-harness","name":"Custom","version":"1","distribution":{"npx":{"package":"cli@1","env":{"HOME":"/somewhere-else"}}}}]});
    assert!(Registry::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
}

#[cfg(unix)]
#[test]
fn broken_symlinks_non_executables_and_missing_script_runtime_are_not_ready() {
    let root = tempfile::tempdir().unwrap();
    let search = Search {
        paths: vec![root.path().into()],
        directory: root.path().into(),
    };
    std::os::unix::fs::symlink(root.path().join("gone"), root.path().join("codex-acp")).unwrap();
    assert_eq!(
        search.scan(&registry()).candidates[0].availability,
        Availability::Unavailable
    );
    std::fs::remove_file(root.path().join("codex-acp")).unwrap();
    std::fs::write(root.path().join("codex-acp"), "unexecutable").unwrap();
    assert_eq!(
        search.scan(&registry()).candidates[0].availability,
        Availability::Unavailable
    );
    program(&root.path().join("codex-acp"), "#!/usr/bin/env node\n");
    assert_eq!(
        search.scan(&registry()).candidates[0].availability,
        Availability::RuntimeMissing
    );
    program(&root.path().join("node"), "#!/bin/sh\nexit 0\n");
    assert_eq!(
        search.scan(&registry()).candidates[0].availability,
        Availability::Installed
    );
}
