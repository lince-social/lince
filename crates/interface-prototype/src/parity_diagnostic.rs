use lince_interface::semantic::{
    DefinitionGraph, FieldValue, ResultTemplate, SandDefinition, TemplateInstance,
    composition_fixture,
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use serde::Serialize;
use std::{fs, path::PathBuf};

const BEHAVIOR_MODULE: &str = r#"
const root=document.querySelector('[data-lince-root]');
root.addEventListener('click',event=>{
  const source=event.target.closest('[data-output]');
  if(!source)return;
  const detail={schema_version:1,event:source.dataset.output,record_uid:source.dataset.record};
  root.dispatchEvent(new CustomEvent('lince-output',{detail,bubbles:true}));
  document.querySelector('[data-event-log]').textContent=JSON.stringify(detail,null,2);
});
root.addEventListener('lince-output',event=>{
  if(root.dataset.authority==='facade')return;
  root.dataset.latestEvent=event.detail.event;
});
"#;

#[derive(Serialize)]
struct Assertion {
    name: &'static str,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
struct ParityReport {
    schema_version: u32,
    gate: &'static str,
    status: &'static str,
    definition_count: usize,
    instance_count: usize,
    mapping_count: usize,
    operation_count: usize,
    browser_path: String,
    facade_path: String,
    replay_path: String,
    assertions: Vec<Assertion>,
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ReplayOp {
    Place {
        instance_uid: String,
        x: f64,
        y: f64,
    },
    SetToken {
        instance_uid: String,
        name: String,
        value: String,
    },
    Connect {
        from: String,
        to: String,
    },
}

fn main() {
    match run() {
        Ok(report) => {
            println!(
                "browser parity {}: {} instances, {} mappings",
                report.status, report.instance_count, report.mapping_count
            );
            println!("browser {}", report.browser_path);
            println!("facade {}", report.facade_path);
            if report.status != "passed" {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("browser parity diagnostic failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<ParityReport, Box<dyn std::error::Error>> {
    let (graph, protein, template) = composition_fixture();
    graph.validate()?;
    protein.validate()?;
    let instances = template.instantiate(&graph, &protein)?;
    let root = PathBuf::from("target/interface-laboratory/parity");
    fs::create_dir_all(&root)?;
    let browser_path = root.join("browser.html");
    let facade_path = root.join("facade.html");
    let replay_path = root.join("replay.json");
    let report_path = root.join("report.json");
    let browser = document(&graph, &template, &instances, false).into_string();
    let facade = document(&graph, &template, &instances, true).into_string();
    let replay = replay_fixture(&instances);
    fs::write(&browser_path, &browser)?;
    fs::write(&facade_path, &facade)?;
    fs::write(&replay_path, serde_json::to_vec_pretty(&replay)?)?;
    let instance = instances
        .first()
        .ok_or_else(|| "parity fixture produced no instance".to_string())?;
    let assertions = vec![
        Assertion {
            name: "browser carries stable semantic identity",
            passed: browser.contains(&instance.uid)
                && browser.contains(&instance.definition_uid)
                && browser.contains("record_clicked"),
            detail: instance.uid.clone(),
        },
        Assertion {
            name: "Maud recursively composes primitive Sands",
            passed: ["background", "title", "description", "status", "open"]
                .iter()
                .all(|child| browser.contains(&format!("data-child=\"{child}\""))),
            detail: "panel, text, dropdown and Button children are ordinary nested Sand nodes"
                .into(),
        },
        Assertion {
            name: "browser exposes typed output behavior",
            passed: browser.contains("data-output=\"record_clicked\"")
                && browser.contains("lince-output"),
            detail: "one pure-JavaScript module emits the declared record output".into(),
        },
        Assertion {
            name: "Facade omits Action and editor authority",
            passed: facade.contains("data-authority=\"facade\"")
                && !facade.contains("data-action=")
                && !facade.contains("data-editor-authority="),
            detail: "Facade retains visitor-local events and no durable write capability".into(),
        },
        Assertion {
            name: "replay names only semantic identities",
            passed: replay.len() == 3
                && !serde_json::to_string(&replay)?.contains("bevy")
                && !serde_json::to_string(&replay)?.contains("cef"),
            detail: format!("{} version-local operations", replay.len()),
        },
    ];
    let status = if assertions.iter().all(|assertion| assertion.passed) {
        "passed"
    } else {
        "failed"
    };
    let report = ParityReport {
        schema_version: 1,
        gate: "browser and Facade parity",
        status,
        definition_count: graph.definitions.len(),
        instance_count: instances.len(),
        mapping_count: template.mappings.len(),
        operation_count: replay.len(),
        browser_path: browser_path.display().to_string(),
        facade_path: facade_path.display().to_string(),
        replay_path: replay_path.display().to_string(),
        assertions,
    };
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn document(
    graph: &DefinitionGraph,
    template: &ResultTemplate,
    instances: &[TemplateInstance],
    facade: bool,
) -> Markup {
    let authority = if facade { "facade" } else { "browser" };
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lince Sand parity" }
                style { (PreEscaped(STYLE)) }
            }
            body {
                main data-lince-root data-schema-version="1" data-authority=(authority) {
                    header {
                        span class="eyebrow" { "SHARED SAND DEFINITION" }
                        h1 { "Record-card Castle" }
                        p { "The same semantic graph rendered as ordinary accessible HTML." }
                    }
                    section class="workspace" aria-label="Protein result Sands" {
                        @for instance in instances {
                            (render_instance(graph, template, instance, facade))
                        }
                    }
                    section class="event" aria-live="polite" {
                        h2 { "Local event" }
                        pre data-event-log { "No event yet" }
                    }
                }
                script type="module" { (PreEscaped(BEHAVIOR_MODULE)) }
            }
        }
    }
}

fn render_instance(
    graph: &DefinitionGraph,
    template: &ResultTemplate,
    instance: &TemplateInstance,
    facade: bool,
) -> Markup {
    let definition = graph.definitions.get(&template.definition_uid);
    html! {
        article
            class="sand castle"
            data-instance=(instance.uid)
            data-definition=(instance.definition_uid)
            data-protein=(instance.protein_uid)
            data-row=(instance.row_key)
        {
            @if let Some(definition) = definition {
                @for child in ordered_children(definition) {
                    (render_child(child.uid.as_str(), instance, facade))
                }
            }
            details class="why" {
                summary { "Why is it here?" }
                p { "Protein " (instance.protein_uid) ", row " (instance.row_key) ", template " (template.uid) "." }
            }
        }
    }
}

fn ordered_children(
    definition: &SandDefinition,
) -> Vec<&lince_interface::semantic::DefinitionChild> {
    let mut children = definition.children.iter().collect::<Vec<_>>();
    children.sort_by_key(|child| child.sibling_order);
    children
}

fn render_child(uid: &str, instance: &TemplateInstance, facade: bool) -> Markup {
    let value = |port: &str| instance.values.get(&format!("{uid}.{port}"));
    match uid {
        "background" => html! { div class="panel" data-child=(uid) {} },
        "title" | "description" => html! {
            p class=(uid) data-child=(uid) { (text_value(value("text"))) }
        },
        "status" => html! {
            label class="status" data-child=(uid) {
                span { "Status" }
                select disabled[facade] {
                    option selected { (text_value(value("selected"))) }
                }
            }
        },
        "open" => html! {
            button
                type="button"
                data-child=(uid)
                data-output="record_clicked"
                data-record=(record_value(value("record")))
            { (text_value(value("label"))) }
        },
        _ => html! { div data-child=(uid) {} },
    }
}

fn text_value(value: Option<&FieldValue>) -> &str {
    match value {
        Some(FieldValue::Text(value)) => value,
        _ => "",
    }
}

fn record_value(value: Option<&FieldValue>) -> &str {
    match value {
        Some(FieldValue::Record(value)) => value,
        _ => "",
    }
}

fn replay_fixture(instances: &[TemplateInstance]) -> Vec<ReplayOp> {
    let uid = instances
        .first()
        .map(|instance| instance.uid.clone())
        .unwrap_or_else(|| "missing".into());
    vec![
        ReplayOp::Place {
            instance_uid: uid.clone(),
            x: 120.0,
            y: 96.0,
        },
        ReplayOp::SetToken {
            instance_uid: uid.clone(),
            name: "sand.radius".into(),
            value: "14".into(),
        },
        ReplayOp::Connect {
            from: format!("{uid}:open.clicked"),
            to: "box:record-detail.record".into(),
        },
    ]
}

const STYLE: &str = r#"
:root{font:16px system-ui;color:#222;background:#efeee8;--radius:18px}
*{box-sizing:border-box}body{margin:0}main{max-width:1000px;margin:auto;padding:48px}
.eyebrow{font-size:.72rem;letter-spacing:.15em;color:#617067}h1{font-size:2.5rem;margin:.25rem 0}
.workspace{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:24px;margin-top:36px}
.sand{position:relative;padding:24px;border:1px solid #9da79f;border-radius:var(--radius);overflow:hidden}
.panel{position:absolute;inset:0;background:#faf9f4;z-index:-1}.title{font-size:1.4rem;font-weight:650;margin:0}
.description{min-height:4rem;color:#58625c}.status{display:flex;gap:10px;align-items:center}
button,select{font:inherit;border:1px solid #809287;border-radius:9px;background:#fff;padding:8px 12px}
button{float:right}.why{clear:both;padding-top:18px;color:#4e6257}.event{margin-top:30px;border-top:1px solid #aaa;padding-top:18px}
pre{white-space:pre-wrap}[data-authority=facade]::before{content:'READ-ONLY FACADE';display:block;color:#587164;font-size:.72rem;letter-spacing:.12em}
@media(prefers-color-scheme:dark){:root{color:#eef2ed;background:#121613}.panel{background:#1d241f}button,select{color:#eef2ed;background:#27312a}.description{color:#b9c5bd}}
"#;
