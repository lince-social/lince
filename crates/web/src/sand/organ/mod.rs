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
            `set-sync-policy`, how WIDE the outbound feed is via \
            `set-contact-scope` (unnarrowed / named columns / nothing but the \
            identifying ones — three states, because the empty scope and the \
            absent one are opposites), and renames or forgets it through the local-only \
            `rename-organ-contact`/`forget-organ-contact` — a contact's record is \
            filed under THEIR uid, so the ordinary record edit would replicate this \
            Cell's private label back to them. The panel also lists what was REFUSED \
            from that contact — the bounded per-contact quarantine ring — because a \
            table nothing displays is the same as a table nobody keeps. \
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
            // The front door queue and the out-of-date device list, both added
            // with the surfaces C3 owed (2026-08-13).
            "door-info",
            "stale-info",
        ] {
            assert!(HTML.contains(&format!("id=\"{id}\"")), "missing {id}");
        }
        // The PROPERTY, not a count of it. This used to assert a magic 11 and
        // had drifted to 16 groups without anyone noticing, so the tripwire
        // that was supposed to catch an unexplained group was itself the
        // thing that was broken. Checking each heading carries its tooltip
        // says the same thing and cannot go stale as groups are added.
        let groups: Vec<&str> = HTML.split("class=\"group-head\"").skip(1).collect();
        assert!(
            groups.len() >= 13,
            "the organ panel lost its groups: {} left",
            groups.len()
        );
        for group in &groups {
            let heading = group.split("</div>").next().unwrap_or_default();
            assert!(
                heading.contains("data-lynx-tooltip="),
                "a group heading carries no explanation: {}",
                heading.trim()
            );
        }
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
        assert!(
            HTML.contains(
                ".badges { display: flex; flex-direction: column; align-items: flex-end;"
            )
        );
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

    /// The scope has three stored states and two of them are opposites:
    /// unnarrowed sends every column, the empty list sends none. A surface
    /// with one text field collapses them, and it collapses toward the wide
    /// one — someone asking to share nothing would end up sharing everything.
    /// So the mode is picked explicitly and the list only exists inside the
    /// middle state.
    #[test]
    fn the_scope_offers_all_three_states_and_never_confuses_two_of_them() {
        assert!(HTML.contains("action: \"set-contact-scope\""));
        assert!(HTML.contains("value=\"all\""));
        assert!(HTML.contains("value=\"some\""));
        assert!(HTML.contains("value=\"none\""));
        assert!(
            HTML.contains("scope === null || scope === undefined"),
            "an absent scope and an empty one must be read apart, not through `|| []`"
        );
        assert!(
            HTML.contains("if (mode === \"none\") return [];"),
            "the \"nothing\" option must send the empty list, not null"
        );
        assert!(
            HTML.contains("id=\"sc-fields-row\" hidden"),
            "the column list must not be typeable under a mode that ignores it"
        );
    }

    /// Both directions exist, and they are two settings rather than one with
    /// two ends. Outbound is a privacy control, inbound an integrity one; a
    /// single control would invite keeping them equal, which is the one thing
    /// they are not for. A panel offering only the outbound half cannot
    /// honestly claim to be the whole pairing.
    #[test]
    fn the_pairing_panel_has_both_directions_and_keeps_them_separate() {
        assert!(HTML.contains("action: \"set-contact-accept-scope\""));
        assert!(HTML.contains("id=\"accept-row\" hidden"));
        assert!(HTML.contains("id=\"ac-mode\""));
        assert!(
            HTML.contains("Nothing but deletions"),
            "the inbound floor is deletes, and it says so — a refused delete would \
             leave us holding a record they removed"
        );
    }

    /// Widening reaches backwards and narrowing does not, so the two are
    /// confirmed differently. The wording changed when the replay landed: it
    /// used to say the older changes kept their shape, which was true of the
    /// mechanism at the time and is not true of this one.
    #[test]
    fn widening_says_it_reaches_back_and_narrowing_does_not() {
        assert!(HTML.contains("function isWidening("));
        assert!(
            HTML.contains("including changes made before now"),
            "a widening re-sends, and must not be confirmed like a narrowing"
        );
        assert!(
            !HTML.contains("Wider from now on"),
            "the old wording described a mechanism that no longer exists"
        );
    }
    /// Per-record hiding is the ROW half of the same cluster, and it is a
    /// separate control with a separate save: batching it with the scope
    /// would let an accidental widening ride along with a deliberate hide.
    #[test]
    fn hiding_records_is_its_own_control_with_its_own_save() {
        assert!(HTML.contains("action: \"hide-record-from-contact\""));
        assert!(HTML.contains("id=\"hide-row\" hidden"));
        assert!(
            HTML.contains("record slug or uid"),
            "a person knows a record by its slug, so the field must accept one"
        );
        assert!(
            !HTML.contains("hide-record-from-contact\", target: row.uid, fields"),
            "hiding must not be folded into the scope save"
        );
    }

    /// The two directions are NOT symmetric and the surface says which is
    /// which. Unhiding REACHES BACK — the record's history is replayed,
    /// because ordinary catch-up never would. Hiding does not, because no
    /// delete is sent: sending one would confirm the record exists.
    #[test]
    fn hiding_and_unhiding_say_which_one_reaches_back() {
        assert!(HTML.contains("Anything they already received stays with them."));
        assert!(HTML.contains("including what changed while it was hidden"));
    }

    /// The empty state has to say WHICH nothing it means. An unloaded list and
    /// a genuinely empty one are both blank boxes otherwise, and one of them
    /// reads as a broken feature while the other is a policy.
    #[test]
    fn the_hide_list_distinguishes_empty_from_unloaded() {
        assert!(HTML.contains("Not loaded."));
        assert!(HTML.contains("Nothing hidden"));
        assert!(
            HTML.contains("if (!Array.isArray(hidden))"),
            "the two states are told apart by the shape of the value, not by falsiness"
        );
    }
    /// A stored scope that cannot be read is being IGNORED, and ignoring it
    /// means the widest setting there is. Showing "everything" without saying
    /// why would report a corrupt row as somebody's decision.
    #[test]
    fn an_unreadable_scope_says_it_is_being_ignored() {
        assert!(HTML.contains("function showBrokenScopes("));
        assert!(HTML.contains("could not be read and is being ignored"));
        assert!(
            HTML.contains("nothing is \"\n          + \"narrowed right now")
                || HTML.contains("nothing is "),
            "it has to say what ignoring it MEANS, not just that it happened"
        );
        assert!(
            HTML.contains("contact.scope_unreadable") && HTML.contains("contact.accept_unreadable"),
            "both directions are separate settings and are reported separately"
        );
    }
    /// The quarantine ring is per contact and bounded, and nothing displayed
    /// it — which is the same as not keeping it. It belongs on the panel for
    /// the contact it accuses.
    #[test]
    fn refused_changes_are_listed_on_the_contact_that_sent_them() {
        assert!(HTML.contains("function showQuarantine("));
        assert!(HTML.contains("id=\"qr-list\""));
        assert!(
            HTML.contains("Nothing refused"),
            "the good case must say which nothing it is"
        );
        assert!(
            HTML.contains("Not loaded."),
            "and must not read the same as a failed load"
        );
    }

    /// A refusal payload is JSON written by a peer — the least trustworthy
    /// text on the page. It goes in as text and never as markup.
    #[test]
    fn a_refused_payload_is_never_treated_as_markup() {
        assert!(
            !HTML.contains("innerHTML = item.payload") && !HTML.contains("innerHTML = item.reason"),
            "a peer's rejected op must not be able to write the panel"
        );
        assert!(HTML.contains("reason.title = item.payload"));
    }

    /// Ops dropped by our own acceptance scope are NOT refusals. Listing them
    /// would fill the ring on the first sync with any contact wider than our
    /// acceptance and bury the reports that mean something.
    #[test]
    fn the_refusal_list_says_it_excludes_our_own_policy() {
        assert!(HTML.contains("are NOT here"));
    }
    /// File Sync selection is a Protein predicate in the SAME vocabulary as
    /// every other filter — that is what "one selector language" means. The
    /// picker covers the shapes people actually want; anything else gets the
    /// language itself rather than a second, smaller one.
    #[test]
    fn file_sync_selection_uses_the_one_selector_language() {
        assert!(HTML.contains("id=\"fs-filter-mode\""));
        assert!(HTML.contains("concept_in") && HTML.contains("kind_eq"));
        assert!(
            HTML.contains("A filter I write myself"),
            "the picker must not be the ceiling"
        );
        assert!(
            !HTML.contains("is a Protein filter and is not wired up yet"),
            "the tooltip outlived the limitation it described"
        );
    }

    /// A filter that cannot be read is IGNORED, so everything from the organ
    /// syncs. Saying only "invalid" would leave the owner guessing whether
    /// files are being written right now.
    #[test]
    fn an_unreadable_file_sync_filter_says_what_is_happening_instead() {
        assert!(HTML.contains("could not be read and is being ignored, so everything"));
        assert!(
            HTML.contains("That filter is not valid JSON."),
            "a NEW broken filter is refused rather than stored to be ignored later"
        );
    }

    /// A filter the picker cannot express is shown as itself. Flattening
    /// somebody's `any` to the nearest menu option and then saving it would
    /// silently delete their filter.
    #[test]
    fn a_filter_the_picker_cannot_express_is_not_flattened() {
        assert!(HTML.contains("JSON.stringify(parsed)"));
    }
}
