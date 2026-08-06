use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.organ";

// Protein-first list of `kind=organ` records (this Cell + its contacts) with
// File Sync and a friends-list contact manager (trust/proximity/block) as
// first-class per-organ features. Nearby Cells can either be deliberately
// promoted to known or receive a one-conversation offer without promotion.
const HTML: &str = include_str!("organ.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◈".into(),
        title: "Organ".into(),
        author: "Lince Labs".into(),
        version: "0.4.0".into(),
        description: "Adds and manages organs, contact trust/proximity, feed \
            direction, and per-organ File Sync to disk."
            .into(),
        details: "Protein-first list of kind=organ records: this Cell plus its \
            contacts. An Organ is added ONLY from a pairing code (`add-known-organ`, \
            pasted or scanned) — registering one by hostname was removed because \
            nothing in the transport dials a URL: pairing parses a NodeId, the outbox \
            dials a NodeId, and inbound authorises by NodeId. Selecting a contact \
            edits its trust (unknown/known/blocked) and proximity via \
            `set-contact-trust`/`set-contact-proximity`, its feed direction via \
            `set-sync-policy`, and renames or forgets it through the local-only \
            `rename-organ-contact`/`forget-organ-contact` — a contact's record is \
            filed under THEIR uid, so the ordinary record edit would replicate this \
            Cell's private label back to them. Quarantine inspection is out of scope \
            here. \
            Selecting any organ also shows and edits its `lince.file_sync` extension \
            (enabled, disk path) via `set-extension`. File Sync mirrors every record \
            whose `organ_uid` is that organ to/from markdown files (head = filename, \
            body = file content) — selection is hardcoded to organ origin for now, a \
            configurable Protein filter is future work. The LOCAL organ additionally \
            shows a Discovery panel (`lince.discovery`): LAN presence, internet \
            reachability, and whether unknown Organs may open a thread. Saving either \
            discovery reach setting rebinds the \
            iroh endpoint in the background — the NodeId is unchanged, so saved \
            contacts stay valid. Nearby `Chat` uses that authenticated NodeId to \
            offer one individual-replica conversation while both sides remain \
            unknown; `Add known` is the separate trust-promoting action."
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
        assert!(HTML.contains("<section id=\"detail\">"));
        assert!(HTML.contains("id=\"selected-detail\" hidden"));
        assert!(HTML.contains("action: \"edit-record-text\""));
        assert!(HTML.contains("action: \"delete-record\""));
        assert!(HTML.contains("class=\"sand-tools\""));
        assert!(HTML.contains("id=\"connection-corner\""));
        assert!(HTML.contains("id=\"local-discovery-tool\""));
        assert!(HTML.contains("id=\"dc-local\""));
        assert!(HTML.contains("/organ/conversation/offer"));
        assert!(HTML.contains("Add known"));
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

    /// Every Cell ships calling itself "Local Lince", so a contact row and
    /// this Cell's own row carry the same name and the same-looking loopback
    /// URL. Without something in the row itself saying which is which, the
    /// list looks like it is showing the same organ twice.
    #[test]
    fn the_list_says_which_row_is_this_cell() {
        assert!(
            HTML.contains("\"this cell\""),
            "this Cell's own row must be badged as such"
        );
        assert!(
            HTML.contains("fingerprintOf(o.contact.node_id)"),
            "contacts are told apart by the short code from their key, not by name"
        );
        assert!(
            HTML.contains("— this Cell"),
            "and the detail header must say it too, since that is what is read next"
        );
    }

    /// A contact's Organ record is filed under THEIR uid. `edit-record-text`
    /// logs a CRDT op and `delete-record` a tombstone — both replicate, so
    /// renaming a contact the ordinary way would publish the private label
    /// this Cell chose for them.
    #[test]
    fn renaming_a_contact_never_goes_through_the_logged_record_edit() {
        assert!(
            HTML.contains("action: \"rename-organ-contact\""),
            "a contact rename must use the local-only action"
        );
        assert!(
            HTML.contains("action: \"forget-organ-contact\""),
            "and dropping a contact must forget them locally, not tombstone their record"
        );
    }

    /// There is exactly ONE string a user has to think about sending: the
    /// pairing code. The identity key lives inside it and is refused by
    /// `add-known-organ` on its own, so presenting it as a second sendable
    /// thing was the whole confusion — it stays, as a fingerprint to read
    /// aloud and compare.
    #[test]
    fn one_string_is_for_sending_and_the_key_is_only_for_comparing() {
        assert!(
            HTML.contains("This Organ's pairing code"),
            "the sendable one is named for the Organ it belongs to"
        );
        assert!(
            HTML.contains("Verify by fingerprint"),
            "and the key appears as a comparison, not as an alternative to send"
        );
        assert!(
            !HTML.contains("Your published key"),
            "presenting the key as a second thing to hand over is what confused people"
        );
        assert!(
            HTML.contains("lince1|"),
            "the code's prefix is named, so the right string is recognisable on sight"
        );
    }

    /// The sand's iframe is sandboxed WITHOUT `allow-modals`, so the browser
    /// ignores `confirm()`/`prompt()`: confirm returns false and the action
    /// never runs. That is why Delete appeared to do nothing. Every ask is
    /// inline instead.
    #[test]
    fn nothing_asks_through_a_browser_modal() {
        assert!(
            !HTML.contains("window.confirm(") && !HTML.contains("window.prompt("),
            "a sandboxed frame silently drops these, which reads as a dead button"
        );
        assert!(HTML.contains("function ask(anchor, options)"));
        assert!(HTML.contains("form.className = \"inline-ask\""));
    }

    /// Adding an Organ and managing this identity's devices are errands, not
    /// properties of whichever row is selected. Registering used to sit on top
    /// of every organ you opened, and the device list appeared under contacts
    /// where it means nothing.
    /// A pairing code is the ONLY way to add an Organ. Registering one by
    /// hostname was removed 2026-08-05: nothing in the transport can dial a
    /// URL — pairing parses a NodeId, the outbox dials a NodeId, and inbound
    /// authorises by `contact_by_node_id` — so the form could only ever make
    /// a row that looked reachable and was not.
    #[test]
    fn an_organ_is_added_by_pairing_code_and_by_nothing_else() {
        assert!(
            !HTML.contains("id=\"organ-form\"") && !HTML.contains("id=\"organ-url\""),
            "registering by hostname promised reachability the transport cannot deliver"
        );
        assert!(
            !HTML.contains("action: \"create-record\", kind: \"organ\""),
            "and with the form gone, nothing here mints a bare organ record"
        );
        assert!(HTML.contains("action: \"add-known-organ\""));
    }

    #[test]
    fn registering_and_devices_are_modes_reached_from_the_corner_tools() {
        assert!(HTML.contains("id=\"register-open\""));
        assert!(HTML.contains("id=\"devices-open\""));
        assert!(HTML.contains("LynxUI.icon(\"plus\")"));
        assert!(HTML.contains("id=\"register-mode\" hidden"));
        assert!(HTML.contains("id=\"devices-mode\" hidden"));
        assert!(
            HTML.contains("id=\"root-key-panel\""),
            "the root key belongs with the devices it signs for, not with an Organ"
        );
    }

    /// Each group of related properties carries its explanation on the
    /// heading, in a tooltip, rather than as prose under the controls: a hint
    /// below is read after the mistake, a heading before it.
    #[test]
    fn every_group_explains_itself_through_a_heading_tooltip() {
        for id in [
            "add-info",
            "devices-info",
            "root-key-info",
            "o-info",
            "c-info",
            "pf-info",
            "pf-key-info",
            "dc-info",
            // On the network list's summary, which is a heading of its own —
            // it just lives in the side column rather than in a panel.
            "nb-info",
            "sync-info",
            "fs-info",
        ] {
            assert!(HTML.contains(&format!("id=\"{id}\"")), "missing {id}");
        }
        assert_eq!(
            HTML.matches("class=\"group-head\"").count(),
            9,
            "every group is titled, and the titles are where the explanations live"
        );
        assert!(
            HTML.contains("main [data-lynx-tooltip]::after"),
            "the shared 180px cap is widened HERE, never in the shared stylesheet"
        );
    }

    /// Who is on the network is not a property of the Organ you happen to
    /// have selected — it is the other half of the same column. It shares the
    /// side panel through a split whose divider the user drags, and it closes
    /// down to its summary rather than holding a third of the column open
    /// around nothing.
    #[test]
    fn the_network_list_shares_the_side_column_through_a_split() {
        assert!(HTML.contains("class=\"lynx-split\" id=\"side-split\""));
        assert!(HTML.contains("class=\"lynx-split__divider\""));
        assert!(
            HTML.contains("id=\"nearby-disclosure\" open"),
            "it starts open — a closed list of nobody teaches nothing"
        );
        assert!(
            HTML.contains("id=\"nb-count\""),
            "and the count rides the summary, so a closed section still says whether anyone is there"
        );
        assert!(
            !HTML.contains("id=\"nearby-panel\""),
            "the old copy inside the Organ detail is gone, not duplicated"
        );
    }

    /// A row can carry four status labels at once, and side by side they leave
    /// the name nowhere to go in a column this narrow.
    #[test]
    fn a_rows_status_labels_stack() {
        assert!(HTML.contains(
            ".badges { display: flex; flex-direction: column; align-items: flex-end;"
        ));
        assert!(
            !HTML.contains("li.append(badge);"),
            "every label goes in the stack, not straight onto the row"
        );
    }

    /// Offering a conversation to someone you already have one with mints a
    /// second one beside it. The panel asks Protein whether one exists —
    /// a grant is neither a link nor a Fact, so nothing else can answer.
    #[test]
    fn an_existing_conversation_is_opened_rather_than_offered_again() {
        assert!(HTML.contains("conversations: true"));
        assert!(HTML.contains("function conversationWith(row)"));
        assert!(HTML.contains("talking ? \"Open conversation\" : \"Start a conversation\""));
        assert!(
            HTML.contains("H.emit(\"recordClicked\", { record: { uid: existing.uid } });"),
            "opening means handing the conversation to whoever reads records"
        );
    }

    /// Sync is one section with both axes: the per-contact feed direction,
    /// which the engine already enforces, and file mirroring to local disk.
    #[test]
    fn synchronisation_shows_direction_next_to_file_sync() {
        assert!(HTML.contains("action: \"set-sync-policy\""));
        assert!(HTML.contains("id=\"sy-out\""));
        assert!(HTML.contains("id=\"sy-in\""));
        assert!(
            HTML.contains("id=\"sync-direction-row\" hidden"),
            "direction is a property of a feed, so it needs a peer at the other end"
        );
        assert!(HTML.contains("id=\"fs-enabled\""));
        assert!(HTML.contains("id=\"fs-path\""));
    }
}
