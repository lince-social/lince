use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.communication";

const HTML: &str = include_str!("communication.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "☏".into(),
        title: "Communication".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Messaging with audio/video rooms over Protein reads and typed Actions.".into(),
        details:
            "Lists conversations carrying a tag (default @communication), each a normal Record with threads. Emits recordClicked so a grouped Record sand shows the threads, and drops into a room mode to configure and join audio/video calls. Media arrives in later stages; this scaffold owns the list, room state, and view switching."
                .into(),
        initial_width: 4,
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
    LincePackage::new(Some("communication.html".into()), manifest(), HTML)
        .expect("communication official sand should render as a valid package")
}
