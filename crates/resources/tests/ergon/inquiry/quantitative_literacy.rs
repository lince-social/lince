// non-lean-yet

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, external_references, learning_edges, load, reachable_nodes, relocate,
    selected_nodes, trails,
};

pub(crate) fn run() {
    let directory = trails().join("ergon/hephaestus/inquiry/quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let files = Publication::files(&directory).unwrap();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert_eq!(audit.documents, files.len());
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Quantitative-literacy.lingua",
        "foundations/Units-and-dimensions.lingua",
        "models/Dimensionless-groups.lingua",
        "study/Advanced-quantitative-study.lingua",
        "sources/SI-Brochure.lingua",
    ] {
        let publication = load(&library, &directory.join(entry));
        assert_eq!(publication.documents.len(), files.len(), "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = load(&library, &directory.join("Quantitative-literacy.lingua"));
    let records: BTreeMap<_, _> = all_records(&publication)
        .map(|record| (record.uid.clone(), record))
        .collect();
    let expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            let path = PathBuf::from(document.uri.strip_prefix("file://").unwrap());
            path.parent().unwrap().file_name().unwrap() != "sources"
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("ql-sources"))
        .map(|record| record.uid.clone())
        .collect();
    let edges = learning_edges(&publication, "quantitative-literacy");
    for (_, target) in &edges {
        assert!(records.contains_key(target), "{target}");
    }
    let selected = selected_nodes(&edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes("r_4XVN56PQC13JGNFPV0CSGXDCZ2", &edges),
        selected
    );
    let external_links: BTreeSet<_> = records
        .values()
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect();
    assert!(!external_links.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    let relocated_identities: BTreeSet<_> = all_records(&reopened)
        .map(|record| record.uid.clone())
        .collect();
    assert_eq!(relocated_identities, records.keys().cloned().collect());
    assert_eq!(external_references(&reopened), external_links);
    assert!(!moved.join("files").exists());
}
