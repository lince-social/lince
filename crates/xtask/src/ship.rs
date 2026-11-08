use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug)]
struct Options {
    message: String,
    flake: String,
    install: bool,
    timeout: Duration,
    interval: Duration,
    report: Option<PathBuf>,
    test: Vec<OsString>,
}

impl Options {
    fn parse(arguments: &[OsString]) -> Result<Self> {
        let mut options = Self {
            message: "Ship tested workspace snapshot".into(),
            flake: "/home/user/git/xaviduds/nixos#computer".into(),
            install: true,
            timeout: Duration::from_secs(14400),
            interval: Duration::from_secs(15),
            report: None,
            test: Vec::new(),
        };
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--") => {
                    options.test = arguments.cloned().collect();
                    if options.test.is_empty() {
                        return Err("-- requires a test executable and optional arguments".into());
                    }
                    break;
                }
                Some("--no-install") => options.install = false,
                Some(
                    flag @ ("--message" | "--nixos-flake" | "--timeout-seconds" | "--poll-seconds"
                    | "--report"),
                ) => {
                    let value = arguments
                        .next()
                        .and_then(|value| value.to_str())
                        .ok_or_else(|| format!("{flag} requires a UTF-8 value"))?;
                    match flag {
                        "--message" => options.message = value.into(),
                        "--nixos-flake" => options.flake = value.into(),
                        "--report" => options.report = Some(value.into()),
                        _ => {
                            let seconds = value
                                .parse::<u64>()
                                .ok()
                                .filter(|value| *value > 0 && *value <= 86400)
                                .ok_or_else(|| format!("{flag} must be between 1 and 86400"))?;
                            if flag == "--timeout-seconds" {
                                options.timeout = Duration::from_secs(seconds);
                            } else {
                                options.interval = Duration::from_secs(seconds);
                            }
                        }
                    }
                }
                _ => {
                    return Err(format!(
                        "unknown ship option: {}",
                        argument.to_string_lossy()
                    ));
                }
            }
        }
        if options.message.trim().is_empty() {
            return Err("the commit message cannot be empty".into());
        }
        Ok(options)
    }
}

#[derive(Default, Serialize)]
struct Receipt {
    stage: String,
    revision: Option<String>,
    run_id: Option<u64>,
    workflow_url: Option<String>,
    tests: Option<String>,
    pushed: bool,
    installed: bool,
    activation: String,
    swap_complete: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    database_id: u64,
    head_sha: String,
    status: String,
    conclusion: Option<String>,
    url: String,
    head_branch: String,
    event: String,
}

struct Tools {
    gh: OsString,
    nix: OsString,
    sudo: OsString,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            gh: "gh".into(),
            nix: "nix".into(),
            sudo: "sudo".into(),
        }
    }
}

fn output(command: &mut Command) -> Result<String> {
    command.stdin(Stdio::null());
    let output = command
        .output()
        .map_err(|error| format!("Cannot start {:?}: {error}", command.get_program()))?;
    if !output.status.success() {
        return Err(format!(
            "{:?} failed ({}): {}",
            command.get_program(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn git(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root);
    command
}

fn git_text(root: &Path, arguments: &[&str]) -> Result<String> {
    output(git(root).args(arguments)).map(|value| value.trim().to_owned())
}

fn stage(receipt: &mut Receipt, report: &Path, name: &str) -> Result<()> {
    receipt.stage = name.into();
    save(receipt, report)?;
    eprintln!("ship: {name}");
    Ok(())
}

fn save(receipt: &Receipt, report: &Path) -> Result<()> {
    let temporary = report.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, report).map_err(|error| error.to_string())
}

struct Snapshot {
    root: PathBuf,
    directory: PathBuf,
    index_lock: PathBuf,
    lock_held: bool,
    worktree: bool,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if self.worktree {
            let _ = output(
                git(&self.root)
                    .args(["worktree", "remove", "--force"])
                    .arg(self.directory.join("source")),
            );
        }
        if self.lock_held {
            let _ = fs::remove_file(&self.index_lock);
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub fn dispatch(root: &Path, arguments: &[OsString]) -> Result<()> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "help" | "-h")) {
        println!(
            "cargo xtask ship [--message TEXT] [--nixos-flake PATH#HOST] [--no-install] [--timeout-seconds N] [--poll-seconds N] [--report PATH] [-- TEST_EXECUTABLE ARGS...]"
        );
        println!(
            "Commits all nonignored changes on main using a separate index and detached test worktree. The default test is the placeholder 1 + 1 == 2. A custom test runs in the snapshot with no shell expansion. Concurrent edits remain uncommitted. Git index operations are locked until the tested snapshot is committed."
        );
        println!(
            "Waits only for this commit's successful build.yml push run. Installation performs the lin equivalent with a revision check before switching and requires existing sudo authorization. It does not restart Park or claim live activation. The JSON receipt distinguishes installed from active; errors exit nonzero. Default receipt: .git/lince-ship.json."
        );
        return Ok(());
    }
    let options = Options::parse(arguments)?;
    let common = PathBuf::from(git_text(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let report = options
        .report
        .clone()
        .unwrap_or_else(|| common.join("lince-ship.json"));
    let report = if report.is_absolute() {
        report
    } else {
        root.join(report)
    };
    if let Some(parent) = report.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(common.join("lince-ship.lock"))
        .map_err(|error| error.to_string())?;
    lock.try_lock()
        .map_err(|error| format!("Another ship is running: {error}"))?;
    let mut receipt = Receipt {
        activation: "not_attempted".into(),
        ..Receipt::default()
    };
    let result = execute(root, &options, &Tools::default(), &report, &mut receipt);
    if let Err(error) = &result {
        receipt.error = Some(error.clone());
    }
    save(&receipt, &report)?;
    println!(
        "{}",
        serde_json::to_string(&receipt).map_err(|error| error.to_string())?
    );
    eprintln!("ship receipt: {}", report.display());
    result
}

fn execute(
    root: &Path,
    options: &Options,
    tools: &Tools,
    report: &Path,
    receipt: &mut Receipt,
) -> Result<()> {
    stage(receipt, report, "snapshot")?;
    let branch = git_text(root, &["symbolic-ref", "HEAD"])?;
    if branch != "refs/heads/main" {
        return Err(
            "ship requires main: the dev workflow only checks compilation and publishes no release"
                .into(),
        );
    }
    let parent = git_text(root, &["rev-parse", "HEAD"])?;
    let remote = git_text(root, &["remote", "get-url", "--push", "origin"])?;
    let repository = output(Command::new(&tools.gh).current_dir(root).args([
        "repo",
        "view",
        &remote,
        "--json",
        "nameWithOwner",
        "--jq",
        ".nameWithOwner",
    ]))?;
    let repository = repository.trim();
    if repository.is_empty() {
        return Err("gh returned no repository for origin".into());
    }
    let git_directory = PathBuf::from(git_text(root, &["rev-parse", "--absolute-git-dir"])?);
    let directory = git_directory.join(format!(
        "ship-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let mut snapshot = Snapshot {
        root: root.into(),
        directory,
        index_lock: git_directory.join("index.lock"),
        lock_held: false,
        worktree: false,
    };
    let mut index_lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&snapshot.index_lock)
        .map_err(|error| format!("Cannot lock the Git index: {error}"))?;
    snapshot.lock_held = true;
    let index = snapshot.directory.join("index");
    output(
        git(root)
            .env("GIT_INDEX_FILE", &index)
            .args(["read-tree", &parent]),
    )?;
    output(
        git(root)
            .env("GIT_INDEX_FILE", &index)
            .args(["add", "--all", "--", "."]),
    )?;
    let tree = output(git(root).env("GIT_INDEX_FILE", &index).arg("write-tree"))?;
    let tree = tree.trim();
    let old_tree = git_text(root, &["rev-parse", "HEAD^{tree}"])?;
    let revision = if tree == old_tree {
        parent.clone()
    } else {
        git_text(
            root,
            &["commit-tree", tree, "-p", &parent, "-m", &options.message],
        )?
    };
    receipt.revision = Some(revision.clone());
    let source = snapshot.directory.join("source");
    output(
        git(root)
            .args(["worktree", "add", "--detach"])
            .arg(&source)
            .arg(&revision),
    )?;
    snapshot.worktree = true;
    stage(receipt, report, "testing")?;
    if let Some(program) = options.test.first() {
        crate::checked(
            Command::new(program)
                .args(&options.test[1..])
                .current_dir(&source)
                .stdin(Stdio::null()),
        )?;
        receipt.tests = Some("custom".into());
    } else {
        let one = std::hint::black_box(1_u32);
        if one + one != 2 {
            return Err("placeholder test failed: 1 + 1 != 2".into());
        }
        receipt.tests = Some("placeholder: 1 + 1 == 2".into());
    }
    if git_text(
        &source,
        &["status", "--porcelain", "--untracked-files=normal"],
    )? != ""
    {
        return Err("Tests changed the snapshot; refusing to push untested source".into());
    }
    stage(receipt, report, "committing")?;
    if git_text(root, &["symbolic-ref", "HEAD"])? != branch {
        return Err("The active branch changed during testing".into());
    }
    index_lock
        .write_all(&fs::read(&index).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    index_lock.sync_all().map_err(|error| error.to_string())?;
    output(git(root).args([
        "update-ref",
        "-m",
        "ship tested snapshot",
        &branch,
        &revision,
        &parent,
    ]))?;
    fs::rename(&snapshot.index_lock, git_directory.join("index")).map_err(|error| {
        format!("Commit {revision} exists, but index installation failed: {error}")
    })?;
    snapshot.lock_held = false;
    drop(index_lock);
    stage(receipt, report, "pushing")?;
    output(git(root).args([
        "push",
        "--porcelain",
        &remote,
        &format!("{revision}:refs/heads/main"),
    ]))?;
    receipt.pushed = true;
    stage(receipt, report, "waiting_for_workflow")?;
    let run = wait(tools, root, repository, &revision, options)?;
    receipt.run_id = Some(run.database_id);
    receipt.workflow_url = Some(run.url);
    if options.install {
        stage(receipt, report, "installing")?;
        install(tools, root, options, &revision)?;
        receipt.installed = true;
        return stage(receipt, report, "installed_activation_pending");
    }
    stage(receipt, report, "complete")
}

fn successful_run(runs: Vec<Run>, revision: &str) -> Result<Option<Run>> {
    let mut matching: Vec<_> = runs
        .into_iter()
        .filter(|run| run.head_sha == revision && run.head_branch == "main" && run.event == "push")
        .collect();
    matching.sort_by_key(|run| std::cmp::Reverse(run.database_id));
    let Some(run) = matching.into_iter().next() else {
        return Ok(None);
    };
    if run.status != "completed" {
        return Ok(None);
    }
    if run.conclusion.as_deref() != Some("success") {
        return Err(format!(
            "Workflow {} completed with {}: {}",
            run.database_id,
            run.conclusion.as_deref().unwrap_or("unknown"),
            run.url
        ));
    }
    Ok(Some(run))
}

fn wait(
    tools: &Tools,
    root: &Path,
    repository: &str,
    revision: &str,
    options: &Options,
) -> Result<Run> {
    let started = Instant::now();
    loop {
        let json = output(Command::new(&tools.gh).current_dir(root).args([
            "run",
            "ls",
            "--repo",
            repository,
            "--workflow",
            "build.yml",
            "--commit",
            revision,
            "--branch",
            "main",
            "--event",
            "push",
            "--limit",
            "100",
            "--json",
            "databaseId,headSha,headBranch,event,status,conclusion,url",
        ]))?;
        let runs = serde_json::from_str(&json)
            .map_err(|error| format!("Invalid workflow response: {error}"))?;
        if let Some(run) = successful_run(runs, revision)? {
            return Ok(run);
        }
        let remaining = options.timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(format!("Timed out waiting for build.yml at {revision}"));
        }
        std::thread::sleep(options.interval.min(remaining));
    }
}

fn verify_manifest(path: &Path, revision: &str) -> Result<()> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if manifest["revision"].as_str() != Some(revision) {
        return Err(format!(
            "The Nix release revision {:?} does not match tested commit {revision}; refusing to switch",
            manifest["revision"]
        ));
    }
    Ok(())
}

fn install(tools: &Tools, root: &Path, options: &Options, revision: &str) -> Result<()> {
    let (directory, host) = options
        .flake
        .rsplit_once('#')
        .filter(|(directory, host)| !directory.is_empty() && !host.is_empty())
        .ok_or("--nixos-flake must be a local PATH#HOST")?;
    let directory = fs::canonicalize(root.join(directory)).map_err(|error| error.to_string())?;
    let flake = format!("{}#{host}", directory.display());
    output(
        Command::new(&tools.nix)
            .args(["flake", "update", "--refresh", "lince", "--flake"])
            .arg(&directory),
    )?;
    let expression = format!(
        "(builtins.getFlake {}).inputs.lince.outPath",
        serde_json::to_string(&directory.to_string_lossy())
            .map_err(|error| error.to_string())?
            .replace("${", "\\${")
    );
    let resolved = output(Command::new(&tools.nix).args([
        "eval",
        "--impure",
        "--raw",
        "--expr",
        &expression,
    ]))?;
    verify_manifest(
        &PathBuf::from(resolved.trim()).join("binary.json"),
        revision,
    )?;
    crate::checked(Command::new(&tools.sudo).args([
        "-n",
        "nixos-rebuild",
        "switch",
        "--flake",
        &flake,
        "--accept-flake-config",
        "--no-update-lock-file",
    ]))
}

#[cfg(test)]
#[path = "ship/tests.rs"]
mod tests;
