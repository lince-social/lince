use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.example_bundle";

// The reference DIRECTORY-BUNDLE sand (Stage 8b, base task 3): a folder of files
// (`index.html` entry + sibling `script.js`/`style.css` assets) shipped as one
// `.lince` archive, as opposed to the single-`.html` sands. It is the format
// template for multi-file sands. The files are embedded here and rebuilt into
// `example-bundle.lince` at boot, so the catalog lists it and the board serves
// its assets like any archive package.
const INDEX_HTML: &str = include_str!("index.html");
const SCRIPT_JS: &str = include_str!("script.js");
const STYLE_CSS: &str = include_str!("style.css");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "📦".into(),
        title: "Example Bundle".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Reference directory-bundle sand: a folder of files shipped as one .lince."
            .into(),
        details:
            "Format template for multi-file sands. `index.html` is the entry; `script.js` and `style.css` are sibling assets in the same .lince archive, served relative to the entry."
                .into(),
        initial_width: 4,
        initial_height: 3,
        requires_server: false,
        permissions: Vec::new(),
    }
}

pub(crate) fn package() -> LincePackage {
    let mut assets = BTreeMap::new();
    assets.insert("style.css".to_string(), STYLE_CSS.as_bytes().to_vec());
    assets.insert("script.js".to_string(), SCRIPT_JS.as_bytes().to_vec());
    LincePackage::new_archive(
        Some("example-bundle.lince".into()),
        manifest(),
        INDEX_HTML,
        "index.html",
        assets,
    )
    .expect("example_bundle official sand should render as a valid archive package")
}
