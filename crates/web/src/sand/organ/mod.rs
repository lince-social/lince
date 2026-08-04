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
        version: "0.4.0".into(),
        description: "Registers and manages organs, contact trust/proximity, and \
            per-organ File Sync to disk."
            .into(),
        details: "Protein-first list of kind=organ records: register a new organ with \
            its name and URL, edit or delete it, and manage the local Cell plus its \
            contacts, `contact` include for trust/proximity). Selecting a contact edits \
            its trust (unknown/known/blocked) and proximity via `set-contact-trust`/ \
            `set-contact-proximity` — a friends-list view, scoped to trust/proximity \
            only; sync policy and quarantine inspection are out of scope here. \
            Selecting any organ also shows and edits its `lince.file_sync` extension \
            (enabled, disk path) via `set-extension`. File Sync mirrors every record \
            whose `organ_uid` is that organ to/from markdown files (head = filename, \
            body = file content) — selection is hardcoded to organ origin for now, a \
            configurable Protein filter is future work. The LOCAL organ additionally \
            shows a Discovery panel (`lince.discovery`): LAN presence, internet \
            reachability, and whether unknown Organs may open a thread. Saving either \
            discovery reach setting rebinds the \
            iroh endpoint in the background — the NodeId is unchanged, so saved \
            contacts stay valid."
            .into(),
        initial_width: 4,
        initial_height: 5,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
            // Scanning a pairing code. The host owns the camera and hands back
            // only the decoded text — this sand never receives an image.
            "media_capture".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("organ.html".into()), manifest(), HTML)
        .expect("organ official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::{HTML, manifest};

    #[test]
    fn organ_sand_exposes_lynx_crud_with_a_separate_connection_indicator() {
        assert!(HTML.contains("/board/lynx-ui.css"));
        assert!(HTML.contains("/board/lynx-ui.js"));
        assert!(HTML.contains("action: \"create-record\", kind: \"organ\""));
        assert!(HTML.contains("id=\"organ-create\""));
        assert!(HTML.contains("createOrgan()"));
        assert!(HTML.contains("<section id=\"detail\">"));
        assert!(HTML.contains("id=\"selected-detail\" hidden"));
        assert!(HTML.contains("action: \"edit-record-text\""));
        assert!(HTML.contains("action: \"delete-record\""));
        assert!(HTML.contains("class=\"sand-tools\""));
        assert!(HTML.contains("id=\"connection-corner\""));
        assert!(HTML.contains("id=\"local-discovery-tool\""));
        assert!(HTML.contains("id=\"dc-local\""));
        assert!(
            manifest()
                .permissions
                .iter()
                .any(|permission| permission == "act")
        );
    }

    /// Scanning is declared as a permission and stops at filling the field.
    ///
    /// The second half is the part worth pinning: pointing a camera at a
    /// screen is a strong story about where a code came from, but it is still
    /// a story and not a verification, so the human still presses Add. A scan
    /// wired straight to `add-known-organ` would quietly erase that.
    #[test]
    fn scanning_a_code_needs_the_camera_permission_and_only_fills_the_field() {
        assert!(
            manifest()
                .permissions
                .iter()
                .any(|permission| permission == "media_capture"),
            "the host refuses `lince:scan-code` from a sand that does not declare it"
        );
        assert!(HTML.contains("H.scanCode()"));
        assert!(HTML.contains("id=\"ad-scan\""));
        assert!(
            !HTML.contains("action: \"add-known-organ\", invite: text"),
            "a scan must never add anyone by itself"
        );
    }
}
