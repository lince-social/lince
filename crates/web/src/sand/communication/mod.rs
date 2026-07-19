use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.communication";

// The Communication sand IS this HTML string (Stage 8b pattern): a
// self-contained document that talks to the board host
// (`window.LinceWidgetHost`) over Protein reads + typed Actions. It lists
// conversations carrying a tag (`@communication` by default), drives a Record
// sand via the `recordClicked` ABI event, and drops into a per-conversation
// room mode (call view) for audio/video. Ships as a single `.html` package and,
// by default, as a GROUP beside a Record sand (see `build_communication_group_archive`).
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
