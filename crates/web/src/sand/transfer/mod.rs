mod body;
mod script;
mod styles;

use {
    crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.transfer";

pub(crate) fn package() -> crate::domain::lince_package::LincePackage {
    let source = source();
    let mut assets = BTreeMap::new();
    assets.insert(
        "d3.v7.min.js".into(),
        include_bytes!("../relations/d3.v7.min.js").to_vec(),
    );
    assets.insert(
        "LICENSE.txt".into(),
        include_bytes!("../relations/LICENSE.txt").to_vec(),
    );
    let document = document(&source);

    crate::domain::lince_package::LincePackage::new_archive(
        Some("transfer.lince".into()),
        source.manifest,
        document,
        "index.html",
        assets,
    )
    .expect("transfer official sand should render as a valid archive package")
}

fn document(source: &SandWidgetSource) -> String {
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
        filename: "transfer.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "T".into(),
            title: "Transfer".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Manual signed Transfer workflow.".into(),
            details:
                "Local-coordinator Transfer flow for signed proposal packages between Lince Organs."
                    .into(),
            initial_width: 8,
            initial_height: 6,
            requires_server: false,
            permissions: vec![
                "bridge_state".into(),
                "read_transfer_stream".into(),
                "write_transfer".into(),
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
