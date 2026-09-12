use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.instinct";

mod render;

const SHELL: &str = include_str!("instinct.html");

const CHAPTERS_MARKER: &str = "<!--CHAPTERS-->";

fn chapters() -> String {
    let records = engine::instinct::records();
    let mut out = String::new();
    for entry in records.iter().filter(|r| r.is_entry()) {
        out.push_str(&format!(
            "<article class=\"chapter\" data-chapter=\"{}\" data-depth=\"{}\">\n",
            entry.head.replace('"', "&quot;"),
            entry.path(&records).len().saturating_sub(1)
        ));
        for record in records
            .iter()
            .filter(|r| r.entry_uid(&records) == entry.projection.uid)
        {
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

    #[test]
    fn every_chapter_is_spliced_into_the_document() {
        let html = document();
        assert!(
            !html.contains(CHAPTERS_MARKER),
            "the chapters marker survived into the output: nothing was spliced"
        );
        let records = engine::instinct::records();
        let chapters: Vec<_> = records.iter().filter(|r| r.is_entry()).collect();
        assert!(
            chapters.len() >= 7,
            "the root and its branches: {}",
            chapters.len()
        );
        assert_eq!(
            chapters[0].head, "First Steps",
            "the reader opens on the root"
        );
        for chapter in &chapters {
            assert!(
                html.contains(&format!("data-chapter=\"{}\"", chapter.head)),
                "chapter {} is missing from the assembled document",
                chapter.head
            );
        }
        for idea in records.iter().filter(|r| !r.is_entry()) {
            let first = idea
                .body
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or_default();
            let probe: String = first
                .trim_start_matches(['#', '>', '-', ' '])
                .chars()
                .take(24)
                .collect();
            if probe.len() < 12 || probe.contains(['*', '_', '`', '<', '&', '[']) {
                continue;
            }
            assert!(
                html.contains(&probe),
                "{} is in the bundle but not in the document",
                idea.head
            );
        }
    }

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
