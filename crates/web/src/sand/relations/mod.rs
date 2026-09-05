use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.relations";

const HTML: &str = include_str!("relations.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "⟠".into(),
        title: "Relations".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
        description: "Protein-ordered record trails and a force-directed typed-link graph.".into(),
        details:
            "d3 force graph for records and their Lingua links: live Protein reads, Shift+drag to link, edge ✕ to unlink, local physics. Protein-directed link orders define ordered trails and quantity-sign focus."
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
