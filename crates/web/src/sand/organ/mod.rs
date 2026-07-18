use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.organ";

// Protein-first list of `kind=organ` records (this Cell + its contacts) with
// File Sync as a first-class per-organ feature. Standalone: no ABI events,
// just its own Protein subscription + `set-extension` Actions.
const HTML: &str = include_str!("organ.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◈".into(),
        title: "Organ".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Lists this Cell's organs and configures per-organ File Sync to disk."
            .into(),
        details: "Protein-first list of kind=organ records (the local Cell plus its \
            contacts); selecting one shows and edits its `lince.file_sync` extension \
            (enabled, disk path) via `set-extension`. File Sync mirrors every record \
            whose `organ_uid` is that organ to/from markdown files (head = filename, \
            body = file content) — selection is hardcoded to organ origin for now, a \
            configurable Protein filter is future work."
            .into(),
        initial_width: 4,
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
    LincePackage::new(Some("organ.html".into()), manifest(), HTML)
        .expect("organ official sand should render as a valid package")
}
