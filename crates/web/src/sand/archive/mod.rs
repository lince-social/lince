use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.view_archive_exporter";

// The Archive sand is only a button: the actual capture runs in the board
// chrome (`/static/presentation/board/archive.js`), because a sand iframe
// cannot read its sibling iframes' documents. See
// notes/institute/Playground.md for the export's no-network guarantees.
const HTML: &str = include_str!("archive.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "🗃".into(),
        title: "Archive".into(),
        author: "Lince Labs".into(),
        version: "1.0.0".into(),
        description: "Exports the current workspace as one static, request-free HTML file.".into(),
        details: "Asks the board chrome to capture every card's rendered DOM (this card \
            excluded), strip scripts, inline styles and images, and download a single \
            sandboxed HTML page sized to the cards' bounding rectangle."
            .into(),
        initial_width: 4,
        initial_height: 4,
        requires_server: false,
        permissions: vec!["bridge_state".into()],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("archive.html".into()), manifest(), HTML)
        .expect("archive official sand should render as a valid package")
}
