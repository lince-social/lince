// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let inquiry = trails().join("ergon/hephaestus/inquiry");
    let directory = inquiry.join("reproducible-computation");
    let expected_documents = [
        &directory,
        &inquiry.join("quantitative-literacy"),
        &inquiry.join("metrology"),
    ]
    .into_iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
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
        "Reproducible-computation.lingua",
        "data/A-complete-small-analysis.lingua",
        "numerics/Random-streams-and-task-identity.lingua",
        "research/Independent-investigation-and-revision.lingua",
        "sources/Scientific-Python-Sundnes.lingua",
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
    let publication = load(&library, &directory.join("Reproducible-computation.lingua"));
    let records = records_by_slug(&publication);
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["reproducible-computation"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/reproducible-computation/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("rc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-data", "rc-schema", "quantitative-literacy"),
        ("ql-sensitivity", "rc-conditioning", "quantitative-literacy"),
        (
            "ql-discretization",
            "rc-convergence",
            "quantitative-literacy",
        ),
        ("met-monte-carlo", "rc-streams", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in reproducible computation:")
        );
    }
    assert!(edges.is_disjoint(&routes["quantitative-literacy"]));
    assert!(edges.is_disjoint(&routes["metrology"]));
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["reproducible-computation"].uid, edges),
        selected
    );
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "rc-sources",
        "rc-sundnes",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    let examples = |publication: &Publication| -> BTreeMap<String, Vec<String>> {
        all_records(publication)
            .filter(|record| record.body.contains("```"))
            .map(|record| {
                (
                    record.uid.clone(),
                    record
                        .body
                        .split("```")
                        .skip(1)
                        .step_by(2)
                        .map(str::to_owned)
                        .collect(),
                )
            })
            .collect()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 7);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(examples(&reopened), original_examples);
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
    });
    assert!(!moved.join("files").exists());
}
