use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.ontology";

const HTML: &str = include_str!("ontology.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "@".into(),
        title: "Ontology".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Manage Linguas, Concepts, hierarchy, and Record assertions.".into(),
        details: "The common semantic workbench: Linguas group shared meanings, Concepts widen through a many-parent hierarchy, and assertions appear as tags or relationships depending on whether they name an object Record.".into(),
        initial_width: 6,
        initial_height: 7,
        requires_server: false,
        permissions: vec!["protein_subscribe".into(), "act".into()],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("ontology.html".into()), manifest(), HTML)
        .expect("ontology official sand should render as a valid package")
}
