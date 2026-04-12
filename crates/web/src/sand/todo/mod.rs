mod body;
mod script;
mod style;

use {
    crate::{
        domain::lince_package::{LincePackage, PackageManifest},
        sand::SandWidgetSource,
    },
    maud::{DOCTYPE, Markup, PreEscaped, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.view_todo_editor";

pub(crate) fn package() -> LincePackage {
    let source = source();
    let mut assets = BTreeMap::new();
    assets.insert("blob.wgsl".into(), include_bytes!("blob.wgsl").to_vec());

    LincePackage::new_archive(
        Some("todo.lince".into()),
        source.manifest.clone(),
        document(&source),
        "index.html",
        assets,
    )
    .expect("todo official sand should render as a valid archive package")
}

fn document(source: &SandWidgetSource) -> String {
    let markup: Markup = html! {
        (DOCTYPE)
        html lang=(source.lang) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (source.manifest.title.as_str()) }
                @for style_block in &source.inline_styles {
                    style { (PreEscaped(style_block)) }
                }
            }
            body {
                (source.body)
                @for script in &source.body_scripts {
                    @match script {
                        crate::sand::WidgetScript::Src(src) => {
                            script src=(src) {}
                        }
                        crate::sand::WidgetScript::Inline(code) => {
                            script { (PreEscaped(code.as_str())) }
                        }
                    }
                }
            }
        }
    };

    markup.into_string()
}

fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "todo.lince",
        lang: "en",
        manifest: PackageManifest {
            icon: "▦".into(),
            title: "Todo".into(),
            author: "Lince Labs".into(),
            version: "1.0.0".into(),
            description: "Standalone todo list driven by the normal view SSE stream.".into(),
            details: "Reads server_id and view_id from the host, subscribes to the standard view stream, and renders a focusable list with an active item plus a details drawer.".into(),
            initial_width: 7,
            initial_height: 5,
            requires_server: true,
            permissions: vec!["bridge_state".into(), "read_view_stream".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
