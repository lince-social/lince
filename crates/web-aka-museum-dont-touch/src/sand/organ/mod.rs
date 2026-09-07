use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.organ";

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

    #[test]
    fn nothing_asks_through_a_browser_modal() {
        assert!(
            !HTML.contains("window.confirm(") && !HTML.contains("window.prompt("),
            "a sandboxed frame silently drops these, which reads as a dead button"
        );
        assert!(HTML.contains("function ask(anchor, options)"));
        assert!(HTML.contains("form.className = \"inline-ask\""));
    }

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
            "nb-info",
            "sync-info",
            "fs-info",
            "door-info",
            "stale-info",
        ] {
            assert!(HTML.contains(&format!("id=\"{id}\"")), "missing {id}");
        }
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

    #[test]
    fn hiding_and_unhiding_say_which_one_reaches_back() {
        assert!(HTML.contains("Anything they already received stays with them."));
        assert!(HTML.contains("including what changed while it was hidden"));
    }

    #[test]
    fn the_hide_list_distinguishes_empty_from_unloaded() {
        assert!(HTML.contains("Not loaded."));
        assert!(HTML.contains("Nothing hidden"));
        assert!(
            HTML.contains("if (!Array.isArray(hidden))"),
            "the two states are told apart by the shape of the value, not by falsiness"
        );
    }
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

    #[test]
    fn a_refused_payload_is_never_treated_as_markup() {
        assert!(
            !HTML.contains("innerHTML = item.payload") && !HTML.contains("innerHTML = item.reason"),
            "a peer's rejected op must not be able to write the panel"
        );
        assert!(HTML.contains("reason.title = item.payload"));
    }

    #[test]
    fn the_refusal_list_says_it_excludes_our_own_policy() {
        assert!(HTML.contains("are NOT here"));
    }
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

    #[test]
    fn an_unreadable_file_sync_filter_says_what_is_happening_instead() {
        assert!(HTML.contains("could not be read and is being ignored, so everything"));
        assert!(
            HTML.contains("That filter is not valid JSON."),
            "a NEW broken filter is refused rather than stored to be ignored later"
        );
    }

    #[test]
    fn a_filter_the_picker_cannot_express_is_not_flattened() {
        assert!(HTML.contains("JSON.stringify(parsed)"));
    }
}
