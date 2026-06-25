use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "sand.finance";

const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    --bg: #11161d;
    --panel: #1a212b;
    --text: #eef3f8;
    --muted: #9ca8b5;
    --accent: #8fe3aa;
  }

  * {
    box-sizing: border-box;
  }

  body {
    margin: 0;
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 18px;
    background: var(--bg);
    color: var(--text);
  }

  main {
    display: grid;
    gap: 6px;
    width: min(100%, 320px);
    border: 1px solid rgba(143, 227, 170, 0.28);
    border-radius: 8px;
    background: var(--panel);
    padding: 18px;
  }

  h1,
  p {
    margin: 0;
  }

  h1 {
    color: var(--accent);
    font-size: 1.15rem;
  }

  p {
    color: var(--muted);
    font-size: 0.86rem;
  }
"#;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "finance.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "$".into(),
            title: "Finance".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Finance sand placeholder.".into(),
            details: "Minimal finance widget scaffold.".into(),
            initial_width: 3,
            initial_height: 2,
            requires_server: false,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![STYLE],
        body: body(),
        body_scripts: vec![],
    }
}

fn body() -> Markup {
    html! {
        main {
            h1 { "Finance" }
            p { "Hello world" }
        }
    }
}
