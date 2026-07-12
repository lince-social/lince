use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.view_table_editor";

// Live records table as a self-contained HTML string over Protein + Actions.
const HTML: &str = include_str!("table.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "▦".into(),
        title: "Table".into(),
        author: "Lince Labs".into(),
        version: "1.0.0".into(),
        description: "Live records table over Protein reads and typed Actions.".into(),
        details:
            "Subscribes to a records Protein through the widget bridge, renders the table client-side, and creates/edits/deletes rows with typed Actions. No server view stream required."
                .into(),
        initial_width: 7,
        initial_height: 5,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("table.html".into()), manifest(), HTML)
        .expect("table official sand should render as a valid package")
}
