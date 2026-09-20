use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.bucket_image_view";

const HTML: &str = include_str!("index.html");
const STYLES: &[u8] = include_bytes!("styles.css");
const APP: &[u8] = include_bytes!("app/main.js");
const STATE: &[u8] = include_bytes!("app/state.js");
const SOURCES: &[u8] = include_bytes!("app/sources.js");
const EPUB_JS: &[u8] = include_bytes!("vendor/epub.min.js");
const JSZIP_JS: &[u8] = include_bytes!("vendor/jszip.min.js");
const EPUBJS_LICENSE: &[u8] = include_bytes!("vendor/EPUBJS-LICENSE.txt");
const JSZIP_LICENSE: &[u8] = include_bytes!("vendor/JSZIP-LICENSE.txt");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◧".into(),
        title: "Document Viewer".into(),
        author: "Lince Labs".into(),
        version: "1.0.0".into(),
        description: "Views PDF, EPUB, and common image documents without a data-plane subscription."
            .into(),
        details: "Opens a browser-picked file, authenticated host media, or an HTTP(S) URL. Selection and reading position stay in per-card widget state; the Ledger is not involved."
            .into(),
        initial_width: 5,
        initial_height: 5,
        requires_server: false,
        permissions: vec!["bridge_state".into()],
    }
}

pub(crate) fn package() -> LincePackage {
    let assets = BTreeMap::from([
        ("styles.css".into(), STYLES.to_vec()),
        ("app/main.js".into(), APP.to_vec()),
        ("app/state.js".into(), STATE.to_vec()),
        ("app/sources.js".into(), SOURCES.to_vec()),
        ("vendor/jszip.min.js".into(), JSZIP_JS.to_vec()),
        ("vendor/epub.min.js".into(), EPUB_JS.to_vec()),
        ("vendor/EPUBJS-LICENSE.txt".into(), EPUBJS_LICENSE.to_vec()),
        ("vendor/JSZIP-LICENSE.txt".into(), JSZIP_LICENSE.to_vec()),
    ]);

    LincePackage::new_archive(
        Some("document-viewer.lince".into()),
        manifest(),
        HTML,
        "index.html",
        assets,
    )
    .expect("document viewer official sand should render as a valid archive package")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_uses_frame_bridge_and_carries_reader_notices() {
        let package = package();
        assert!(package.html_document().contains("/board/frame.js"));
        assert!(package.asset_bytes("app/main.js").is_some());
        assert!(package.asset_bytes("app/state.js").is_some());
        assert!(package.asset_bytes("app/sources.js").is_some());
        assert!(package.asset_bytes("vendor/EPUBJS-LICENSE.txt").is_some());
        assert!(package.asset_bytes("vendor/JSZIP-LICENSE.txt").is_some());
        assert!(!package.html_document().contains("widget-frame-bootstrap"));
        assert!(!package.html_document().contains("host/integrations"));
    }
}
