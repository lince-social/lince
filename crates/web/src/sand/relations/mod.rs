mod body;
mod script;
mod styles;

use {
    crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.relations";

pub(crate) fn package() -> crate::domain::lince_package::LincePackage {
    let manifest = PackageManifest {
        icon: "⟠".into(),
        title: "Relations".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Directed record graph for Need-Contribution relations.".into(),
        details: "Graph view for records with persistent category and need-head filters, local graph physics, and host-backed connect/disconnect relation edits.".into(),
        initial_width: 7,
        initial_height: 6,
        requires_server: true,
        permissions: vec![
            "bridge_state".into(),
            "read_view_stream".into(),
            "write_records".into(),
            "write_table".into(),
        ],
    };

    let mut assets = BTreeMap::new();
    assets.insert(
        "d3.v7.min.js".into(),
        include_bytes!("d3.v7.min.js").to_vec(),
    );
    assets.insert("LICENSE.txt".into(), include_bytes!("LICENSE.txt").to_vec());

    crate::domain::lince_package::LincePackage::new_archive(
        Some("relations.lince".into()),
        manifest,
        document(),
        "index.html",
        assets,
    )
    .expect("relations official sand should render as a valid archive package")
}

fn document() -> String {
    let source = source();
    let markup = maud::html! {
        (maud::DOCTYPE)
        html lang=(source.lang) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (source.manifest.title.as_str()) }
                @for style_block in &source.inline_styles {
                    style { (maud::PreEscaped(style_block)) }
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
                            script { (maud::PreEscaped(code.as_str())) }
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
        filename: "relations.lince",
        lang: "en",
        manifest: PackageManifest {
            icon: "⟠".into(),
            title: "Relations".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Directed record graph for Need-Contribution relations.".into(),
            details: "Graph view for records with persistent category and need-head filters, local graph physics, and host-backed connect/disconnect relation edits.".into(),
            initial_width: 7,
            initial_height: 6,
            requires_server: true,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
                "write_table".into(),
            ],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![
            crate::sand::WidgetScript::src("d3.v7.min.js"),
            crate::sand::WidgetScript::inline(script::script()),
        ],
    }
}
