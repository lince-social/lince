use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.instinct";

// Instinct: the sand you read to learn Lince. Replaces the old flat "Tutorial"
// sand (shell::tutorial_source, removed) with a chaptered document — the intro
// chapter "First Steps" is the practical frontend walkthrough, and the rest go
// concept by concept in dependency order (Records -> Links -> Concepts ->
// Cells & Organs -> Transfers -> Karma).
//
// Diagrams come from `docs/Sand: First Steps.md`, whose typst `visual-text`
// blocks were already mermaid-shaped. They are NOT copy-pasted: that source
// uses single-dash `->`/`<-` edges and unquoted parentheses in labels, both of
// which mermaid rejects, so every graph was converted (`-->`, quoted labels).
//
// mermaid loads from the embedded `/board/vendor/mermaid.min.js` (LICENSE
// served beside it). Diagrams render lazily, on chapter activation: mermaid
// measures text to size nodes, so rendering one inside a `display:none`
// chapter produces a mis-sized graph. Each chapter renders once, then caches.
//
// The document is split so a chapter can be edited on its own: `instinct.html`
// is the shell (head, styles, nav, script) and each chapter is one fragment
// under `chapters/`, spliced in at `<!--CHAPTERS-->` below. Chapter ORDER is
// this array's order — the nav, the prev/next footer and the saved reading
// position are all derived from the DOM at runtime, so adding a chapter means
// adding a file and one line here, nothing else.
const SHELL: &str = include_str!("instinct.html");

const CHAPTERS_MARKER: &str = "<!--CHAPTERS-->";

const CHAPTERS: [&str; 7] = [
    include_str!("chapters/01-first-steps.html"),
    include_str!("chapters/02-records.html"),
    include_str!("chapters/03-links.html"),
    include_str!("chapters/04-concepts.html"),
    include_str!("chapters/05-cells-and-organs.html"),
    include_str!("chapters/06-transfers.html"),
    include_str!("chapters/07-karma.html"),
];

fn document() -> String {
    SHELL.replace(CHAPTERS_MARKER, &CHAPTERS.concat())
}

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◎".into(),
        title: "Instinct".into(),
        author: "Lince".into(),
        version: "1.0.0".into(),
        description: "Chaptered introduction to Lince, from the canvas to Karma.".into(),
        details:
            "Start at First Steps for the frontend itself — the corner triangle, edit mode, adding sands and pointing them at data with Protein — then read the chapters on Records, Links, Concepts, Cells & Organs, Transfers and Karma."
                .into(),
        initial_width: 6,
        initial_height: 5,
        requires_server: false,
        permissions: vec!["bridge_state".into()],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("instinct.html".into()), manifest(), document())
        .expect("instinct official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every chapter fragment has to actually land in the document. A typo in
    /// the marker, or a fragment dropped from `CHAPTERS`, otherwise ships a
    /// tutorial that is silently missing a chapter — the nav is built from the
    /// DOM, so a missing chapter looks intentional rather than broken.
    #[test]
    fn every_chapter_is_spliced_into_the_document() {
        let html = document();
        assert!(
            !html.contains(CHAPTERS_MARKER),
            "the chapters marker survived into the output: nothing was spliced"
        );
        for chapter in CHAPTERS {
            let name = chapter
                .split("data-chapter=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .expect("each chapter fragment declares data-chapter");
            assert!(
                html.contains(&format!("data-chapter=\"{name}\"")),
                "chapter {name} is missing from the assembled document"
            );
        }
    }

    /// The tutorial teaches users to click filters by name in the Data panel.
    /// Those names live in `protein-config.js`, so nothing stops the panel from
    /// being renamed and the tutorial from quietly going stale — which is
    /// exactly what happened when `linked to` became `Relation`. Pin the names
    /// the chapters spell out to the ones the panel actually offers.
    #[test]
    fn filter_names_the_tutorial_teaches_still_exist_in_the_data_panel() {
        const PROTEIN_CONFIG: &str =
            include_str!("../../../static/presentation/board/protein-config.js");
        let html = document();

        for label in ["Kind", "Concept", "Relation", "Assignee"] {
            assert!(
                PROTEIN_CONFIG.contains(&format!("label: \"{label}\"")),
                "the tutorial teaches the {label} filter, but protein-config.js no longer \
                 offers a filter with that label"
            );
            assert!(
                html.contains(label),
                "{label} is a Data-panel filter the tutorial should be teaching, \
                 but no chapter mentions it"
            );
        }

        // The pre-rename spellings. Matched only where the tutorial presents
        // them as a control name (`<strong>`/`<code>`), because "linked to" and
        // "concept in" are also ordinary English that legitimately appears in
        // prose and in comments — asserting on the bare phrase fails on
        // sentences like "records linked to many others".
        for stale in ["linked to", "concept in", "Assignee is"] {
            for markup in [format!("<strong>{stale}"), format!("<code>{stale}")] {
                assert!(
                    !html.contains(&markup),
                    "the tutorial presents {stale:?} as a Data-panel control, \
                     which is not what the panel calls it"
                );
            }
        }
    }
}
