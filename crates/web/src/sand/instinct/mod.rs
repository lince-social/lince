use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.instinct";

// Instinct: the sand you read to learn Lince. Replaces the old flat "Tutorial"
// sand (shell::tutorial_source, removed) with one tree rooted at "First Steps"
// — the practical frontend walkthrough — which branches into why Lince exists,
// then the model underneath it, then Links, Concepts, Cells & Organs,
// Transfers and Karma.
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
// **The chapters are RECORDS now** (2026-08-16). `chapters/*.html` is deleted;
// the source is `docs/records/*.lingua`, embedded once by `engine::instinct`
// and read by both this sand and `Action::ImportInstinct`, so what a reader
// sees and what the import button would put in their store cannot drift apart.
// `instinct.html` is still the shell (head, styles, nav, script) and the
// generated chapters are spliced in at `<!--CHAPTERS-->`. Chapter ORDER comes
// from the assertions, not from an array here — the nav, the prev/next footer
// and the saved reading position are all derived from the DOM at runtime, so
// adding a chapter means adding a file to `docs/records/` and nothing else.
mod render;

const SHELL: &str = include_str!("instinct.html");

const CHAPTERS_MARKER: &str = "<!--CHAPTERS-->";

/// The chapters, built from the RECORDS rather than from seven HTML files.
///
/// The files are gone (2026-08-16). `docs/records/*.lingua` is the source, and
/// `engine::instinct` is the one embedded copy that both this sand and
/// `Action::ImportInstinct` read — so the chapter you are reading and the
/// Record the button would put in your store cannot drift apart.
///
/// **The Records are one tree now.** Everything hangs off `First Steps` by
/// `@part-of [[Idea|uid]] n`, at any depth, and the number on that link is the
/// order among siblings. So reading order is a depth-first walk that the
/// engine has already done — this only has to decide where one entry in the
/// navigation ends and the next begins, which is what `is_entry` answers.
///
/// The shell script is untouched by this: it builds the nav, the prev/next
/// footer and the saved reading position from `.chapter` elements in the DOM,
/// so it neither knows nor cares that they are now generated.
fn chapters() -> String {
    let records = engine::instinct::records();
    let mut out = String::new();
    for entry in records.iter().filter(|r| r.is_entry()) {
        // Depth travels with the entry so the navigation can show the TREE.
        // Without it the nav is a flat list of every chapter at every level,
        // which reads as thirty-odd peers and hides the one structure the
        // Records exist to carry.
        out.push_str(&format!(
            "<article class=\"chapter\" data-chapter=\"{}\" data-depth=\"{}\">\n",
            entry.head.replace('"', "&quot;"),
            entry.path(&records).len().saturating_sub(1)
        ));
        // The records already arrive depth-first, so everything that reads
        // under this entry is simply everything that names it — in order,
        // however deep it sits. The entry's own body comes first because a
        // Record's path is a prefix of its children's.
        for record in records.iter().filter(|r| r.entry_uid(&records) == entry.projection.uid) {
            out.push_str(&render::body_to_html(&record.body));
        }
        out.push_str("</article>\n");
    }
    out
}

fn document() -> String {
    SHELL.replace(CHAPTERS_MARKER, &chapters())
}

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◎".into(),
        title: "Instinct".into(),
        author: "Lince".into(),
        version: "1.0.0".into(),
        description: "Chaptered introduction to Lince, from the canvas to Karma.".into(),
        details:
            "Start at First Steps for the frontend itself — the corner triangle, edit mode, adding sands and pointing them at data with Protein — then follow the tree out through why Lince exists, the model underneath it, Links, Concepts, Cells & Organs, Transfers and Karma."
                .into(),
        initial_width: 6,
        initial_height: 5,
        requires_server: false,
        permissions: vec!["bridge_state".into(), "act".into()],
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
        let records = engine::instinct::records();
        let chapters: Vec<_> = records.iter().filter(|r| r.is_entry()).collect();
        assert!(chapters.len() >= 7, "the root and its branches: {}", chapters.len());
        assert_eq!(chapters[0].head, "First Steps", "the reader opens on the root");
        for chapter in &chapters {
            assert!(
                html.contains(&format!("data-chapter=\"{}\"", chapter.head)),
                "chapter {} is missing from the assembled document",
                chapter.head
            );
        }
        // And every IDEA landed in a chapter too. The nav is built from
        // `.chapter` elements, so an idea whose chapter link went nowhere
        // would vanish from the document without the nav looking wrong.
        for idea in records.iter().filter(|r| !r.is_entry()) {
            let first = idea.body.lines().find(|l| !l.trim().is_empty()).unwrap_or_default();
            let probe: String = first.trim_start_matches(['#', '>', '-', ' ']).chars().take(24).collect();
            if probe.len() < 12 || probe.contains(['*', '_', '`', '<', '&', '[']) {
                continue; // inline markup is rewritten; those are covered elsewhere
            }
            assert!(
                html.contains(&probe),
                "{} is in the bundle but not in the document",
                idea.head
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
