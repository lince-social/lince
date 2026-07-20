use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.relations";

// Record graph / relations sand as a self-contained HTML string over Protein +
// Actions: the retired d3 force graph (8f0a5df^ script.rs) ported to the new
// data plane — one live Protein (record source + links include), typed
// add-link/remove-link Actions, a board-wide `recordClicked`/`recordCreate`
// instead of the old sidepanels (caught by the pinned Record sand). d3 loads
// from the embedded `/static/vendored/d3.v7.min.js` (LICENSE served beside
// it). Ships as a single `relations.html` package.
const HTML: &str = include_str!("relations.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "⟠".into(),
        title: "Relations".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
        description: "Force-directed record graph over typed links.".into(),
        details:
            "d3 force graph for records and their Lingua links: live Protein reads, Shift+drag to link, edge ✕ to unlink, local physics. Node click opens the grouped Record; trail mode lands as a view over a root's tree."
                .into(),
        initial_width: 7,
        initial_height: 6,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("relations.html".into()), manifest(), HTML)
        .expect("relations official sand should render as a valid package")
}
