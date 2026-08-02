use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.organ";

// Protein-first list of `kind=organ` records (this Cell + its contacts) with
// File Sync and a friends-list contact manager (trust/proximity/block) as
// first-class per-organ features. Standalone: no ABI events, just its own
// Protein subscription + `set-extension`/`set-contact-trust`/
// `set-contact-proximity` Actions.
const HTML: &str = include_str!("organ.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◈".into(),
        title: "Organ".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Lists this Cell's organs, manages contact trust/proximity, and \
            configures per-organ File Sync to disk."
            .into(),
        details: "Protein-first list of kind=organ records (the local Cell plus its \
            contacts, `contact` include for trust/proximity). Selecting a contact edits \
            its trust (unknown/known/blocked) and proximity via `set-contact-trust`/ \
            `set-contact-proximity` — a friends-list view, scoped to trust/proximity \
            only; sync policy and quarantine inspection are out of scope here. \
            Selecting any organ also shows and edits its `lince.file_sync` extension \
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
