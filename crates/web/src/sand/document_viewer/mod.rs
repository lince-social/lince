mod body;
mod script;
mod style;

use {
    crate::{
        domain::lince_package::{LincePackage, PackageManifest},
        sand::{SandWidgetSource, WidgetScript},
    },
    maud::{DOCTYPE, Markup, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.bucket_image_view";
const EPUB_JS: &[u8] = include_bytes!("vendor/epub.min.js");
const JSZIP_JS: &[u8] = include_bytes!("vendor/jszip.min.js");
const EPUBJS_LICENSE: &[u8] = include_bytes!("vendor/EPUBJS-LICENSE.txt");
const JSZIP_LICENSE: &[u8] = include_bytes!("vendor/JSZIP-LICENSE.txt");

pub(crate) fn package() -> LincePackage {
    let source = source();
    let mut assets = BTreeMap::new();
    assets.insert("vendor/jszip.min.js".into(), JSZIP_JS.to_vec());
    assets.insert("vendor/epub.min.js".into(), EPUB_JS.to_vec());
    assets.insert("vendor/EPUBJS-LICENSE.txt".into(), EPUBJS_LICENSE.to_vec());
    assets.insert("vendor/JSZIP-LICENSE.txt".into(), JSZIP_LICENSE.to_vec());

    LincePackage::new_archive(
        Some("document-viewer.lince".into()),
        source.manifest.clone(),
        document(&source),
        "index.html",
        assets,
    )
    .expect("document viewer official sand should render as a valid archive package")
}

fn document(source: &SandWidgetSource) -> String {
    let markup: Markup = html! {
        (DOCTYPE)
        html lang=(source.lang) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (source.manifest.title.as_str()) }
                @for link in &source.head_links {
                    link rel=(link.rel) href=(link.href);
                }
                @for style_block in &source.inline_styles {
                    style { (maud::PreEscaped(style_block)) }
                }
            }
            body {
                (source.body)
                @for script in &source.body_scripts {
                    @match script {
                        WidgetScript::Src(src) => {
                            script src=(src) {}
                        }
                        WidgetScript::Inline(code) => {
                            script { (maud::PreEscaped(code.as_str())) }
                        }
                    }
                }
            }
        }
    };

    markup.into_string()
}

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "document-viewer.lince",
        lang: "en",
        manifest: PackageManifest {
            icon: "◧".into(),
            title: "Document Viewer".into(),
            author: "Lince Labs".into(),
            version: "0.3.0".into(),
            description: "Views PDF, EPUB, JPEG, and PNG documents from disk, a Lince bucket, or an internet URL.".into(),
            details: "Choose a local path, a Lince bucket object path, or an HTTP URL. The widget stores the selected source and path in card state and renders supported PDF, EPUB, JPEG, and PNG files.".into(),
            initial_width: 5,
            initial_height: 5,
            requires_server: false,
            permissions: vec!["bridge_state".into(), "read_files".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![
            WidgetScript::src("vendor/jszip.min.js"),
            WidgetScript::src("vendor/epub.min.js"),
            WidgetScript::inline(script::script()),
        ],
    }
}
