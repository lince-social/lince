use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.view_todo_editor";

// Standalone todo focus queue as a self-contained HTML string over Protein +
// Actions.
const HTML: &str = include_str!("todo.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "▦".into(),
        title: "Todo".into(),
        author: "Lince Labs".into(),
        version: "1.0.0".into(),
        description: "Standalone todo focus queue driven by Protein and Actions.".into(),
        details:
            "Subscribes to the card's selected Protein, defaulting to the focus queue, and completes/undoes tasks with typed Actions through the widget bridge."
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
    LincePackage::new(Some("todo.html".into()), manifest(), HTML)
        .expect("todo official sand should render as a valid package")
}
