// non-lean-yet

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::rename,
    path::{Path, PathBuf},
};

use anicca::{ProjectedAssertion, ProjectedRecord};
use lince_resources::{Library, publication::Publication};

pub(super) type Records<'a> = BTreeMap<&'a str, &'a ProjectedRecord>;
pub(super) type Edges = BTreeSet<(String, String)>;

pub(super) fn trails() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails")
}

pub(super) fn load(library: &Library, path: &Path) -> Publication {
    let uri = path.to_str().expect("the fixture path is UTF-8");
    Publication::load(library, uri, &[], false)
        .unwrap_or_else(|error| panic!("could not load {uri}: {error}"))
}

pub(super) fn relocate(
    publication: &Publication,
    library: &Library,
    temporary: &Path,
) -> (Publication, PathBuf) {
    let destination = temporary.join("export");
    let exported = publication
        .export(library, &destination, false)
        .expect("export the publication");
    let entry = exported
        .strip_prefix(&destination)
        .expect("the exported entry is within the export directory");
    let moved = temporary.join("moved");
    rename(&destination, &moved).expect("move the export");
    let fresh = Library::new(temporary.join("relocated-library"));
    (load(&fresh, &moved.join(entry)), moved)
}

pub(super) fn all_records(publication: &Publication) -> impl Iterator<Item = &ProjectedRecord> {
    publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
}

pub(super) fn records_by_slug(publication: &Publication) -> Records<'_> {
    all_records(publication)
        .map(|record| {
            (
                record
                    .slug
                    .as_deref()
                    .expect("the fixture record has a slug"),
                record,
            )
        })
        .collect()
}

pub(super) fn learning_edges(publication: &Publication, predicate: &str) -> Edges {
    all_records(publication)
        .flat_map(|record| {
            record
                .assertions
                .iter()
                .filter(|assertion| assertion.predicate == predicate)
                .map(|assertion| (record.uid.clone(), target(assertion).1.to_owned()))
        })
        .collect()
}

pub(super) fn external_references(publication: &Publication) -> BTreeSet<String> {
    all_records(publication)
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect()
}

pub(super) fn selected_nodes(edges: &Edges) -> BTreeSet<String> {
    edges
        .iter()
        .flat_map(|(from, to)| [from, to])
        .cloned()
        .collect()
}

pub(super) fn reachable_nodes(root: &str, edges: &Edges) -> BTreeSet<String> {
    let mut successors = BTreeMap::<&str, Vec<&str>>::new();
    for (from, to) in edges {
        successors.entry(from).or_default().push(to);
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid)
            && let Some(next) = successors.get(uid)
        {
            pending.extend(next);
        }
    }
    reached.into_iter().map(str::to_owned).collect()
}

pub(super) fn assert_local_targets(
    publication: &Publication,
    include: impl Fn(&ProjectedAssertion) -> bool,
) {
    let bases: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in all_records(publication) {
        for assertion in record
            .assertions
            .iter()
            .filter(|assertion| include(assertion))
        {
            let (location, uid) = target(assertion);
            assert_eq!(
                bases.get(uid).copied(),
                Some(location),
                "{} –{}→ {uid}",
                record.uid,
                assertion.predicate
            );
        }
    }
}

fn target(assertion: &ProjectedAssertion) -> (&str, &str) {
    assertion
        .object_uri
        .as_deref()
        .expect("the assertion has a target URI")
        .rsplit_once('#')
        .expect("the assertion target URI has a Record UID fragment")
}
