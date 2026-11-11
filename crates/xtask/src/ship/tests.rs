use super::*;
use std::os::unix::fs::PermissionsExt;

struct Repository {
    directory: PathBuf,
    root: PathBuf,
    remote: PathBuf,
    gh: PathBuf,
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl Repository {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "lince-ship-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let root = directory.join("repository");
        let remote = directory.join("remote.git");
        output(
            Command::new("git")
                .args(["init", "--initial-branch=main"])
                .arg(&root),
        )
        .unwrap();
        output(
            Command::new("git")
                .args(["init", "--bare", "--initial-branch=main"])
                .arg(&remote),
        )
        .unwrap();
        git_text(&root, &["config", "user.name", "Ship Test"]).unwrap();
        git_text(&root, &["config", "user.email", "ship@example.invalid"]).unwrap();
        git_text(&root, &["config", "commit.gpgsign", "false"]).unwrap();
        output(git(&root).args(["remote", "add", "origin"]).arg(&remote)).unwrap();
        fs::write(root.join("tracked"), "before\n").unwrap();
        fs::write(root.join("deleted"), "delete me\n").unwrap();
        git_text(&root, &["add", "."]).unwrap();
        git_text(&root, &["commit", "-m", "initial"]).unwrap();
        let gh = directory.join("gh");
        let repository = Self {
            directory,
            root,
            remote,
            gh,
        };
        repository.workflow("success");
        repository
    }

    fn workflow(&self, conclusion: &str) {
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = repo ]; then echo test/repository; exit 0; fi\nsha=\nwhile [ $# -gt 0 ]; do\nif [ \"$1\" = --commit ]; then shift; sha=$1; fi\nshift\ndone\nprintf '[{{\"databaseId\":42,\"headSha\":\"%s\",\"status\":\"completed\",\"conclusion\":\"{conclusion}\",\"url\":\"https://example.invalid/run/42\",\"headBranch\":\"main\",\"event\":\"push\"}}]' \"$sha\"\n"
        );
        executable(&self.gh, &script);
    }

    fn run(&self, options: &Options) -> (Result<()>, Receipt) {
        let report = self.directory.join("report.json");
        let mut receipt = Receipt {
            activation: "not_attempted".into(),
            ..Receipt::default()
        };
        let tools = Tools {
            gh: self.gh.clone().into(),
            nix: "must-not-run-nix".into(),
            sudo: "must-not-run-sudo".into(),
        };
        let result = execute(&self.root, options, &tools, &report, &mut receipt);
        (result, receipt)
    }
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn options() -> Options {
    Options::parse(&["--no-install".into()]).unwrap()
}

fn run(revision: &str, status: &str, conclusion: &str, id: u64) -> Run {
    Run {
        database_id: id,
        head_sha: revision.into(),
        head_branch: "main".into(),
        event: "push".into(),
        status: status.into(),
        conclusion: Some(conclusion.into()),
        url: "https://example.invalid/run".into(),
    }
}

#[test]
fn checkout_push_includes_edits_additions_and_deletions_without_creating_worktrees() {
    let repository = Repository::new();
    fs::write(repository.root.join("tracked"), "tested\n").unwrap();
    fs::write(repository.root.join("new"), "included\n").unwrap();
    fs::remove_file(repository.root.join("deleted")).unwrap();
    git_text(&repository.root, &["add", "new"]).unwrap();
    let mut options = options();
    options.test = vec!["sh".into(), "-c".into(), "test \"$(cat tracked)\" = tested && test -f new && test ! -e deleted && test \"$(pwd)\" = \"$1\"".into(), "test".into(), repository.root.clone().into()];
    let (result, receipt) = repository.run(&options);
    result.unwrap();
    assert_eq!(receipt.stage, "complete");
    assert!(receipt.pushed);
    assert!(!receipt.installed);
    assert_eq!(receipt.activation, "not_attempted");
    assert!(!receipt.swap_complete);
    assert_eq!(receipt.run_id, Some(42));
    assert_eq!(
        git_text(&repository.remote, &["show", "main:tracked"]).unwrap(),
        "tested"
    );
    assert_eq!(
        git_text(&repository.remote, &["show", "main:new"]).unwrap(),
        "included"
    );
    assert!(git_text(&repository.remote, &["show", "main:deleted"]).is_err());
    assert_eq!(
        fs::read_to_string(repository.root.join("tracked")).unwrap(),
        "tested\n"
    );
    assert_eq!(
        git_text(&repository.root, &["diff", "--cached", "--name-only"]).unwrap(),
        ""
    );
    assert_eq!(
        git_text(&repository.root, &["diff", "--name-only"]).unwrap(),
        ""
    );
    assert!(!repository.root.join(".git/index.lock").exists());
    assert_eq!(
        git_text(&repository.root, &["worktree", "list", "--porcelain"])
            .unwrap()
            .matches("worktree ")
            .count(),
        1
    );
}

#[test]
fn failing_tests_leave_head_index_and_remote_untouched() {
    let repository = Repository::new();
    let head = git_text(&repository.root, &["rev-parse", "HEAD"]).unwrap();
    fs::write(repository.root.join("tracked"), "staged\n").unwrap();
    git_text(&repository.root, &["add", "tracked"]).unwrap();
    let index = fs::read(repository.root.join(".git/index")).unwrap();
    let mut options = options();
    options.test = vec!["sh".into(), "-c".into(), "exit 7".into()];
    let (result, receipt) = repository.run(&options);
    assert!(result.is_err());
    assert!(!receipt.pushed);
    assert_eq!(
        git_text(&repository.root, &["rev-parse", "HEAD"]).unwrap(),
        head
    );
    assert_eq!(fs::read(repository.root.join(".git/index")).unwrap(), index);
    assert!(git_text(&repository.remote, &["rev-parse", "refs/heads/main"]).is_err());
    assert!(!repository.root.join(".git/index.lock").exists());
}

#[test]
fn tests_that_modify_source_are_not_pushed() {
    let repository = Repository::new();
    let mut options = options();
    options.test = vec!["sh".into(), "-c".into(), "echo changed > tracked".into()];
    let (result, receipt) = repository.run(&options);
    assert!(
        result
            .unwrap_err()
            .contains("changed during checkout testing")
    );
    assert!(!receipt.pushed);
}

#[test]
fn workflow_failure_stops_before_installation() {
    let repository = Repository::new();
    repository.workflow("failure");
    let mut options = options();
    options.install = true;
    let (result, receipt) = repository.run(&options);
    assert!(result.unwrap_err().contains("completed with failure"));
    assert!(receipt.pushed);
    assert!(!receipt.installed);
    assert_eq!(receipt.stage, "waiting_for_workflow");
}

#[test]
fn only_the_matching_completed_success_can_advance() {
    let pending: Run = serde_json::from_str(r#"{"databaseId":1,"headSha":"wanted","headBranch":"main","event":"push","status":"queued","conclusion":null,"url":"https://example.invalid/run"}"#).unwrap();
    assert!(successful_run(vec![pending], "wanted").unwrap().is_none());
    for conclusion in [
        "failure",
        "cancelled",
        "timed_out",
        "skipped",
        "neutral",
        "action_required",
        "",
    ] {
        assert!(successful_run(vec![run("wanted", "completed", conclusion, 1)], "wanted").is_err());
    }
    assert!(
        successful_run(vec![run("other", "completed", "success", 1)], "wanted")
            .unwrap()
            .is_none()
    );
    assert!(
        successful_run(vec![run("wanted", "in_progress", "", 1)], "wanted")
            .unwrap()
            .is_none()
    );
    assert!(successful_run(vec![], "wanted").unwrap().is_none());
    assert!(
        successful_run(
            vec![
                run("wanted", "completed", "success", 1),
                run("wanted", "queued", "", 2)
            ],
            "wanted"
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(
        successful_run(vec![run("wanted", "completed", "success", 1)], "wanted")
            .unwrap()
            .unwrap()
            .database_id,
        1
    );
    let mut wrong_event = run("wanted", "completed", "success", 1);
    wrong_event.event = "workflow_dispatch".into();
    assert!(
        successful_run(vec![wrong_event], "wanted")
            .unwrap()
            .is_none()
    );
}

#[test]
fn remote_changes_are_never_force_pushed_away() {
    let repository = Repository::new();
    repository.run(&options()).0.unwrap();
    let parent = git_text(&repository.root, &["rev-parse", "HEAD"]).unwrap();
    let tree = git_text(&repository.root, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let remote_commit = git_text(
        &repository.root,
        &["commit-tree", &tree, "-p", &parent, "-m", "other publisher"],
    )
    .unwrap();
    output(git(&repository.root).args([
        "push",
        "origin",
        &format!("{remote_commit}:refs/heads/main"),
    ]))
    .unwrap();
    fs::write(repository.root.join("tracked"), "new local edit\n").unwrap();
    let (result, receipt) = repository.run(&options());
    assert!(result.is_err());
    assert!(!receipt.pushed);
    assert!(!receipt.installed);
    assert_eq!(
        git_text(&repository.remote, &["rev-parse", "refs/heads/main"]).unwrap(),
        remote_commit
    );
    assert_eq!(
        git_text(&repository.root, &["show", "HEAD:tracked"]).unwrap(),
        "new local edit"
    );
}

#[test]
fn missing_workflow_times_out_without_installing() {
    let repository = Repository::new();
    executable(
        &repository.gh,
        "#!/bin/sh\nif [ \"$1\" = repo ]; then echo test/repository; else echo '[]'; fi\n",
    );
    let mut options = options();
    options.timeout = Duration::from_millis(20);
    options.interval = Duration::from_millis(5);
    let (result, receipt) = repository.run(&options);
    assert!(result.unwrap_err().contains("Timed out"));
    assert!(!receipt.installed);
}

#[test]
fn unsupported_branch_and_busy_index_are_rejected() {
    let repository = Repository::new();
    git_text(&repository.root, &["switch", "-c", "dev"]).unwrap();
    assert!(
        repository
            .run(&options())
            .0
            .unwrap_err()
            .contains("requires main")
    );
    git_text(&repository.root, &["switch", "main"]).unwrap();
    fs::write(repository.root.join(".git/index.lock"), "other process").unwrap();
    assert!(
        repository
            .run(&options())
            .0
            .unwrap_err()
            .contains("Cannot lock")
    );
    assert_eq!(
        fs::read_to_string(repository.root.join(".git/index.lock")).unwrap(),
        "other process"
    );
}

#[test]
fn installation_verifies_the_locked_release_before_switching() {
    let repository = Repository::new();
    let bundle = repository.directory.join("bundle");
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("binary.json"), r#"{"revision":"wanted"}"#).unwrap();
    let nix = repository.directory.join("nix");
    let sudo = repository.directory.join("sudo");
    let marker = repository.directory.join("switched");
    executable(
        &nix,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = eval ]; then printf '%s' '{}'; fi\n",
            bundle.display()
        ),
    );
    executable(
        &sudo,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            marker.display()
        ),
    );
    let tools = Tools {
        gh: repository.gh.clone().into(),
        nix: nix.into(),
        sudo: sudo.into(),
    };
    let mut options = options();
    options.flake = format!("{}#test-host", repository.directory.display());
    assert!(
        install(&tools, &repository.root, &options, "different")
            .unwrap_err()
            .contains("does not match")
    );
    assert!(!marker.exists());
    install(&tools, &repository.root, &options, "wanted").unwrap();
    let invoked = fs::read_to_string(marker).unwrap();
    assert!(invoked.starts_with("-n\nnixos-rebuild\nswitch\n"));
    assert!(invoked.contains("--no-update-lock-file"));
    assert!(invoked.contains("#test-host"));
}

#[test]
fn a_successful_install_is_never_reported_as_a_completed_swap() {
    let repository = Repository::new();
    let revision = git_text(&repository.root, &["rev-parse", "HEAD"]).unwrap();
    let bundle = repository.directory.join("bundle");
    fs::create_dir(&bundle).unwrap();
    fs::write(
        bundle.join("binary.json"),
        serde_json::to_vec(&serde_json::json!({"revision": revision})).unwrap(),
    )
    .unwrap();
    let nix = repository.directory.join("nix");
    let sudo = repository.directory.join("sudo");
    executable(
        &nix,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = eval ]; then printf '%s' '{}'; fi\n",
            bundle.display()
        ),
    );
    executable(&sudo, "#!/bin/sh\nexit 0\n");
    let tools = Tools {
        gh: repository.gh.clone().into(),
        nix: nix.into(),
        sudo: sudo.into(),
    };
    let mut options = options();
    options.install = true;
    options.flake = format!("{}#test-host", repository.directory.display());
    let mut receipt = Receipt {
        activation: "not_attempted".into(),
        ..Receipt::default()
    };
    let report = repository.directory.join("report.json");
    execute(&repository.root, &options, &tools, &report, &mut receipt).unwrap();
    assert_eq!(receipt.stage, "installed_activation_pending");
    assert!(receipt.installed);
    assert!(!receipt.swap_complete);
    assert_eq!(receipt.activation, "not_attempted");
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(saved["swap_complete"], false);
    assert_eq!(saved["stage"], "installed_activation_pending");
}

#[test]
fn options_validate_deadlines_and_keep_test_arguments_literal() {
    for args in [
        vec!["--"],
        vec!["--timeout-seconds", "0"],
        vec!["--poll-seconds", "-1"],
        vec!["--unknown"],
        vec!["--message", " "],
    ] {
        assert!(Options::parse(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err());
    }
    let options = Options::parse(&["--".into(), "printf".into(), "$(false)".into()]).unwrap();
    assert_eq!(
        options.test,
        vec![OsString::from("printf"), OsString::from("$(false)")]
    );
}
