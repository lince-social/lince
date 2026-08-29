use lince_interface::semantic::{
    DefinitionGraph, FieldValue, ResultTemplate, SandDefinition, TemplateInstance,
    composition_fixture,
};
use lince_interface::{
    composition::{
        COMPOSITION_SCHEMA_VERSION, CompositionArtifact, CompositionWorkbenchState,
        DefinitionCatalog, NodeAddress, composition_schema, composition_workbench_fixture,
        composition_workbench_package, dom_identity,
    },
    primitive_gallery::{
        INSTALLED_GALLERY_ABI_JS, INSTALLED_GALLERY_ABI_JS_PATH, INSTALLED_GALLERY_COMPOSITION_JS,
        INSTALLED_GALLERY_COMPOSITION_JS_PATH, INSTALLED_GALLERY_CSS, INSTALLED_GALLERY_CSS_PATH,
        INSTALLED_GALLERY_HTML, INSTALLED_GALLERY_HTML_PATH, INSTALLED_GALLERY_JS,
        INSTALLED_GALLERY_JS_PATH, INSTALLED_GALLERY_PROBES_JS, INSTALLED_GALLERY_PROBES_JS_PATH,
        PRIMITIVE_COUNT, primitive_gallery_package,
    },
    sand::{DefinitionRef, SAND_ABI_VERSION, SAND_SCHEMA_VERSION, SandElement, contract_schemas},
};
use lince_interface::{git_dirty, raw_git_revision, source_fingerprint};
use maud::{DOCTYPE, Markup, html};
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
    git_revision: String,
    git_dirty: bool,
    source_fingerprint_sha256: String,
    definition_count: usize,
    instance_count: usize,
    mapping_count: usize,
    operation_count: usize,
    browser_path: String,
    facade_path: String,
    replay_path: String,
    contract_schema_path: String,
    valid_package_path: String,
    invalid_package_path: String,
    primitive_count: usize,
    primitive_package_sha256: String,
    composition_path: String,
    composition_schema_path: String,
    valid_composition_path: String,
    invalid_composition_path: String,
    composition_definition_count: usize,
    composition_package_sha256: String,
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
    let style_path = root.join("parity.css");
    let behavior_path = root.join("parity.js");
    let composition_path = root.join("composition.html");
    let browser = document(&graph, &template, &instances, false).into_string();
    let facade = document(&graph, &template, &instances, true).into_string();
    let composition_artifact = composition_workbench_fixture()?;
    let composition = composition_document(&composition_artifact).into_string();
    let composition_package = composition_workbench_package();
    composition_package.validate()?;
    let composition_package_sha256 = composition_package.graph_sha256()?;
    let mut workbench = CompositionWorkbenchState::new()?;
    workbench.exercise()?;
    let replay = replay_fixture(&instances);
    fs::write(&browser_path, &browser)?;
    fs::write(&facade_path, &facade)?;
    fs::write(&style_path, STYLE)?;
    fs::write(&behavior_path, BEHAVIOR_MODULE)?;
    fs::write(&composition_path, &composition)?;
    fs::write(&replay_path, serde_json::to_vec_pretty(&replay)?)?;
    let contract_root = PathBuf::from("target/interface-laboratory/sand-contract");
    fs::create_dir_all(&contract_root)?;
    let contract_schema_path = contract_root.join("schemas.json");
    let valid_package_path = contract_root.join("valid-package.json");
    let invalid_package_path = contract_root.join("invalid-package.json");
    let composition_schema_path = contract_root.join("composition-schema.json");
    let valid_composition_path = contract_root.join("valid-composition.json");
    let invalid_composition_path = contract_root.join("invalid-composition.json");
    let primitive_package = primitive_gallery_package();
    primitive_package.validate()?;
    let primitive_package_sha256 = primitive_package.graph_sha256()?;
    let mut invalid_package = primitive_package.clone();
    invalid_package.assets[0].sha256 = "sha256:stale".into();
    let invalid_refused = invalid_package.validate().is_err();
    fs::write(
        &contract_schema_path,
        serde_json::to_vec_pretty(&contract_schemas()?)?,
    )?;
    fs::write(
        &valid_package_path,
        serde_json::to_vec_pretty(&primitive_package)?,
    )?;
    fs::write(
        &invalid_package_path,
        serde_json::to_vec_pretty(&invalid_package)?,
    )?;
    let valid_composition = composition_artifact.to_json()?;
    let mut invalid_composition = serde_json::to_value(&composition_artifact)?;
    invalid_composition["unknown"] = serde_json::Value::Bool(true);
    let invalid_composition = serde_json::to_vec_pretty(&invalid_composition)?;
    let invalid_composition_refused = CompositionArtifact::from_json(&invalid_composition).is_err();
    fs::write(
        &composition_schema_path,
        serde_json::to_vec_pretty(&composition_schema()?)?,
    )?;
    fs::write(&valid_composition_path, valid_composition)?;
    fs::write(&invalid_composition_path, invalid_composition)?;
    for (path, source) in [
        (INSTALLED_GALLERY_HTML_PATH, INSTALLED_GALLERY_HTML),
        (INSTALLED_GALLERY_CSS_PATH, INSTALLED_GALLERY_CSS),
        (INSTALLED_GALLERY_JS_PATH, INSTALLED_GALLERY_JS),
        (INSTALLED_GALLERY_ABI_JS_PATH, INSTALLED_GALLERY_ABI_JS),
        (
            INSTALLED_GALLERY_COMPOSITION_JS_PATH,
            INSTALLED_GALLERY_COMPOSITION_JS,
        ),
        (
            INSTALLED_GALLERY_PROBES_JS_PATH,
            INSTALLED_GALLERY_PROBES_JS,
        ),
    ] {
        let destination = contract_root.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, source)?;
    }
    let instance = instances
        .first()
        .ok_or_else(|| "parity fixture produced no instance".to_string())?;
    let assertions = vec![
        Assertion {
            name: "browser carries stable semantic identity",
            passed: browser.contains(&instance.uid)
                && browser.contains(&instance.definition_uid)
                && browser.contains("record-clicked"),
            detail: instance.uid.clone(),
        },
        Assertion {
            name: "versioned Sand schemas and valid/invalid fixtures are generated",
            passed: invalid_refused
                && primitive_package.graph.definitions.len() == PRIMITIVE_COUNT
                && primitive_package_sha256.starts_with("sha256:")
                && SAND_SCHEMA_VERSION == 1
                && SAND_ABI_VERSION == 1,
            detail: format!(
                "{} primitives, schema {}, ABI {}, {}",
                PRIMITIVE_COUNT, SAND_SCHEMA_VERSION, SAND_ABI_VERSION, primitive_package_sha256
            ),
        },
        Assertion {
            name: "first-party HTML keeps CSS and Behavior in declared external assets",
            passed: browser.contains("href=\"parity.css\"")
                && browser.contains("src=\"parity.js\"")
                && !browser.contains("<style")
                && !browser.contains("<script>")
                && !browser.contains("onclick=")
                && !BEHAVIOR_MODULE.contains("eval("),
            detail: "Maud structure, scoped CSS and native ES-module Behavior remain separate"
                .into(),
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
            name: "native constructors Maud and workbench consume one recursive definition graph",
            passed: composition.contains("data-definition=\"video-call-room\"")
                && composition.contains("data-node-path=\"call/open\"")
                && composition.contains("data-output=\"pressed\"")
                && composition.contains("READ  current-records.title")
                && composition.contains("WRITE room-native.record-clicked")
                && composition_package.graph.definitions.len() == PRIMITIVE_COUNT + 2
                && composition_package_sha256.starts_with("sha256:")
                && workbench.facts().mounted_placements == 5,
            detail: format!(
                "{} normalized definitions, {} mounted nodes, {}",
                composition_package.graph.definitions.len(),
                workbench.facts().mounted_nodes,
                composition_package_sha256
            ),
        },
        Assertion {
            name: "composition save reopen and invalid artifact evidence are generated",
            passed: invalid_composition_refused
                && workbench.facts().save_reopens > 0
                && workbench.facts().rejected_publications > 0
                && workbench.facts().retired_behavior_handles > 0
                && workbench.facts().lock_changes > 0
                && workbench.facts().style_override_sets > 0
                && workbench.facts().style_override_resets > 0
                && workbench.facts().shared_publications > 0
                && workbench.facts().saved_definitions > 0
                && workbench.facts().forked_definitions > 0
                && COMPOSITION_SCHEMA_VERSION == 1,
            detail: format!(
                "schema {}, {} reopen, {} retired Behavior, {} refused publication",
                COMPOSITION_SCHEMA_VERSION,
                workbench.facts().save_reopens,
                workbench.facts().retired_behavior_handles,
                workbench.facts().rejected_publications
            ),
        },
        Assertion {
            name: "browser exposes typed output behavior",
            passed: browser.contains("data-output=\"record-clicked\"")
                && BEHAVIOR_MODULE.contains("lince-output"),
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
        schema_version: 4,
        gate: "browser and Facade parity",
        status,
        git_revision: raw_git_revision(),
        git_dirty: git_dirty(),
        source_fingerprint_sha256: source_fingerprint(),
        definition_count: graph.definitions.len(),
        instance_count: instances.len(),
        mapping_count: template.mappings.len(),
        operation_count: replay.len(),
        browser_path: browser_path.display().to_string(),
        facade_path: facade_path.display().to_string(),
        replay_path: replay_path.display().to_string(),
        contract_schema_path: contract_schema_path.display().to_string(),
        valid_package_path: valid_package_path.display().to_string(),
        invalid_package_path: invalid_package_path.display().to_string(),
        primitive_count: PRIMITIVE_COUNT,
        primitive_package_sha256,
        composition_path: composition_path.display().to_string(),
        composition_schema_path: composition_schema_path.display().to_string(),
        valid_composition_path: valid_composition_path.display().to_string(),
        invalid_composition_path: invalid_composition_path.display().to_string(),
        composition_definition_count: composition_package.graph.definitions.len(),
        composition_package_sha256,
        assertions,
    };
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn composition_document(artifact: &CompositionArtifact) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lince recursive composition parity" }
                link rel="stylesheet" href="parity.css";
            }
            body {
                main data-lince-root data-schema-version=(COMPOSITION_SCHEMA_VERSION) data-authority="browser" {
                    header {
                        span class="eyebrow" { "ONE NORMALIZED GRAPH · MAUD PROJECTION" }
                        h1 { "Recursive Sand composition" }
                        p { "The standalone Button and twice-nested Button retain exact definition and port identity." }
                    }
                    section class="workspace" aria-label="Composition placements" {
                        @for placement in &artifact.document.placements {
                            article class="sand castle" data-instance=(placement.instance_uid) data-definition=(placement.definition.uid) data-revision=(placement.definition.revision) data-locked=(placement.locked) {
                                (render_composition_node(
                                    &artifact.catalog,
                                    &NodeAddress {
                                        instance_uid: placement.instance_uid.clone(),
                                        node_path: Vec::new(),
                                    },
                                    &placement.definition,
                                ))
                            }
                        }
                    }
                    section class="event" aria-label="Typed bindings" {
                        h2 { "Typed routes" }
                        @for binding in &artifact.document.bindings {
                            p { code { (binding.visible_arrow()) } }
                        }
                        pre data-event-log { "No event yet" }
                    }
                }
                script type="module" src="parity.js" {}
            }
        }
    }
}

fn render_composition_node(
    catalog: &DefinitionCatalog,
    address: &NodeAddress,
    reference: &DefinitionRef,
) -> Markup {
    let Ok(definition) = catalog.definition(reference) else {
        return html! { strong { "REFUSED missing definition" } };
    };
    let path = if address.node_path.is_empty() {
        "root".into()
    } else {
        address.node_path.join("/")
    };
    let dom_uid = dom_identity(address, "root");
    if definition.element == SandElement::Button {
        let output = definition
            .outputs
            .first()
            .map(|port| port.name.as_str())
            .unwrap_or("none");
        html! {
            button type="button" id=(dom_uid) data-definition=(definition.uid) data-revision=(definition.revision) data-node-path=(path) data-output=(output) data-record="fixture-record" {
                (definition.display_name)
            }
        }
    } else {
        html! {
            section id=(dom_uid) data-definition=(definition.uid) data-revision=(definition.revision) data-node-path=(path) {
                strong { (definition.display_name) }
                @for child in ordered_definition_children(definition) {
                    (render_composition_child(catalog, address, child))
                }
            }
        }
    }
}

fn render_composition_child(
    catalog: &DefinitionCatalog,
    parent: &NodeAddress,
    child: &lince_interface::sand::DefinitionChild,
) -> Markup {
    let mut node_path = parent.node_path.clone();
    node_path.push(child.local_uid.clone());
    render_composition_node(
        catalog,
        &NodeAddress {
            instance_uid: parent.instance_uid.clone(),
            node_path,
        },
        &child.definition,
    )
}

fn ordered_definition_children(
    definition: &SandDefinition,
) -> Vec<&lince_interface::sand::DefinitionChild> {
    let mut children = definition.children.iter().collect::<Vec<_>>();
    children.sort_by_key(|child| child.sibling_order);
    children
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
                link rel="stylesheet" href="parity.css";
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
                script type="module" src="parity.js" {}
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
                    (render_child(child.local_uid.as_str(), instance, facade))
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
                data-output="record-clicked"
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
