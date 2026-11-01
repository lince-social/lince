use crate::{Result, ui_process};
use lince_interface::testing::{Backend, Dispatch, Limits, Route, Runner, Selector, Step};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const PACKAGE: &str = "social.lince.mobile.debug";
const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Target {
    Name(String),
    Named { name: String, role: Option<String> },
}

impl Target {
    fn selector(&self) -> Selector {
        match self {
            Self::Name(name) => Selector::new(name),
            Self::Named { name, role } => Selector {
                name: name.clone(),
                role: role.clone(),
            },
        }
    }
}

#[derive(Deserialize)]
struct ValueExpectation {
    target: Target,
    equals: String,
}

#[derive(Deserialize)]
struct Expect {
    visible: Option<Target>,
    absent: Option<Target>,
    value: Option<ValueExpectation>,
}

#[derive(Deserialize)]
struct PlanStep {
    name: String,
    target: Target,
    action: String,
    value: Option<Value>,
    key: Option<String>,
    direction: Option<String>,
    expect: Expect,
    fallback_name: Option<String>,
    #[serde(default, alias = "fail_if")]
    reject: Vec<Target>,
    #[serde(default)]
    independent: bool,
}

#[derive(Deserialize)]
struct Plan {
    device: String,
    adb: Option<PathBuf>,
    #[serde(default = "package")]
    package: String,
    #[serde(default)]
    allowed_packages: Vec<String>,
    #[serde(alias = "fixture_path")]
    fixture: Option<PathBuf>,
    #[serde(alias = "report_path")]
    report: PathBuf,
    #[serde(default = "timeout")]
    timeout_ms: u64,
    #[serde(default = "polls")]
    max_polls: usize,
    #[serde(default)]
    continue_independent: bool,
    #[serde(default)]
    keep_open_on_failure: bool,
    steps: Vec<PlanStep>,
}

fn package() -> String {
    PACKAGE.into()
}
fn timeout() -> u64 {
    6000
}
fn polls() -> usize {
    8
}

#[derive(Clone)]
struct Node {
    name: String,
    role: String,
    package: String,
    bounds: Option<[u32; 4]>,
    enabled: bool,
    focused: bool,
    editable: bool,
    secret: bool,
    value: Option<String>,
}

fn sensitive(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    ["password", "api key", "api_key", "secret"]
        .iter()
        .any(|word| label.contains(word))
}

fn bounds(value: &str) -> Option<[u32; 4]> {
    let values = value
        .split(['[', ']', ','])
        .filter(|part| !part.is_empty())
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    let values: [u32; 4] = values.try_into().ok()?;
    (values[0] < values[2] && values[1] < values[3] && values.iter().all(|value| *value <= 32767))
        .then_some(values)
}

fn parse(xml: &str) -> Result<Vec<Node>> {
    let start = xml
        .find("<hierarchy")
        .ok_or("UI hierarchy is unavailable")?;
    let end = xml
        .rfind("</hierarchy>")
        .ok_or("UI hierarchy is incomplete")?
        + "</hierarchy>".len();
    let mut reader = Reader::from_str(&xml[start..end]);
    let mut nodes = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|_| "Invalid UI hierarchy XML")?
        {
            Event::Start(event) | Event::Empty(event) if event.name().as_ref() == b"node" => {
                let mut attrs = BTreeMap::new();
                for attr in event.attributes() {
                    let attr = attr.map_err(|_| "Invalid UI hierarchy attribute")?;
                    let key = std::str::from_utf8(attr.key.as_ref())
                        .map_err(|_| "Invalid UI attribute name")?
                        .to_string();
                    let value = attr
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(|_| "Invalid UI attribute value")?
                        .into_owned();
                    attrs.insert(key, value);
                }
                let get = |key: &str| attrs.get(key).map(String::as_str).unwrap_or("");
                if get("visible-to-user") == "false" {
                    continue;
                }
                let class = get("class").rsplit('.').next().unwrap_or("");
                let editable = class == "EditText";
                let raw = if get("content-desc").is_empty() {
                    get("text")
                } else {
                    get("content-desc")
                };
                let prefix = raw.split_once(':').map(|(label, _)| label.trim());
                let secret = get("password") == "true"
                    || sensitive(prefix.unwrap_or(raw))
                    || sensitive(get("resource-id"));
                let name = if secret {
                    prefix
                        .filter(|label| sensitive(label))
                        .unwrap_or("Protected field")
                        .to_string()
                } else if editable {
                    prefix
                        .unwrap_or_else(|| {
                            if get("content-desc").is_empty() {
                                "TextInput"
                            } else {
                                get("content-desc")
                            }
                        })
                        .to_string()
                } else {
                    raw.to_string()
                };
                nodes.push(Node {
                    name: if name.is_empty() && !editable {
                        class.to_string()
                    } else {
                        name
                    },
                    role: if editable {
                        "TextInput".into()
                    } else {
                        class.into()
                    },
                    package: get("package").into(),
                    bounds: bounds(get("bounds")),
                    enabled: get("enabled") != "false",
                    focused: get("focused") == "true",
                    editable,
                    secret,
                    value: (!secret && editable).then(|| raw.to_string()),
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let editing = nodes
        .iter()
        .any(|node| node.name.eq_ignore_ascii_case("Keep edit") && node.role == "Button");
    for node in &mut nodes {
        if editing && node.editable && node.focused {
            node.name = "Editor".into();
        } else if let Some(value) = &mut node.value {
            if let Some((_, content)) = value.split_once(':') {
                *value = content.strip_prefix(' ').unwrap_or(content).to_string();
            }
        }
    }
    Ok(nodes)
}

fn matches(node: &Node, selector: &Selector) -> bool {
    node.name == selector.name
        && selector
            .role
            .as_ref()
            .is_none_or(|role| role == &node.role || role == "EditText" && node.role == "TextInput")
}

fn selected<'a>(nodes: &'a [Node], selector: &Selector) -> Result<&'a Node> {
    let mut found = nodes
        .iter()
        .filter(|node| node.enabled && node.bounds.is_some() && matches(node, selector));
    let node = found
        .next()
        .ok_or("No visible enabled control matches the exact target")?;
    if found.next().is_some() {
        return Err("The exact target is ambiguous in the foreground package".into());
    }
    Ok(node)
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn input_text(value: &str) -> Result<String> {
    if !value
        .bytes()
        .all(|byte| (b' '..=b'~').contains(&byte) && byte != b'%')
    {
        return Err(
            "Android text input requires printable ASCII without literal percent signs".into(),
        );
    }
    Ok(quote(&value.replace(' ', "%s")))
}

fn foreground(output: &str) -> Result<String> {
    output
        .lines()
        .filter(|line| line.contains("mResumedActivity") || line.contains("topResumedActivity"))
        .flat_map(str::split_whitespace)
        .find_map(|part| {
            part.split_once('/')
                .map(|(package, _)| package.trim_start_matches('{').to_string())
        })
        .ok_or_else(|| "The foreground activity owner is unavailable".into())
}

fn hierarchy_command() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!(
        "lince_ui_dump=/data/local/tmp/lince-ui-{}-{nonce}.xml; trap 'rm -f \"$lince_ui_dump\"' EXIT HUP INT TERM; uiautomator dump \"$lince_ui_dump\" >/dev/null && cat \"$lince_ui_dump\"",
        std::process::id()
    )
}

struct Action {
    kind: String,
    value: Option<String>,
    key: Option<String>,
    direction: Option<String>,
    fallback: Option<String>,
}

#[derive(Clone)]
struct Snapshot {
    owner: String,
    nodes: Vec<Node>,
}

fn summary(snapshot: &Snapshot) -> String {
    let mut roles = BTreeMap::new();
    for node in &snapshot.nodes {
        *roles.entry(node.role.as_str()).or_insert(0usize) += 1;
    }
    format!(
        "foreground={}; controls={}; enabled={}; roles={roles:?}",
        snapshot.owner,
        snapshot.nodes.len(),
        snapshot.nodes.iter().filter(|node| node.enabled).count()
    )
}

struct Android {
    adb: PathBuf,
    device: String,
    allowed: Vec<String>,
    cached: Option<(Instant, Snapshot)>,
    diagnostics: String,
    terminal: bool,
    reject: Vec<Selector>,
}

impl Android {
    fn check_status(&self, snapshot: &Snapshot) -> Result<()> {
        if self
            .reject
            .iter()
            .any(|selector| snapshot.nodes.iter().any(|node| matches(node, selector)))
        {
            return Err("An explicitly rejected UI status is visible".into());
        }
        Ok(())
    }

    fn command(&self, command: &str, remaining: Duration) -> Result<std::process::Output> {
        ui_process::output(
            Command::new(&self.adb).args(["-s", &self.device, "shell", command]),
            remaining,
            OUTPUT_LIMIT,
        )
    }

    fn owner(&self, remaining: Duration) -> Result<String> {
        let output = self.command("dumpsys activity activities", remaining)?;
        if !output.status.success() {
            return Err("Cannot inspect the foreground activity".into());
        }
        let owner = foreground(&String::from_utf8_lossy(&output.stdout))?;
        if !self.allowed.contains(&owner) {
            return Err("The foreground owner is outside the explicitly allowed packages".into());
        }
        Ok(owner)
    }
}

impl Backend for Android {
    type Snapshot = Snapshot;
    type Action = Action;
    type Condition = Expect;

    fn snapshot(&mut self, remaining: Duration) -> Result<Snapshot> {
        if self.terminal {
            return Err("Input acknowledgement is uncertain; this plan must stop".into());
        }
        if let Some((when, snapshot)) = &self.cached
            && when.elapsed() < Duration::from_millis(100)
        {
            self.check_status(snapshot)?;
            return Ok(snapshot.clone());
        }
        let start = Instant::now();
        let owner = self.owner(remaining)?;
        let output = self.command(
            &hierarchy_command(),
            remaining.saturating_sub(start.elapsed()),
        )?;
        if !output.status.success() {
            return Err("Cannot capture the UI hierarchy".into());
        }
        let mut nodes = parse(&String::from_utf8_lossy(&output.stdout))?;
        nodes.retain(|node| node.package == owner);
        let snapshot = Snapshot { owner, nodes };
        self.diagnostics = summary(&snapshot);
        self.check_status(&snapshot)?;
        self.cached = Some((Instant::now(), snapshot.clone()));
        Ok(snapshot)
    }

    fn dispatch(
        &mut self,
        snapshot: &Snapshot,
        selector: &Selector,
        action: &Action,
        route: Route,
        remaining: Duration,
    ) -> Dispatch {
        if self.terminal {
            return Dispatch::Rejected("Input acknowledgement is uncertain".into());
        }
        let start = Instant::now();
        match self.owner(remaining) {
            Ok(owner) if owner == snapshot.owner => {}
            _ => return Dispatch::Rejected("The foreground owner changed before input".into()),
        }
        let selector = if route == Route::Fallback {
            match &action.fallback {
                Some(name) if name != "focused-enter" => Selector {
                    name: name.clone(),
                    role: selector.role.clone(),
                },
                Some(_) => selector.clone(),
                None => return Dispatch::Unavailable("No fallback was declared".into()),
            }
        } else {
            selector.clone()
        };
        let focused_enter =
            route == Route::Fallback && action.fallback.as_deref() == Some("focused-enter");
        let target = match selected(&snapshot.nodes, &selector) {
            Ok(target) => target,
            Err(error) => return Dispatch::Rejected(error),
        };
        let [left, top, right, bottom] = target.bounds.unwrap();
        let center = ((left + right) / 2, (top + bottom) / 2);
        let command = if focused_enter {
            if !target.focused {
                return Dispatch::Unavailable("The fallback target is not focused".into());
            }
            "input keyevent 66".into()
        } else {
            match action.kind.as_str() {
                "tap" => format!(
                    "input swipe {} {} {} {} 100",
                    center.0, center.1, center.0, center.1
                ),
                "text" => {
                    if target.name != "Editor" || !target.editable || !target.focused {
                        return Dispatch::Rejected(
                            "Text requires the focused native Editor dialog".into(),
                        );
                    }
                    let Some(value) = &action.value else {
                        return Dispatch::Rejected("Text input value is missing".into());
                    };
                    let value = match input_text(value) {
                        Ok(value) => value,
                        Err(error) => return Dispatch::Rejected(error),
                    };
                    if action.value.as_deref() == Some("") {
                        "input keycombination 113 29 && input keyevent 67".into()
                    } else {
                        format!("input keycombination 113 29 && input text {value}")
                    }
                }
                "key" => match action.key.as_deref().unwrap_or("ENTER") {
                    "BACK" => "input keyevent 4".into(),
                    "ENTER" => "input keyevent 66".into(),
                    "TAB" => "input keyevent 61".into(),
                    "ESCAPE" => "input keyevent 111".into(),
                    "DELETE" => "input keyevent 67".into(),
                    _ => return Dispatch::Rejected("Unsupported scoped key".into()),
                },
                "scroll" => {
                    let (from, to) = match action.direction.as_deref().unwrap_or("down") {
                        "down" => (top + (bottom - top) * 3 / 4, top + (bottom - top) / 4),
                        "up" => (top + (bottom - top) / 4, top + (bottom - top) * 3 / 4),
                        _ => {
                            return Dispatch::Rejected(
                                "Scroll direction must be up or down".into(),
                            );
                        }
                    };
                    format!("input swipe {} {from} {} {to} 100", center.0, center.0)
                }
                _ => return Dispatch::Rejected("Unsupported Android input action".into()),
            }
        };
        self.cached = None;
        match self.command(&command, remaining.saturating_sub(start.elapsed())) {
            Ok(output) if output.status.success() => Dispatch::Accepted,
            _ => {
                self.terminal = true;
                Dispatch::Accepted
            }
        }
    }

    fn condition(&self, snapshot: &Snapshot, expect: &Expect) -> Result<bool> {
        if self.terminal {
            return Err("Input acknowledgement is uncertain; no input will be replayed".into());
        }
        self.check_status(snapshot)?;
        if let Some(target) = &expect.visible {
            return Ok(selected(&snapshot.nodes, &target.selector()).is_ok());
        }
        if let Some(target) = &expect.absent {
            return Ok(!snapshot
                .nodes
                .iter()
                .any(|node| matches(node, &target.selector())));
        }
        if let Some(value) = &expect.value {
            let target = selected(&snapshot.nodes, &value.target.selector())?;
            if target.secret {
                return Err("Protected field values cannot be observed".into());
            }
            return Ok(target.value.as_ref() == Some(&value.equals));
        }
        Err("An explicit UI postcondition is required".into())
    }

    fn pump(&mut self, _: Duration) -> Result<()> {
        self.cached = None;
        Ok(())
    }
    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }
}

#[derive(Serialize)]
struct ReportSelector {
    name: String,
    role: Option<String>,
}

#[derive(Serialize)]
struct StepReport {
    name: String,
    selector: ReportSelector,
    outcome: String,
    polls: usize,
    snapshots: usize,
    dispatches: usize,
    pumps: usize,
    elapsed_ms: u128,
    diagnostics: String,
}

fn resolve(value: &Option<Value>, fixture: &Value) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(value) => value
            .get("fixture")
            .and_then(Value::as_str)
            .and_then(|pointer| fixture.pointer(pointer))
            .and_then(Value::as_str)
            .map(|value| Some(value.into()))
            .ok_or_else(|| "The private fixture value is unavailable".into()),
    }
}

pub(super) fn run(root: &Path, args: &[OsString]) -> Result<()> {
    if args.is_empty()
        || args.len() == 1 && matches!(args[0].to_str(), Some("help" | "--help" | "-h"))
    {
        println!("cargo xtask ui-test android PLAN.json");
        println!(
            r#"{{"device":"SERIAL","report":"results.json","timeout_ms":6000,"max_polls":8,"steps":[{{"name":"Open navigation","target":{{"name":"Pages","role":"Button"}},"action":"tap","expect":{{"visible":"Organ"}}}}]}}"#
        );
        println!(
            "Actions: tap, text, key, scroll, observe. Each step requires one visible, absent or value postcondition. Explicit fallback_name runs only before accepted input. Failed plans stop the disposable debug app unless keep_open_on_failure is true. Private text values may use {{\"fixture\":\"/json/pointer\"}} with a plan fixture path."
        );
        return Ok(());
    }
    if args.len() != 2 || args[0] != "android" {
        return Err("cargo xtask ui-test android PLAN.json".into());
    }
    let plan_path = root.join(&args[1]);
    let base = plan_path.parent().unwrap_or(root);
    let mut plan: Plan =
        serde_json::from_slice(&fs::read(&plan_path).map_err(|_| "Cannot read UI test plan")?)
            .map_err(|_| "Invalid UI test plan")?;
    if plan.steps.is_empty()
        || plan.timeout_ms == 0
        || plan.max_polls == 0
        || plan.steps.iter().any(|step| {
            usize::from(step.expect.visible.is_some())
                + usize::from(step.expect.absent.is_some())
                + usize::from(step.expect.value.is_some())
                != 1
                || !matches!(
                    step.action.as_str(),
                    "tap" | "text" | "key" | "scroll" | "observe"
                )
        })
    {
        return Err("A UI plan needs bounded steps, supported actions and exactly one postcondition per step".into());
    }
    if plan.package != PACKAGE
        || !plan
            .device
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        || plan.device.is_empty()
    {
        return Err(
            "UI tests require the disposable debug package and a valid device serial".into(),
        );
    }
    if plan.allowed_packages.iter().any(|package| {
        matches!(
            package.as_str(),
            "social.lince.mobile" | "social.lince.mobile.smoketest"
        ) || !package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._".contains(&byte))
    }) {
        return Err("The allowed packages include an invalid or protected app".into());
    }
    let fixture: Value = if let Some(path) = plan.fixture {
        serde_json::from_slice(
            &fs::read(base.join(path)).map_err(|_| "Cannot read private fixture")?,
        )
        .map_err(|_| "Invalid private fixture")?
    } else {
        Value::Null
    };
    plan.report = base.join(&plan.report);
    let adb = plan
        .adb
        .map(|path| base.join(path))
        .or_else(|| std::env::var_os("LINCE_ADB").map(PathBuf::from))
        .or_else(|| {
            std::env::var_os("ANDROID_HOME")
                .map(|sdk| PathBuf::from(sdk).join("platform-tools/adb"))
                .filter(|adb| adb.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("adb"));
    let mut allowed = plan.allowed_packages;
    allowed.push(plan.package);
    let mut backend = Android {
        adb,
        device: plan.device,
        allowed,
        cached: None,
        diagnostics: "No UI snapshot acquired".into(),
        terminal: false,
        reject: Vec::new(),
    };
    let mut reports = Vec::new();
    let mut failed = false;
    for item in plan.steps {
        if failed && !(plan.continue_independent && item.independent) {
            break;
        }
        backend.reject = item.reject.iter().map(Target::selector).collect();
        let action = Action {
            kind: item.action.clone(),
            value: resolve(&item.value, &fixture)?,
            key: item.key,
            direction: item.direction,
            fallback: item.fallback_name,
        };
        let mut step = if item.action == "observe" {
            Step::observe(&item.name, item.target.selector(), item.expect)
        } else {
            Step::action(&item.name, item.target.selector(), action, item.expect)
        };
        step.fallback = item.action != "observe"
            && step
                .action
                .as_ref()
                .is_some_and(|action| action.fallback.is_some());
        let report = Runner::new(Limits {
            timeout: Duration::from_millis(plan.timeout_ms),
            max_polls: plan.max_polls,
        })
        .run(&mut backend, &step);
        let passed = report.is_success();
        reports.push(StepReport {
            name: report.step,
            selector: ReportSelector {
                name: report.selector.name,
                role: report.selector.role,
            },
            outcome: format!("{:?}", report.outcome),
            polls: report.metrics.polls,
            snapshots: report.metrics.snapshots,
            dispatches: report.metrics.dispatches,
            pumps: report.metrics.pumps,
            elapsed_ms: report.metrics.elapsed.as_millis(),
            diagnostics: report.diagnostics,
        });
        if let Some(parent) = plan
            .report
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|_| "Cannot create report directory")?;
        }
        fs::write(
            &plan.report,
            serde_json::to_vec_pretty(&reports).map_err(|_| "Cannot encode sanitized report")?,
        )
        .map_err(|_| "Cannot write sanitized report")?;
        println!("{} {}", if passed { "PASS" } else { "FAIL" }, item.name);
        failed |= !passed;
        if backend.terminal {
            break;
        }
    }
    if failed {
        if !plan.keep_open_on_failure {
            let stopped = backend
                .command(&format!("am force-stop {PACKAGE}"), Duration::from_secs(2))
                .is_ok_and(|output| output.status.success());
            if let Some(last) = reports.last_mut() {
                last.diagnostics.push_str(if stopped {
                    "; failed plan cleanup: debug app stopped"
                } else {
                    "; failed plan cleanup: debug app stop unconfirmed"
                });
            }
            if let Ok(encoded) = serde_json::to_vec_pretty(&reports) {
                let _ = fs::write(&plan.report, encoded);
            }
        }
        Err("UI workflow failed; inspect the sanitized step report".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(body: &str) -> String {
        format!("<?xml version=\"1.0\"?><hierarchy>{body}</hierarchy>\nUI hierarchy dumped")
    }

    #[test]
    fn xml_decodes_exact_roles_and_discards_password_values() {
        let nodes = parse(&xml(r#"<node package="social.lince.mobile.debug" class="android.widget.EditText" text="Password: secret-canary" password="true" enabled="true" bounds="[0,0][100,40]"/><node package="social.lince.mobile.debug" class="android.widget.EditText" text="Username: Ana &amp; B" enabled="true" bounds="[0,40][100,80]"/>"#)).unwrap();
        assert_eq!(nodes[0].name, "Password");
        assert!(nodes[0].value.is_none());
        assert_eq!(nodes[1].name, "Username");
        assert_eq!(nodes[1].value.as_deref(), Some("Ana & B"));
        assert!(selected(&nodes, &Selector::new("Username").with_role("TextInput")).is_ok());
    }

    #[test]
    fn selectors_reject_ambiguity_hidden_controls_and_malformed_bounds() {
        let node = r#"<node class="android.widget.Button" text="Confirm" enabled="true" bounds="[0,0][100,40]"/>"#;
        assert!(
            selected(
                &parse(&xml(&format!("{node}{node}"))).unwrap(),
                &Selector::new("Confirm")
            )
            .is_err()
        );
        assert!(
            parse(&xml(
                &node.replace("enabled=\"true\"", "visible-to-user=\"false\"")
            ))
            .unwrap()
            .is_empty()
        );
        assert!(bounds("[0,0][0,40]").is_none());
        assert!(bounds("[0,0][100,40]unexpected").is_none());
        let clipped = parse(&xml(&node
            .replace("[0,0][100,40]", "[0,0][0,0]")
            .replace("enabled=\"true\"", "enabled=\"true\" focused=\"true\"")))
        .unwrap();
        assert!(selected(&clipped, &Selector::new("Confirm")).is_err());
    }

    #[test]
    fn shell_quotes_metacharacters_and_rejects_percent_space_aliases() {
        assert_eq!(input_text("a b;$(id)'x").unwrap(), "'a%sb;$(id)'\\''x'");
        assert!(input_text("literal%s").is_err());
        assert!(input_text("line\nbreak").is_err());
        assert!(input_text("界").is_err());
    }

    #[test]
    fn editor_is_named_only_in_the_native_keep_edit_dialog() {
        let nodes = parse(&xml(r#"<node class="android.widget.EditText" text="hidden-value" focused="true" password="true" bounds="[0,0][100,40]"/><node class="android.widget.Button" text="KEEP EDIT" bounds="[0,40][100,80]"/>"#)).unwrap();
        assert_eq!(nodes[0].name, "Editor");
        assert!(nodes[0].value.is_none());
        assert!(nodes[0].secret);
        assert!(selected(&nodes, &Selector::new("Editor").with_role("TextInput")).is_ok());
        assert_eq!(foreground("mResumedActivity: ActivityRecord{abc u0 social.lince.mobile.debug/.MainActivity t1}").unwrap(), PACKAGE);
    }

    #[test]
    fn portable_diagnostics_exclude_coordinate_address_and_control_text_values() {
        let snapshot = Snapshot { owner: PACKAGE.into(), nodes: parse(&xml(r#"<node class="android.widget.TextView" text="Point 12.345678,-45.678901" enabled="true" bounds="[0,0][100,40]"/><node class="android.widget.Button" text="Canary Street 99" enabled="true" bounds="[0,40][100,80]"/><node class="android.widget.EditText" text="Title: private-input-canary" bounds="[0,80][100,120]"/>"#)).unwrap() };
        let report = StepReport {
            name: "GPS workflow".into(),
            selector: ReportSelector {
                name: "Stop".into(),
                role: Some("Button".into()),
            },
            outcome: "Passed".into(),
            polls: 1,
            snapshots: 1,
            dispatches: 1,
            pumps: 0,
            elapsed_ms: 1,
            diagnostics: summary(&snapshot),
        };
        let encoded = serde_json::to_string(&report).unwrap();
        for private in [
            "12.345678",
            "45.678901",
            "Canary Street",
            "private-input-canary",
            "Title",
        ] {
            assert!(!encoded.contains(private));
        }
        assert!(encoded.contains("Stop"));
        assert!(encoded.contains("TextView"));
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_input_acknowledgement_stops_without_trying_the_declared_fallback() {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("lince-ui-ack-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let adb = directory.join("adb");
        let hierarchy = xml(
            r#"<node package="social.lince.mobile.debug" class="android.widget.Button" text="Confirm" enabled="true" bounds="[0,0][100,40]"/>"#,
        );
        fs::write(&adb, format!("#!/bin/sh\ncase \"$4\" in\n'dumpsys activity activities') printf '%s\\n' 'mResumedActivity: ActivityRecord{{a u0 social.lince.mobile.debug/.MainActivity t1}}';;\nlince_ui_dump=*) printf '%s\\n' {};;\n*) exit 1;;\nesac\n", quote(&hierarchy))).unwrap();
        fs::set_permissions(&adb, fs::Permissions::from_mode(0o700)).unwrap();
        let mut backend = Android {
            adb,
            device: "test".into(),
            allowed: vec![PACKAGE.into()],
            cached: None,
            diagnostics: String::new(),
            terminal: false,
            reject: vec![],
        };
        let step = Step::action(
            "uncertain command",
            Selector::new("Confirm"),
            Action {
                kind: "tap".into(),
                value: None,
                key: None,
                direction: None,
                fallback: Some("Confirm".into()),
            },
            Expect {
                visible: Some(Target::Name("Confirm".into())),
                absent: None,
                value: None,
            },
        )
        .with_fallback();
        let report = Runner::new(Limits::default()).run(&mut backend, &step);
        assert!(matches!(
            report.outcome,
            lince_interface::testing::Outcome::BackendError(_)
        ));
        assert_eq!(report.metrics.dispatches, 1);
        assert!(backend.terminal);
        assert!(!report.diagnostics.contains("input tap"));
        fs::remove_dir_all(directory).unwrap();
    }
}
