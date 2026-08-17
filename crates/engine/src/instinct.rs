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
///
/// **One tree, one parent link.** `@part-of [[Idea|uid]] n` is how everything
/// hangs off everything: a chapter off the root, an idea off a chapter, a task
/// off the idea it belongs to. The number on the link is the order among
/// siblings, and it is a decimal, so inserting between 7 and 8 is `7.5` and
/// renumbers nothing. `@reference` is the only other link, and it points
/// sideways rather than down — it never makes a parent.
///
/// `@chapter` survives as the identity of a branch worth its own entry in the
/// reader. It is also still accepted as a PARENT link while the corpus is
/// converted file by file; `@see-also` likewise, until the last one is renamed
/// to `@reference`.
///
/// The state words mirror the quantity ladder — `1` stable, `0` backlog, `-1`
/// todo, `-2` wip — so a sand can colour and filter on a Concept while the
/// number stays the thing that sorts. They are two projections of one fact,
/// never two authorities: `check_lingua.js` refuses a file whose word and
/// number disagree.
pub const VOCABULARY: [&str; 14] = [
    "idea",
    "chapter",
    "position",
    "see-also",
    "reference",
    "instinct",
    "document",
    "section",
    "task",
    "part-of",
    "stable",
    "backlog",
    "todo",
    "wip",
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
    /// The folder used to hold two families that never touched — `chapter` +
    /// `idea` for the tutorial, `document` + `section` + `task` for the
    /// project's own documentation — describing the same subjects twice under
    /// two different link names. They are being merged into one tree; identity
    /// now says how a Record is READ (`chapter` earns its own entry) rather
    /// than which of two documentations it belonged to.
    pub fn identity(&self) -> &str {
        self.projection
            .assertions
            .iter()
            .find(|line| line.identity)
            .map(|line| line.predicate.as_str())
            .unwrap_or("")
    }

    /// The one line that makes this Record a child of another.
    ///
    /// `@chapter` is still read here because the corpus is being converted a
    /// subject at a time and a half-converted folder must still render. When
    /// the last `@chapter` link is gone this is a single `find`.
    ///
    /// A parent link is a line with an OBJECT, which is what separates
    /// `@chapter [[Karma|uid]] 2` — an idea saying which branch it belongs to
    /// — from `@@chapter`, the identity line saying what the Record itself is.
    /// The two spell the same word and mean opposite things, which is one more
    /// reason `@part-of` is the name worth converging on.
    fn parent_line(&self) -> Option<&crate::lingua_file::Line> {
        let link = |name: &str| {
            self.projection
                .assertions
                .iter()
                .find(|line| line.predicate == name && !line.identity && line.object.is_some())
        };
        link("part-of").or_else(|| link("chapter"))
    }

    /// Whether this Record is the root — the one thing nothing contains.
    ///
    /// Being the root is a fact about links, not about identity: a Record is
    /// the root because nothing above it claims it, which is checkable, rather
    /// than because it calls itself a chapter, which is a label anyone can
    /// write on anything.
    pub fn is_root(&self) -> bool {
        self.parent_line().is_none()
    }

    /// Where this Record sits among its siblings — the number on its parent
    /// link. The root has no parent, so it falls back to `@position`.
    ///
    /// A decimal on purpose. Order is the thing most often revised, and an
    /// integer scheme makes inserting one idea into the middle of nine a
    /// nine-file edit; `7.5` is a one-file edit that reads exactly as clearly.
    pub fn position(&self) -> f64 {
        let predicate = if self.is_root() { "position" } else { "" };
        self.parent_line()
            .or_else(|| self.projection.assertions.iter().find(|l| l.predicate == predicate))
            .and_then(|line| line.quantity.as_deref())
            .and_then(|value| value.parse().ok())
            .unwrap_or(f64::MAX)
    }

    /// The uid of whatever this Record hangs off.
    pub fn parent_uid(&self) -> Option<&str> {
        self.parent_line()?.object.as_ref().map(|link| link.uid.as_str())
    }

    /// This Record's position, and its parent's, and its parent's, from the
    /// root down: `[0, 11, 2, 4]`.
    ///
    /// **This is what puts the whole folder in one order.** Comparing two of
    /// these compares the branches first and only then the leaves, so a
    /// depth-first reading falls out of an ordinary sort at any depth — which
    /// the old fixed `(family, chapter, position)` key could not do, because
    /// it could only see one hop above a Record.
    ///
    /// A missing or circular parent stops the walk rather than looping; the
    /// Record sorts as if it hung off wherever the walk stopped, which puts a
    /// broken link somewhere visible instead of hanging the build.
    pub fn path(&self, all: &[BundledRecord]) -> Vec<f64> {
        let mut out = vec![self.position()];
        let mut seen = vec![self.projection.uid.as_str()];
        let mut current = self;
        while let Some(parent) = current.parent_uid() {
            if seen.contains(&parent) {
                break;
            }
            let Some(owner) = all.iter().find(|r| r.projection.uid == parent) else {
                break;
            };
            out.push(owner.position());
            seen.push(parent);
            current = owner;
        }
        out.reverse();
        out
    }

    /// Whether this Record opens an entry in the reader's navigation.
    ///
    /// **This is the whole job `@@chapter` has left.** The reader needs a flat
    /// list of entries while the data is a tree of any depth, and depth alone
    /// cannot decide where to cut: `Record` sits two levels down and deserves
    /// its own entry, while the nine ideas of `First Steps` sit one level down
    /// and belong inside it. So it is a judgement, written on the Record —
    /// this subject is big enough to be arrived at directly — and everything
    /// else is read within the nearest chapter above it.
    pub fn is_entry(&self) -> bool {
        self.is_root() || self.identity() == "chapter"
    }

    /// The entry this Record is read under: itself, or the nearest chapter
    /// above it.
    pub fn entry_uid<'a>(&'a self, all: &'a [BundledRecord]) -> &'a str {
        let mut current = self;
        let mut seen = vec![current.projection.uid.as_str()];
        while !current.is_entry() {
            let Some(parent) = current.parent_uid() else { break };
            if seen.contains(&parent) {
                break;
            }
            let Some(owner) = all.iter().find(|r| r.projection.uid == parent) else {
                break;
            };
            seen.push(parent);
            current = owner;
        }
        current.projection.uid.as_str()
    }
}

/// Where a file that arrived without a metadata block is filed among the
/// root's children: after everything somebody placed on purpose.
const UNFILED_ORDER: &str = "999";

/// The uid a file gets when it carries none — derived from its head, so the
/// same file is the same Record on every machine and across every build.
///
/// The bundle cross-links by uid and the import writes Records under these
/// uids, so a fresh random one per boot would make a new Record every time the
/// binary was rebuilt. Deriving it from the head instead means writing the
/// file IS adopting it: rename the file and it becomes a different Record,
/// which is the same rule the rest of the folder already follows.
fn derived_uid(head: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(head.as_bytes());
    let mut millis = [0u8; 8];
    millis[2..8].copy_from_slice(&digest[0..6]);
    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&digest[6..22]);
    format!(
        "r_{}",
        nucleus::id::ulid_from(u64::from_be_bytes(millis), u128::from_be_bytes(entropy)),
    )
}

/// What a hand-written file with no metadata block becomes.
///
/// Refusing it was a panic that took the whole Cell down with it, which made
/// dropping a plain file into `docs/records/` an act nobody could recover from
/// through the UI. So a bare file is ADOPTED instead: it is an `@@idea`, it
/// hangs off the root at the end, and it carries a uid derived from its head.
/// Nothing about it is a guess at meaning — that is a decision for whoever
/// writes the block, in the file or in the sand.
///
/// It hangs off the root rather than floating free because a Record with no
/// parent IS a root here, and a second root is a second documentation nobody
/// asked for. Filed last under the root, it is in the tree and visibly
/// unplaced, which is what a file waiting to be sorted should look like.
fn adopted_projection(head: &str, root: Option<&(String, String)>) -> Projection {
    let mut assertions = vec![crate::lingua_file::Line {
        predicate: "idea".to_string(),
        identity: true,
        object: None,
        quantity: None,
        unit: None,
    }];
    if let Some((title, uid)) = root {
        assertions.push(crate::lingua_file::Line {
            predicate: "part-of".to_string(),
            identity: false,
            object: Some(crate::lingua_file::Link {
                title: title.clone(),
                uid: uid.clone(),
            }),
            quantity: Some(UNFILED_ORDER.to_string()),
            unit: None,
        });
    }
    Projection {
        uid: derived_uid(head),
        assertions,
        quantity: None,
    }
}

/// Every Record in the bundle, parsed.
///
/// Panics on a file whose metadata block cannot be READ, deliberately: this is
/// data compiled INTO the binary, so a block naming a concept that is not
/// Lingua is a build-time mistake by whoever edited `docs/records/`. A file
/// with no block at all is a different thing entirely and is adopted — see
/// `adopted_projection`.
pub fn records() -> Vec<BundledRecord> {
    let parsed: Vec<(String, Option<Projection>, String)> = BUNDLE
        .iter()
        .map(|(head, text)| {
            let (projection, body) = parse_file(text)
                .unwrap_or_else(|err| panic!("docs/records/{head}.lingua is malformed: {err}"));
            // A block that lost its uid is as unadopted as a file with no
            // block: the uid is what the rest of the folder links to.
            let projection = projection.filter(|p| !p.uid.trim().is_empty());
            ((*head).to_string(), projection, body)
        })
        .collect();
    // Read from the files that carry their own metadata, before any adoption:
    // an adopted file hangs off the root, so it must not be able to become the
    // root by being read first.
    let root: Option<(String, String)> = parsed
        .iter()
        .filter_map(|(head, projection, _)| {
            projection.clone().map(|projection| BundledRecord {
                head: head.clone(),
                projection,
                body: String::new(),
            })
        })
        .find(BundledRecord::is_root)
        .map(|record| (record.head, record.projection.uid));
    let mut out: Vec<BundledRecord> = parsed
        .into_iter()
        .map(|(head, projection, body)| {
            let projection =
                projection.unwrap_or_else(|| adopted_projection(&head, root.as_ref()));
            BundledRecord {
                head,
                projection,
                body,
            }
        })
        .collect();
    // Depth-first from the root, which is what reading the tree out loud is.
    // A parent sorts before its children because its path is a prefix of
    // theirs and a shorter vector compares as less; siblings sort by the
    // number on their own parent link. Nothing here knows how deep the tree
    // goes, which is the point — the old key could only see one hop.
    let snapshot = out.clone();
    let paths: std::collections::HashMap<String, Vec<f64>> = snapshot
        .iter()
        .map(|r| (r.projection.uid.clone(), r.path(&snapshot)))
        .collect();
    out.sort_by(|a, b| {
        let empty = Vec::new();
        let left = paths.get(&a.projection.uid).unwrap_or(&empty);
        let right = paths.get(&b.projection.uid).unwrap_or(&empty);
        left.iter()
            .zip(right.iter())
            .find_map(|(x, y)| match x.total_cmp(y) {
                std::cmp::Ordering::Equal => None,
                other => Some(other),
            })
            .unwrap_or_else(|| left.len().cmp(&right.len()))
            .then(a.head.cmp(&b.head))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writing a plain file into `docs/records/` is how a thought gets in.
    /// It used to be how the Cell stopped booting: no metadata block was a
    /// panic, in a bundle compiled into the binary, so the only way out was to
    /// edit the file by hand — never through the UI the file was written for.
    #[test]
    fn a_file_with_no_metadata_block_is_adopted_rather_than_refused() {
        let root = ("First Steps".to_string(), "r_S8PQ17MQ3WBWM53ZN700K89V9F".to_string());
        let projection = adopted_projection("Thoughts", Some(&root));
        assert!(
            nucleus::id::valid_uid(&projection.uid, "r"),
            "adoption hands out a real uid, not a placeholder: {}",
            projection.uid
        );
        // The same file is the same Record on the next build, and on another
        // machine — otherwise every rebuild would import it again as new.
        assert_eq!(projection.uid, adopted_projection("Thoughts", None).uid);
        assert_ne!(projection.uid, adopted_projection("Other", Some(&root)).uid);

        let record = BundledRecord {
            head: "Thoughts".to_string(),
            projection,
            body: String::new(),
        };
        assert_eq!(record.identity(), "idea");
        assert_eq!(record.parent_uid(), Some(root.1.as_str()));
        assert!(
            !record.is_root(),
            "an unplaced file joins the tree — a second root would be a second documentation"
        );
    }

    /// The bundle is data compiled into the binary; a file that cannot be
    /// parsed, or that lost its uid, breaks both the sand and the import at
    /// once. Cheaper to find here than in either of them.
    #[test]
    fn every_bundled_file_parses_and_carries_its_uid() {
        let records = records();
        assert!(records.len() > 20, "the bundle has content: {}", records.len());
    }

    /// **One tree, one root.** Two roots is two documentations that happen to
    /// share a folder, which is the state this replaced; nothing above the
    /// data would say which one a reader starts at.
    #[test]
    fn the_folder_is_one_tree_rooted_at_first_steps() {
        let records = records();
        let roots: Vec<&str> = records.iter().filter(|r| r.is_root()).map(|r| r.head.as_str()).collect();
        assert_eq!(roots, vec!["First Steps"], "exactly one Record has no parent");
    }

    /// Reading order comes from the assertions. If it did not, the first
    /// Record would be whatever sorted first alphabetically.
    #[test]
    fn the_reader_starts_at_the_root_and_the_branches_follow_it() {
        let records = records();
        let entries: Vec<String> =
            records.iter().filter(|r| r.is_entry()).map(|r| r.head.clone()).collect();
        assert_eq!(entries[0], "First Steps", "the root is read first");
        assert_eq!(entries[1], "Lince", "then why any of this exists");
        assert!(entries.contains(&"Ontology".to_string()), "then the branches");
    }

    /// A parent is read before its children, at any depth. The old key could
    /// only see one hop above a Record, so this is the property that had to be
    /// proven again once the tree could be deeper than three.
    #[test]
    fn a_parent_is_always_read_before_its_children() {
        let records = records();
        let at = |uid: &str| records.iter().position(|r| r.projection.uid == uid);
        for (index, record) in records.iter().enumerate() {
            let Some(parent) = record.parent_uid() else { continue };
            let owner = at(parent)
                .unwrap_or_else(|| panic!("{} hangs off a uid no file claims", record.head));
            assert!(owner < index, "{} is read before its parent", record.head);
        }
    }

    /// Every Record reaches an entry the reader actually navigates to. A link
    /// to a uid nothing claims would put a branch's worth of reading somewhere
    /// nobody can get to.
    #[test]
    fn every_record_reaches_an_entry_in_the_bundle() {
        let records = records();
        let entries: Vec<&str> = records
            .iter()
            .filter(|r| r.is_entry())
            .map(|r| r.projection.uid.as_str())
            .collect();
        for record in &records {
            let uid = record.entry_uid(&records);
            assert!(entries.contains(&uid), "{} points outside the bundle", record.head);
        }
    }
}



