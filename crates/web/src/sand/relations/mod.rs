use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.relations";

// Record graph / relations sand as a self-contained HTML string over Protein +
// Actions (the old d3 `graph_view` archive is retired).
const HTML: &str = include_str!("relations.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "⟠".into(),
        title: "Relations".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Record graph and ordered trail view over typed links.".into(),
        details:
            "Default graph sand for records and their Lingua links. Trail is a Relation mode over order-like links, not a separate package."
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
