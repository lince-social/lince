//! Lince's own documentation, as a bundle of Records.
//!
//! The Instinct sand used to be seven HTML pages. It is now a reader over
//! these Records, and the same bundle can be imported into a store — which is
//! the point: a chapter about Karma can put real Karma Records in front of you
//! instead of describing them.
//!
//! **The bundle is embedded, and `docs/records/` is where it is authored.**
//! Both readers use this one copy (see `build.rs`), so the sand can never show
//! a chapter the import would not create.

include!(concat!(env!("OUT_DIR"), "/bundle.rs"));

use crate::lingua_file::{Projection, parse_file};

/// The Concepts every file in the bundle names.
///
/// A file must never invent a meaning, so importing has to ensure these first
/// or every record in the bundle is refused. Kept as a list rather than
/// discovered from the files on purpose: the vocabulary is a decision, and a
/// bundle that could introduce arbitrary Concepts by being imported would be a
/// way around the rule rather than an application of it.
/// Two families share it: `chapter`/`idea`/`see-also` is the tutorial, and
/// `document`/`section`/`task`/`part-of` is the project's own documentation,
/// which used to be the Markdown files in `docs/`.
pub const VOCABULARY: [&str; 9] = [
    "idea", "chapter", "position", "see-also", "instinct", "document", "section", "task", "part-of",
];

#[derive(Debug, Clone)]
pub struct BundledRecord {
    /// The filename without its extension — and therefore the Record's head.
    pub head: String,
    pub projection: Projection,
    pub body: String,
}

impl BundledRecord {
    /// What this Record IS, from its identity assertion.
    ///
    /// Two families live in one folder on purpose. The tutorial is
    /// `chapter` + `idea`; the project's own documentation, which used to be
    /// the Markdown in `docs/`, is `document` + `section` + `task`. They share
    /// a shape — a top-level thing with ordered children — so the sand reads
    /// both with one code path and the reader gets the reference material in
    /// the same place as the tutorial.
    pub fn identity(&self) -> &str {
        self.projection
            .assertions
            .iter()
            .find(|line| line.identity)
            .map(|line| line.predicate.as_str())
            .unwrap_or("")
    }

    /// Whether this Record is a top-level entry in the reader: a tutorial
    /// chapter or one of the project's documents.
    pub fn is_chapter(&self) -> bool {
        matches!(self.identity(), "chapter" | "document")
    }

    /// A top-level entry's own position, or a child's position inside its
    /// parent.
    pub fn position(&self) -> f64 {
        let predicate = if self.is_chapter() { "position" } else { self.parent_predicate() };
        self.projection
            .assertions
            .iter()
            .find(|line| line.predicate == predicate)
            .and_then(|line| line.quantity.as_deref())
            .and_then(|value| value.parse().ok())
            .unwrap_or(f64::MAX)
    }

    fn parent_predicate(&self) -> &'static str {
        if self.identity() == "idea" { "chapter" } else { "part-of" }
    }

    /// The uid of what this Record hangs off — the chapter of an idea, the
    /// document of a section, the section of a task.
    pub fn parent_uid(&self) -> Option<&str> {
        if self.is_chapter() {
            return None;
        }
        let predicate = self.parent_predicate();
        self.projection
            .assertions
            .iter()
            .find(|line| line.predicate == predicate)
            .and_then(|line| line.object.as_ref())
            .map(|link| link.uid.as_str())
    }

    /// The top-level entry this Record ultimately sits under, resolving one
    /// hop for a task (task -> section -> document).
    pub fn chapter_uid<'a>(&'a self, all: &'a [BundledRecord]) -> Option<&'a str> {
        let parent = self.parent_uid()?;
        let owner = all.iter().find(|r| r.projection.uid == parent)?;
        if owner.is_chapter() {
            Some(parent)
        } else {
            owner.parent_uid()
        }
    }
}

/// Every Record in the bundle, parsed.
///
/// Panics on a malformed file, deliberately: this is data compiled INTO the
/// binary, so a bad file is a build-time mistake by whoever edited
/// `docs/records/`, not a runtime condition to degrade around. Failing at the
/// first read is how they find out.
pub fn records() -> Vec<BundledRecord> {
    let mut out: Vec<BundledRecord> = BUNDLE
        .iter()
        .map(|(head, text)| {
            let (projection, body) = parse_file(text)
                .unwrap_or_else(|err| panic!("docs/records/{head}.lingua is malformed: {err}"));
            let projection = projection.unwrap_or_else(|| {
                panic!("docs/records/{head}.lingua has no metadata block")
            });
            assert!(
                !projection.uid.trim().is_empty(),
                "docs/records/{head}.lingua has no uid — the bundle cross-links by uid, so \
                 every file has to carry the one it will become"
            );
            BundledRecord {
                head: (*head).to_string(),
                projection,
                body,
            }
        })
        .collect();
    // Top-level entries in order, then their children in order. Reading order
    // lives in the assertions, never in the filenames. The tutorial comes
    // before the reference material: `document` sorts after `chapter` at the
    // same position, which is what the leading 0 vs 1 does.
    let snapshot = out.clone();
    let top_order: std::collections::HashMap<String, (f64, f64)> = out
        .iter()
        .filter(|r| r.is_chapter())
        .map(|r| {
            let family = if r.identity() == "chapter" { 0.0 } else { 1.0 };
            (r.projection.uid.clone(), (family, r.position()))
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |r: &BundledRecord| {
            let top = if r.is_chapter() {
                top_order.get(&r.projection.uid).copied().unwrap_or((2.0, f64::MAX))
            } else {
                r.chapter_uid(&snapshot)
                    .and_then(|uid| top_order.get(uid).copied())
                    .unwrap_or((2.0, f64::MAX))
            };
            // A section sorts by its own position; a task sorts immediately
            // after the section it belongs to rather than at the end.
            let within = if r.is_chapter() { -1.0 } else { r.position() };
            let last: f64 = if r.identity() == "task" { 1.0 } else { 0.0 };
            (top.0, top.1, within, last)
        };
        let (af, ap, aw, al) = key(a);
        let (bf, bp, bw, bl) = key(b);
        af.total_cmp(&bf)
            .then(ap.total_cmp(&bp))
            .then(aw.total_cmp(&bw))
            .then(al.total_cmp(&bl))
            .then(a.head.cmp(&b.head))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundle is data compiled into the binary; a file that cannot be
    /// parsed, or that lost its uid, breaks both the sand and the import at
    /// once. Cheaper to find here than in either of them.
    #[test]
    fn every_bundled_file_parses_and_carries_its_uid() {
        let records = records();
        assert!(records.len() > 20, "the bundle has content: {}", records.len());
        let chapters = records.iter().filter(|r| r.identity() == "chapter").count();
        assert_eq!(chapters, 7, "seven tutorial chapters");
        assert!(
            records.iter().any(|r| r.identity() == "document"),
            "and the project documents, which used to be the Markdown in docs/"
        );
    }

    /// Reading order comes from the assertions. If it did not, the first
    /// Record would be whatever sorted first alphabetically.
    #[test]
    fn chapters_come_back_in_reading_order() {
        let records = records();
        let order: Vec<String> = records
            .iter()
            .filter(|r| r.identity() == "chapter")
            .map(|r| r.head.clone())
            .collect();
        assert_eq!(
            order,
            vec![
                "First Steps",
                "Records",
                "Links",
                "Concepts",
                "Cells and Organs",
                "Transfers",
                "Karma"
            ]
        );
    }

    /// Every idea belongs to a chapter that is actually in the bundle. A link
    /// to a uid nothing claims would put a chapter's worth of reading
    /// somewhere nobody navigates to.
    #[test]
    fn every_child_reaches_a_top_level_entry_in_the_bundle() {
        let records = records();
        let tops: Vec<&str> = records
            .iter()
            .filter(|r| r.is_chapter())
            .map(|r| r.projection.uid.as_str())
            .collect();
        for record in records.iter().filter(|r| !r.is_chapter()) {
            let uid = record
                .chapter_uid(&records)
                .unwrap_or_else(|| panic!("{} reaches no chapter or document", record.head));
            assert!(tops.contains(&uid), "{} points outside the bundle", record.head);
        }
    }
}
