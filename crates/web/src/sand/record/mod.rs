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
        for label in ["<span class=\"k\">Head</span>", "<span class=\"k\">Body</span>"] {
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
        assert!(
            HTML.contains("section.style.order = String((filled ? 10 : 50) + at);"),
            "order is a style, never an appendChild — a protein push must not move the scroll"
        );
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
}
