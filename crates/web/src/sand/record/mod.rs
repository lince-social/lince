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

#[cfg(test)]
mod tests {
    use super::HTML;

    /// The card is a fixed viewport, so the panel needs its own scrolling
    /// region. `min-height: 0` is the load-bearing half: a flex item defaults
    /// to `min-height: auto`, refuses to shrink below its content, and the
    /// `overflow-y` never fires — which is exactly how this sand had scroll
    /// CSS and no scrollbar.
    #[test]
    fn the_panel_scrolls_inside_the_card() {
        assert!(HTML.contains("<div id=\"scroll\">"));
        assert!(HTML.contains("#scroll { flex: 1 1 auto; min-height: 0; overflow-y: auto;"));
        assert!(
            !HTML.contains(".rec { margin-bottom: 4px; overflow: hidden; }"),
            "the panel's own clip has to go, or the scroller has nothing to scroll"
        );
    }

    /// `#focus` needs `display: flex` for the section ordering below, and that
    /// outranks the UA `[hidden]` rule — the same author-origin tie the file's
    /// `#thread-new[hidden]` comment already documents.
    #[test]
    fn the_focus_panel_still_hides_when_idle() {
        assert!(HTML.contains("#focus { display: flex; flex-direction: column; }"));
        assert!(HTML.contains("#focus[hidden] { display: none; }"));
    }

    /// The head IS the record: a borderless title input, first, unlabelled.
    /// The body loses its label for the same reason — a textarea spanning the
    /// panel needs no one to name it.
    #[test]
    fn the_head_reads_as_a_title_and_the_body_needs_no_label() {
        assert!(HTML.contains("id=\"f-head\" class=\"titleinput\""));
        assert!(HTML.contains("id=\"c-head\" class=\"titleinput\""));
        assert!(HTML.contains(".titleinput { flex: 1; min-width: 0; border: 0;"));
        for label in [
            "<span class=\"k\">Head</span>",
            "<span class=\"k\">Body</span>",
        ] {
            assert!(!HTML.contains(label), "still labelled: {label}");
        }
    }

    /// Slug and quantity are one line, the schedule is one line, and the
    /// timer shares a line with the manual log entry it duplicates.
    #[test]
    fn related_fields_share_a_line() {
        assert_eq!(
            HTML.matches("class=\"rowset\"").count(),
            4,
            "slug+qty twice (create and focus), the estimate row, and the work-log bar"
        );
        assert!(HTML.contains(".rowset { display: flex; flex-wrap: wrap;"));
        for id in ["w-start", "w-due", "w-estimate"] {
            assert!(HTML.contains(&format!("id=\"{id}\"")));
        }
    }

    /// Work log and Estimate are separate sections because the five sections
    /// are ordered independently — a record that is only being timed must not
    /// drag an empty schedule to the top with it.
    #[test]
    fn what_the_record_has_sorts_above_what_it_does_not() {
        assert!(HTML.contains("id=\"sec-worklog\""));
        assert!(HTML.contains("id=\"sec-estimate\""));
        assert!(
            !HTML.contains("id=\"sec-work\""),
            "the combined section is gone, not shadowed"
        );
        assert!(HTML.contains(
            "const SECTIONS = [\"sec-worklog\", \"sec-estimate\", \"sec-people\", \"sec-links\", \"sec-comments\"];"
        ));
        assert!(HTML.contains("section.hidden = propertyFold === \"hidden\""));
        assert!(
            HTML.contains("if (sectionFilled[id] !== filled) {"),
            "and `open` is written only on a change, or an empty section a user just \
             opened is slammed shut by the next push"
        );
        assert!(
            HTML.contains("l.kind !== \"assigned-to\""),
            "assignees have their own section; counting them as links marks Links \
             filled for every assigned record"
        );
    }

    #[test]
    fn body_is_one_surface_with_raw_pragmatic_and_pretty_modes() {
        for id in ["body-raw", "body-pragmatic", "body-pretty"] {
            assert!(HTML.contains(&format!("id=\"{id}\"")));
        }
        assert!(HTML.contains("function rawBlockForLine("));
        assert!(HTML.contains("}, 5000);"));
        assert!(HTML.contains("pragmatic-source"));
    }

    #[test]
    fn properties_fold_between_filled_all_and_hidden() {
        assert!(HTML.contains("id=\"property-fold\""));
        assert!(HTML.contains("id=\"properties-more\""));
        assert!(HTML.contains("id=\"properties-hide\""));
        assert!(HTML.contains("let propertyFold = \"filled\""));
        assert!(HTML.contains("$(\"sec-facts\").open = false"));
    }
    /// A reference is a POINTER read live, not a copy, and the interface has
    /// to say so — the whole property it buys is that the owner can still take
    /// it back, and a panel indistinguishable from held data would hide that.
    #[test]
    fn a_live_reference_is_read_on_demand_and_never_cached() {
        assert!(HTML.contains("function liveReference("));
        assert!(HTML.contains("/organ/reference/read"));
        assert!(
            HTML.contains("Read live from their cell just now"),
            "what came back must be labelled with when it was read"
        );
    }

    /// Unreachable and refused mean OPPOSITE things to the reader and are
    /// never collapsed. Showing "no longer shared" to somebody whose friend
    /// closed their laptop is a false accusation.
    #[test]
    fn a_failed_reference_read_says_which_failure_it_was() {
        assert!(HTML.contains("response.status === 403"));
        assert!(HTML.contains("not sharing this with you any more"));
        assert!(
            HTML.contains("Could not reach their cell right now"),
            "offline is temporary and must read as temporary"
        );
        assert!(
            HTML.contains("there is nothing stored to show"),
            "and must say WHY there is no fallback: a cached copy would be the \
             thing that makes revocation stop working"
        );
    }
    /// Reading a reference is observable by its owner whether or not anyone
    /// records it. Both sides are told: the reader BEFORE the read, because
    /// after is too late to be a choice.
    #[test]
    fn both_sides_are_told_that_a_reference_read_is_observable() {
        assert!(
            HTML.contains("they can see that you opened it and when"),
            "the reader is warned before pressing, not after"
        );
        assert!(HTML.contains("function renderReferenceReads("));
        assert!(HTML.contains("reference_reads: true"));
    }

    /// Most records are never referenced. A permanent empty "opened by" panel
    /// on every record would train people to ignore the one that eventually
    /// says something.
    #[test]
    fn the_opened_by_panel_is_absent_rather_than_empty() {
        assert!(HTML.contains("$(\"sec-reads\").hidden = reads.length === 0"));
        assert!(
            HTML.contains("item.reader_name || item.reader_organ"),
            "a reader is named, or identified by uid — never \"someone\", which \
             would imply we do not know"
        );
    }
    /// A copy and a reference are separate decisions and get separate
    /// controls. One control with a mode would let the irreversible act be
    /// reached by the same gesture as the reversible one.
    #[test]
    fn sending_a_copy_is_its_own_control_not_a_mode_of_posting() {
        assert!(HTML.contains("action: \"send-record-copy\""));
        assert!(HTML.contains("id=\"cm-copy\""));
        assert!(
            !HTML.contains("prompt("),
            "this sand asks in the page, never through a browser modal"
        );
    }

    /// The warning appears AT the moment of copying, names the difference
    /// rather than asking "are you sure", and states the safer alternative so
    /// it is visible at the point of choosing.
    #[test]
    fn the_copy_warning_names_the_difference_and_the_alternative() {
        assert!(HTML.contains("You cannot take it back"));
        assert!(
            HTML.contains("later edits here will not reach it"),
            "a copy is a moment, not a window, and the wording must say so"
        );
        assert!(
            HTML.contains("To point at a record instead"),
            "the reversible option belongs in front of the irreversible one"
        );
    }
    /// Key exchange IS the promotion step and it happens inside the thread:
    /// you talk to someone first, then decide they are someone you know.
    /// Both halves live in the conversation — sending the code and acting on
    /// one that arrives.
    #[test]
    fn promotion_happens_inside_the_conversation_in_both_directions() {
        assert!(HTML.contains("action: \"share-my-key\""));
        assert!(HTML.contains("function pairingOffer("));
        assert!(HTML.contains("action: \"add-known-organ\""));
    }

    /// A sender's claimed label is a string they chose and is never identity.
    /// The local user types what THEY call this person; prefilling the field
    /// from the message would quietly turn an untrusted label into a name.
    #[test]
    fn a_pairing_offer_never_names_the_sender_for_you() {
        assert!(HTML.contains("What you call them"));
        assert!(
            HTML.contains("a name is not something they send you"),
            "the refusal must say WHY, or it reads as a validation quirk"
        );
        assert!(
            !HTML.contains("name.value = m.organ_name") && !HTML.contains("name.value = code"),
            "the name field must never be prefilled from anything they sent"
        );
    }

    /// Adding someone widens more than the conversation did, and the control
    /// says so before it is pressed.
    #[test]
    fn adding_a_contact_says_what_it_opens() {
        assert!(HTML.contains("opens your ordinary feed to them, which a conversation"));
    }
}
