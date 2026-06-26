use crate::{
    domain::lince_package::PackageManifest,
    sand::{SandWidgetSource, WidgetScript},
};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "shell";

const STYLE: &str = r#"
:root{color-scheme:dark;font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:transparent;color:#f4f7fb}
*{box-sizing:border-box}html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}
button,input{font:inherit}button{cursor:pointer;border:0;color:inherit;background:transparent}
.shell{width:100%;height:100%;display:flex;align-items:center;gap:8px;border:1px solid rgba(255,255,255,.12);background:rgba(30,33,38,.92);box-shadow:0 10px 30px rgba(0,0,0,.24);border-radius:8px;padding:8px}
.brand{justify-content:flex-start;font-weight:750;letter-spacing:0}.brandMark{width:28px;height:28px;border-radius:6px;background:#f4f7fb;color:#17191d;display:grid;place-items:center;font-weight:900}.brandName{font-size:18px}
.operation input{flex:1;min-width:0;height:36px;border:1px solid rgba(255,255,255,.14);border-radius:6px;background:#15171b;color:#f4f7fb;padding:0 12px;outline:none}.operation button,.workspaces button,.notif button,.edit button,.zoom button,.ai button{height:36px;border-radius:6px;background:#3a414b;padding:0 12px;font-weight:650}.operation button:hover,.workspaces button:hover,.notif button:hover,.edit button:hover,.zoom button:hover,.ai button:hover{background:#4a5360}
.workspaces{justify-content:space-between}.workspaceName{font-weight:750;min-width:72px;text-align:center}.notif,.edit{justify-content:center}.badge{min-width:18px;height:18px;border-radius:999px;background:#e7b454;color:#181a1f;display:inline-grid;place-items:center;font-size:12px;margin-left:4px}
.editPopover{position:fixed;right:0;top:46px;width:176px;display:none;flex-direction:column;gap:6px;padding:8px;border-radius:8px;background:#22262d;border:1px solid rgba(255,255,255,.14);box-shadow:0 16px 40px rgba(0,0,0,.35)}.editPopover.isOpen{display:flex}.editPopover label{display:grid;gap:4px;font-size:11px;color:#bac3cf}.editPopover input{width:100%}
.zoom{justify-content:center}.zoom .percent{min-width:64px;text-align:center;background:#15171b}.tutorial,.aiPanel{display:block;padding:18px;align-items:stretch}.tutorial h1,.aiPanel h1{font-size:18px;margin:0 0 8px}.tutorial p,.aiPanel p{font-size:14px;line-height:1.45;margin:0;color:#cbd3dd}.aiPanel button{margin-top:14px}
"#;

fn manifest(title: &str, description: &str) -> PackageManifest {
    PackageManifest {
        icon: "◎".into(),
        title: title.into(),
        description: description.into(),
        details: description.into(),
        author: "Lince".into(),
        version: "1.0.0".into(),
        initial_width: 2,
        initial_height: 1,
        requires_server: false,
        permissions: vec!["bridge_state".into(), "shell_board".into()],
    }
}

fn source(filename: &'static str, title: &str, description: &str, body: Markup, script: &'static str) -> SandWidgetSource {
    let body_scripts = if script.trim().is_empty() {
        vec![]
    } else {
        vec![
            WidgetScript::src("/host/static/presentation/board/widget-frame-bootstrap.js"),
            WidgetScript::inline(script),
        ]
    };

    SandWidgetSource {
        filename,
        lang: "en",
        manifest: manifest(title, description),
        head_links: vec![],
        inline_styles: vec![STYLE],
        body,
        body_scripts,
    }
}

pub(crate) fn logo_source() -> SandWidgetSource {
    source(
        "lince-shell-logo.html",
        "Lince Logo",
        "Pinned shell logo.",
        html! { div class="shell brand" { div class="brandMark" { "L" } span class="brandName" { "Lince" } } },
        "",
    )
}

pub(crate) fn operation_source() -> SandWidgetSource {
    source(
        "lince-shell-operation.html",
        "Lince Operation",
        "Pinned operation launcher.",
        html! {
            form class="shell operation" id="operation-form" {
                input id="operation-input" placeholder="Operation" autocomplete="off";
                button type="submit" { "Run" }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const form=document.getElementById("operation-form");const input=document.getElementById("operation-input");
form?.addEventListener("submit",(event)=>{event.preventDefault();withHost((host)=>host.shell("operation.submit",{value:input?.value||""}));if(input)input.value="";});
"#,
    )
}

pub(crate) fn workspaces_source() -> SandWidgetSource {
    source(
        "lince-shell-workspaces.html",
        "Lince Workspaces",
        "Pinned workspace controller.",
        html! {
            div class="shell workspaces" {
                button id="prev" aria-label="Previous workspace" { "‹" }
                button id="current" class="workspaceName" aria-label="Open workspaces" { "Area" }
                button id="next" aria-label="Next workspace" { "›" }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const current=document.getElementById("current");
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};current.textContent=shell.workspace?.label||"Area";}));
document.getElementById("prev")?.addEventListener("click",()=>withHost((host)=>host.shell("workspace.relative",{direction:-1})));
document.getElementById("next")?.addEventListener("click",()=>withHost((host)=>host.shell("workspace.relative",{direction:1})));
current?.addEventListener("click",()=>withHost((host)=>host.shell("workspace.toggle",{})));
"#,
    )
}

pub(crate) fn notifications_source() -> SandWidgetSource {
    source(
        "lince-shell-notifications.html",
        "Lince Notifications",
        "Pinned notification control.",
        html! { div class="shell notif" { button id="toggle" aria-label="Notifications" { "!" span id="count" class="badge" hidden { "0" } } } },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const count=document.getElementById("count");
withHost((host)=>host.subscribe(({meta})=>{const value=Number(meta.shell?.notifications?.count||0);count.hidden=value<=0;count.textContent=String(value);}));
document.getElementById("toggle")?.addEventListener("click",()=>withHost((host)=>host.shell("notifications.toggle",{})));
"#,
    )
}

pub(crate) fn edit_source() -> SandWidgetSource {
    source(
        "lince-shell-edit.html",
        "Lince Edit",
        "Pinned edit controls.",
        html! {
            div class="shell edit" {
                button id="edit" { "Edit" }
                div id="popover" class="editPopover" {
                    button data-action="card.add" { "Add card" }
                    button data-action="card.import" { "Import" }
                    button data-action="card.local" { "Local" }
                    button data-action="card.dna" { "DNA" }
                    label { "Grid" input id="density" type="range" min="1" max="7" step="1"; }
                }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const edit=document.getElementById("edit");const popover=document.getElementById("popover");const density=document.getElementById("density");
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};const editing=shell.editMode===true;edit.textContent=editing?"Done":"Edit";popover.classList.toggle("isOpen",editing);if(density&&shell.density)density.value=String(shell.density);}));
edit?.addEventListener("click",()=>withHost((host)=>host.shell("edit.toggle",{})));
popover?.addEventListener("click",(event)=>{const button=event.target.closest("button[data-action]");if(button)withHost((host)=>host.shell(button.dataset.action,{}));});
density?.addEventListener("input",()=>withHost((host)=>host.shell("density.set",{value:Number(density.value)||4})));
"#,
    )
}

pub(crate) fn zoom_source() -> SandWidgetSource {
    source(
        "lince-shell-zoom.html",
        "Lince Zoom",
        "Pinned canvas zoom controls.",
        html! {
            div class="shell zoom" {
                button data-action="zoom.out" aria-label="Zoom out" { "−" }
                button data-action="zoom.reset" id="percent" class="percent" { "100%" }
                button data-action="zoom.in" aria-label="Zoom in" { "+" }
                button data-action="zoom.center" aria-label="Recenter" { "⌖" }
                button data-action="layout.circle" aria-label="Organize" { "◎" }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const percent=document.getElementById("percent");
withHost((host)=>host.subscribe(({meta})=>{const zoom=Number(meta.shell?.zoom?.percent||100);percent.textContent=`${Math.round(zoom)}%`;}));
document.body.addEventListener("click",(event)=>{const button=event.target.closest("button[data-action]");if(button)withHost((host)=>host.shell(button.dataset.action,{}));});
"#,
    )
}

pub(crate) fn ai_source() -> SandWidgetSource {
    source(
        "lince-shell-ai.html",
        "AI",
        "Seed AI entry point.",
        html! { section class="shell aiPanel" { h1 { "AI" } p { "Open the local AI builder to draft and install new sand widgets." } button id="open-ai" { "Open AI" } } },
        r#"function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}document.getElementById("open-ai")?.addEventListener("click",()=>withHost((host)=>host.shell("navigation.ai",{})));"#,
    )
}

pub(crate) fn tutorial_source() -> SandWidgetSource {
    source(
        "lince-shell-tutorial.html",
        "Tutorial",
        "Seed workspace tutorial.",
        html! { section class="shell tutorial" { h1 { "Tutorial" } p { "This workspace is a large 2D canvas. Pinned sand stays above the workspace; regular sand lives in the zoomable world." } } },
        "",
    )
}
