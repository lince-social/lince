// non-lean-yet

use std::collections::BTreeSet;

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let inquiry = trails().join("ergon/hephaestus/inquiry");
    let directory = inquiry.join("metrology");
    let quantitative = inquiry.join("quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let expected_documents = Publication::files(&directory).unwrap().len()
        + Publication::files(&quantitative).unwrap().len();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
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
        "Metrology.lingua",
        "foundations/Question-and-measurand.lingua",
        "uncertainty/Multivariate-and-constraints.lingua",
        "research/Independent-research.lingua",
        "sources/GUM-and-Amendment.lingua",
    ] {
        let publication = load(&library, &directory.join(entry));
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = load(&library, &directory.join("Metrology.lingua"));
    let records = records_by_slug(&publication);
    let reused = [
        ("ql-quantity", "met-measurand"),
        ("ql-units", "met-si-realizations"),
        ("ql-repeated-observations", "met-rr"),
        ("ql-small-changes", "met-sensitivity"),
    ];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/metrology/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("met-sources"))
        .map(|record| record.uid.clone())
        .collect();
    let edges = learning_edges(&publication, "metrology");
    let quantitative_edges = learning_edges(&publication, "quantitative-literacy");
    for (shared, continuation) in reused {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(quantitative_edges.iter().any(|(from, _)| *from == uid));
        assert!(edges.is_disjoint(&quantitative_edges));
    }
    let selected = selected_nodes(&edges);
    assert_eq!(selected, expected);
    assert_eq!(reachable_nodes(&records["metrology"].uid, &edges), selected);
    assert!(!selected.contains(&records["quantitative-literacy"].uid));
    assert!(!selected.contains(&records["met-gum"].uid));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(learning_edges(&reopened, "metrology"), edges);
    assert_eq!(
        learning_edges(&reopened, "quantitative-literacy"),
        quantitative_edges
    );
    assert_local_targets(&reopened, |assertion| {
        matches!(
            assertion.predicate.as_str(),
            "metrology" | "quantitative-literacy"
        )
    });
    assert!(!moved.join("files").exists());
}
