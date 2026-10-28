use super::*;
use serde_json::{Value, json};

mod scenarios;
mod manual;

const NAMES: [&str; 10] = [
    "Installed package and login registration", "Ordinary application windows",
    "Tiling, focus and workspaces", "Settings, rules and saved state",
    "Canvas and native rendering", "Typing, clipboard, IME and drops",
    "External tools and sharing", "Release pacing and resource stability",
    "Physical outputs, input and protection", "Login, daily use and logout",
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    executable: Option<PathBuf>,
    fixtures: Configuration,
    geckodriver: PathBuf,
    firefox: PathBuf,
    provider: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    id: u8,
    name: String,
    status: String,
    automatic: bool,
    detail: String,
    evidence: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    directory: PathBuf,
    settings: Settings,
    source: String,
    candidate: Option<PathBuf>,
    candidate_hash: Option<String>,
    payload_hash: Option<String>,
    prepared: Option<PathBuf>,
    park_tests: Option<PathBuf>,
    test_hash: Option<String>,
    scenarios: Vec<Scenario>,
}

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition { Ok(()) } else { Err(message.into()) }
}

fn private(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(error)?;
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    require(metadata.is_dir() && metadata.uid() == rustix::process::geteuid().as_raw(), "Suite directory must be an owned real directory")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(error)
}

fn source_identity() -> Result<String> {
    let output = Command::new("git").args(["ls-files", "-co", "--exclude-standard", "-z"]).output().map_err(error)?;
    require(output.status.success(), "Cannot identify suite source inputs")?;
    let mut paths = std::collections::BTreeSet::new();
    for bytes in output.stdout.split(|byte| *byte == 0).filter(|bytes| !bytes.is_empty()) {
        let path = std::str::from_utf8(bytes).map_err(error)?;
        if !path.ends_with(".md") && !path.starts_with("target/") { paths.insert(path); }
    }
    let mut hash = Sha256::new();
    for path in paths {
        let metadata = match fs::symlink_metadata(path) { Ok(metadata) => metadata, Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue, Err(failure) => return Err(error(failure)) };
        hash.update(path.as_bytes()); hash.update([0]);
        if metadata.file_type().is_symlink() { hash.update(fs::read_link(path).map_err(error)?.as_os_str().as_encoded_bytes()); }
        else if metadata.is_file() { hash.update(digest(Path::new(path))?.as_bytes()); }
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn save(suite: &Suite) -> Result<()> {
    require(suite.scenarios.len() == 10 && suite.scenarios.iter().enumerate().all(|(index, scenario)| scenario.id as usize == index + 1 && scenario.name == NAMES[index]), "The final suite must retain exactly its ten fixed scenarios")?;
    write_json(&suite.directory.join("report.json"), suite)
}

fn open(path: &Path) -> Result<Suite> {
    require(path.is_absolute(), "Use an absolute final-suite run directory")?;
    let directory = fs::canonicalize(path).map_err(error)?;
    let base = fs::canonicalize(env::current_dir().map_err(error)?.join("target/park-validation/final-suite")).map_err(error)?;
    require(directory.starts_with(base), "Final-suite state must stay under target/park-validation/final-suite")?;
    let metadata = fs::symlink_metadata(&directory).map_err(error)?;
    require(metadata.is_dir() && metadata.uid() == rustix::process::geteuid().as_raw() && metadata.mode() & 0o077 == 0, "Final-suite state must be private and owned")?;
    let suite: Suite = load(&directory.join("report.json"))?;
    require(suite.directory == directory, "Final-suite manifest belongs to another directory")?;
    require(suite.scenarios.len() == 10 && suite.scenarios.iter().enumerate().all(|(index, scenario)| scenario.id as usize == index + 1 && scenario.name == NAMES[index]), "Invalid final-suite scenario manifest")?;
    Ok(suite)
}

fn verify(suite: &Suite) -> Result<()> {
    require(source_identity()? == suite.source, "Source changed after suite preparation; initialize a fresh run for the new candidate")?;
    if let Some(candidate) = &suite.candidate {
        require(Some(digest(candidate)?) == suite.candidate_hash, "Selected installed executable changed")?;
        require(Some(digest(&process_executable(candidate)?)?) == suite.payload_hash, "Selected compositor payload changed")?;
    }
    if let Some(binary) = &suite.park_tests { require(Some(digest(binary)?) == suite.test_hash, "Frozen fixture executable changed")?; }
    Ok(())
}

fn artifact(suite: &Suite, id: u8) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(error)?.as_nanos();
    let path = suite.directory.join(format!("{id:02}/attempt-{stamp}"));
    private(&path)?;
    Ok(path)
}

fn command(command: &mut Command, log: &Path, seconds: u64) -> Result<()> {
    deny_warnings(command);
    command.env("RUST_MIN_STACK", "33554432");
    logged(command, log)?;
    let mut children = Children(vec![ManagedChild::spawn(command)?]);
    let start = Instant::now();
    let result = loop {
        if let Err(failure) = check_interruption() { break Err(failure); }
        match children.0[0].child.try_wait().map_err(error)? {
            Some(status) => break require(status.success(), format!("Command failed: {status}; {}", log.display())),
            None if start.elapsed() >= Duration::from_secs(seconds) => break Err(format!("Command exceeded {seconds}s; {}", log.display())),
            None => thread::sleep(Duration::from_millis(25)),
        }
    };
    drop(children);
    result
}

fn init(path: &Path) -> Result<()> {
    let settings: Settings = load(path)?;
    require(settings.fixtures.valid_duration(), "Invalid fixture observation duration")?;
    automation::configuration(&settings.fixtures)?;
    if let Some(helper) = &settings.fixtures.helper { require(helper.valid(), "Invalid helper capability selection")?; }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(error)?.as_nanos();
    let base = env::current_dir().map_err(error)?.join("target/park-validation/final-suite");
    private(&base)?;
    let directory = base.join(format!("run-{stamp}-{}", std::process::id()));
    private(&directory)?;
    let source = source_identity()?;
    let scenarios = NAMES.iter().enumerate().map(|(index, name)| Scenario { id: index as u8 + 1, name: (*name).into(), status: "not_run".into(), automatic: false, detail: "Not executed".into(), evidence: Vec::new() }).collect();
    let suite = Suite { directory, settings, source, candidate: None, candidate_hash: None, payload_hash: None, prepared: None, park_tests: None, test_hash: None, scenarios };
    save(&suite)?;
    println!("{}", suite.directory.display());
    Ok(())
}

fn template(path: &Path) -> Result<()> {
    require(path.is_absolute() && !path.exists(), "Choose a new absolute fixture JSON path")?;
    let settings = Settings {
        executable: None,
        fixtures: Configuration { terminal: Program { program: "xdg-terminal-exec".into(), arguments: Vec::new() }, launcher: None, lock: None, toolkit_qml: None, showoff: false, clients: Vec::new(), helper: None, automation: false, lifecycle_rounds: 1, observation_seconds: 300, soak_seconds: 0 },
        geckodriver: "geckodriver".into(), firefox: "firefox".into(), provider: None,
    };
    write_json(path, &settings)
}

fn run(suite: &mut Suite, only: Option<u8>) -> Result<()> {
    let _signals = interrupt_handler()?;
    verify(suite)?;
    for id in 1..=10 {
        if only.is_some_and(|selected| selected != id) || suite.scenarios[(id - 1) as usize].status == "pass" { continue; }
        if id != 1 && suite.prepared.is_none() { return Err("Run scenario 01 before the remaining scenarios".into()); }
        let directory = artifact(suite, id)?;
        suite.scenarios[(id - 1) as usize].status = "running".into();
        suite.scenarios[(id - 1) as usize].evidence.push(directory.clone());
        save(suite)?;
        let result = match id {
            1 => scenarios::package(suite, &directory), 2 => scenarios::windows(suite, &directory),
            3 => scenarios::arrangement(suite, &directory), 4 => scenarios::settings(suite, &directory),
            5 => scenarios::canvas(suite, &directory), 6 => scenarios::input(suite, &directory),
            7 => scenarios::providers(suite, &directory), 8 => scenarios::performance(suite, &directory),
            9 | 10 => manual::guide(suite, id, &directory), _ => unreachable!(),
        };
        let result = result.and_then(|detail| { verify(suite)?; Ok(detail) });
        let scenario = &mut suite.scenarios[(id - 1) as usize];
        match result {
            Ok(waiting) => {
                scenario.automatic = id <= 8;
                scenario.status = if waiting.is_some() { "waiting" } else { "pass" }.into();
                scenario.detail = waiting.unwrap_or_else(|| "Required scenario observations passed".into());
            }
            Err(failure) => { scenario.status = "fail".into(); scenario.automatic = false; scenario.detail = failure; }
        }
        save(suite)?;
        println!("{id:02}: {} — {}", scenario_status(suite, id), suite.scenarios[(id - 1) as usize].detail);
        if id == 1 && suite.prepared.is_none() { return Err("Scenario 01 failed; inspect its evidence before continuing".into()); }
        check_interruption()?;
    }
    report(suite)
}

fn scenario_status(suite: &Suite, id: u8) -> &str { &suite.scenarios[(id - 1) as usize].status }

fn report(suite: &Suite) -> Result<()> {
    verify(suite)?;
    for scenario in &suite.scenarios { println!("{:02} {}: {}", scenario.id, scenario.name, scenario.status); }
    println!("Evidence: {}", suite.directory.display());
    if suite.scenarios.iter().all(|scenario| scenario.status == "pass") { println!("The fixed ten-scenario Park acceptance suite passed."); }
    else { println!("Park acceptance remains incomplete; waiting and failed scenarios are not passes."); }
    Ok(())
}

fn id(value: &OsString) -> Result<u8> {
    value.to_str().and_then(|value| value.parse().ok()).filter(|value| (1..=10).contains(value)).ok_or_else(|| "Choose one of the ten fixed scenario IDs: 01 through 10".into())
}

pub(super) fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|value| value.to_str()) {
        None | Some("help" | "--help") => {
            println!("cargo xtask park validate final template /absolute/config.json");
            println!("cargo xtask park validate final init /absolute/config.json");
            println!("cargo xtask park validate final run /absolute/run [--only 01..10]");
            println!("cargo xtask park validate final guide /absolute/run 07|09|10");
            println!("cargo xtask park validate final seat /absolute/run");
            println!("cargo xtask park validate final observe /absolute/run PID [--seconds 5400] [--unit USER_UNIT]");
            println!("cargo xtask park validate final attest /absolute/run 07|09|10 /absolute/observations.json");
            println!("cargo xtask park validate final report /absolute/run");
            Ok(())
        }
        Some("template") if arguments.len() == 2 => template(Path::new(&arguments[1])),
        Some("init") if arguments.len() == 2 => init(Path::new(&arguments[1])),
        Some("run") if arguments.len() == 2 || arguments.len() == 4 && arguments[2] == "--only" => {
            let mut suite = open(Path::new(&arguments[1]))?;
            run(&mut suite, if arguments.len() == 4 { Some(id(&arguments[3])?) } else { None })
        }
        Some("report") if arguments.len() == 2 => report(&open(Path::new(&arguments[1]))?),
        Some("guide") if arguments.len() == 3 => {
            let suite = open(Path::new(&arguments[1]))?; verify(&suite)?;
            let selected = id(&arguments[2])?; let directory = artifact(&suite, selected)?;
            manual::guide(&suite, selected, &directory).map(|_| ())
        }
        Some("seat") if arguments.len() == 2 => {
            let mut suite = open(Path::new(&arguments[1]))?; verify(&suite)?;
            manual::seat(&mut suite)
        }
        Some("observe") if arguments.len() >= 3 => {
            let mut suite = open(Path::new(&arguments[1]))?; verify(&suite)?;
            manual::observe(&mut suite, &arguments[2..])
        }
        Some("attest") if arguments.len() == 4 => {
            let mut suite = open(Path::new(&arguments[1]))?; verify(&suite)?;
            manual::attest(&mut suite, id(&arguments[2])?, Path::new(&arguments[3]))
        }
        _ => Err("Use cargo xtask park validate final help".into()),
    }
}
