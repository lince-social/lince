use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.record";

// Protein-first record detail sand as a self-contained HTML string. Listens for
// the recordClicked ABI event and drives itself over Protein through the board
// host. Emitted as a single `.html` package.
const HTML: &str = include_str!("record.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◉".into(),
        title: "Record".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Detail sand for the last record clicked in any sand.".into(),
        details:
            "Reference consumer of the recordClicked ABI event: sits as a dot and expands into a detail panel driven by a Protein subscription when it receives the event."
                .into(),
        initial_width: 2,
        initial_height: 3,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("record.html".into()), manifest(), HTML)
        .expect("record official sand should render as a valid package")
}
