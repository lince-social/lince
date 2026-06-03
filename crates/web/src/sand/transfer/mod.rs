use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "sand.transfer";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "transfer.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "⇄".into(),
            title: "Transfer".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Placeholder transfer sand.".into(),
            details: "Reserved for the transfer workflow; currently only exposes the name and an empty body.".into(),
            initial_width: 4,
            initial_height: 3,
            requires_server: false,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![r#"
      :root {
        color-scheme: dark;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        margin: 0;
        min-height: 100%;
        background: transparent;
      }

      body {
        min-height: 100vh;
        display: grid;
        place-items: center;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        color: #eef2f7;
      }

      .transfer {
        font-size: 1rem;
        font-weight: 600;
        letter-spacing: 0.02em;
      }
"#],
        body: body(),
        body_scripts: vec![],
    }
}

fn body() -> Markup {
    html! {
        main class="transfer" {
            "Transfer"
        }
    }
}
