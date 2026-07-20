use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.kanban_record_view";

// The sand IS this HTML string (Stage 8b): a self-contained document that talks
// to the board host (`window.LinceWidgetHost`) over Protein reads + typed
// Actions. Emitted as a single `.html` package like the other sands.
const HTML: &str = include_str!("kanban.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "▥".into(),
        title: "Kanban".into(),
        author: "Lince Labs".into(),
        version: "2.0.0".into(),
        description: "Live kanban board over Protein reads and typed Actions.".into(),
        details:
            "Subscribes to a records Protein through the widget bridge, buckets records into columns, moves cards with typed Actions, and emits the recordClicked ABI event so a Record sand can show details."
                .into(),
        initial_width: 6,
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
    LincePackage::new(Some("kanban.html".into()), manifest(), HTML)
        .expect("kanban official sand should render as a valid package")
}
