use crate::{
    domain::lince_package::PackageManifest,
    sand::{HeadLink, SandWidgetSource, WidgetScript},
};
use maud::{Markup, PreEscaped, html};

pub(crate) const FEATURE_FLAG: &str = "shell";
const LINCE_LOGO_SVG: &str = include_str!("../../static/lince_logo_white.svg");

const STYLE: &str = r#"
:root{
  color-scheme:dark;
  --text-strong:#f6f8fb;
  --text-soft:#c8d0dc;
  --text-muted:#788190;
  --border:rgba(255,255,255,.08);
  --border-strong:rgba(255,255,255,.16);
  --primary-background:#121214;
  --secondary-background:#141416;
  --raised-background:var(--primary-background);
  --primary-ink:#f8fafc;
  --secondary-ink:#a7b4c2;
  --tooltip:var(--primary-background);
  --tooltip-ink:var(--primary-ink);
  --tooltip-border:#fff;
  --gray:#a7b4c2;
  --accent:#6366f1;
  --focus:#6366f1;
  --transition-fast:160ms ease;
  font-family:"IBM Plex Sans",Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
  background:transparent;
  color:var(--text-strong);
}
*{box-sizing:border-box}
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}
button,input{font:inherit}
button{cursor:pointer}
.brand{display:flex;align-items:center;gap:10px;width:100%;height:100%}
.brand-mark{display:grid;place-items:center;width:28px;height:28px;color:var(--text-strong);background:none}
.brand-logo,.brand-logo svg{width:100%;height:auto}.brand-logo svg{display:block}.brand-logo .s0{stroke-width:50!important}
.brand-name{color:var(--text-strong);font-size:1rem;font-weight:600;letter-spacing:.03em}
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

fn source(
    filename: &'static str,
    title: &str,
    description: &str,
    body: Markup,
    script: &'static str,
) -> SandWidgetSource {
    let body_scripts = if script.trim().is_empty() {
        vec![]
    } else {
        vec![
            WidgetScript::src("/host/static/presentation/board/widget-frame-bootstrap.js"),
            WidgetScript::src("/host/static/presentation/board/LynxDS-components.js"),
            WidgetScript::src("/board/lynx-ui.js"),
            WidgetScript::inline(script),
        ]
    };

    SandWidgetSource {
        filename,
        lang: "en",
        manifest: manifest(title, description),
        head_links: vec![HeadLink {
            rel: "stylesheet",
            href: "/board/lynx-ui.css",
        }],
        inline_styles: vec![STYLE],
        body,
        body_scripts,
    }
}

#[allow(dead_code)]
pub(crate) fn logo_source() -> SandWidgetSource {
    source(
        "lince-shell-logo.html",
        "Lince Logo",
        "Pinned shell logo.",
        html! {
            div class="brand" {
                div class="brand-mark" aria-hidden="true" {
                    div class="brand-logo" { (PreEscaped(LINCE_LOGO_SVG)) }
                }
                span class="brand-name" { "Lince" }
            }
        },
        "",
    )
}

pub(crate) fn ai_source() -> SandWidgetSource {
    source(
        "lince-shell-ai.html",
        "AI",
        "Seed AI entry point.",
        html! { section class="aiPanel" { h1 { "AI" } p { "Open the local AI builder to draft and install new sand widgets." } button id="open-ai" { "Open AI" } } },
        r#"function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}document.getElementById("open-ai")?.addEventListener("click",()=>withHost((host)=>host.shell("navigation.ai",{})));"#,
    )
}
