// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/gaia/earth/geology");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
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
        "Geology.lingua",
        "field/Field-records-and-specimen-identity.lingua",
        "structures/An-inspectable-three-point-plane.lingua",
        "research/Independent-geological-scholarship.lingua",
        "sources/Geodiversidade-do-Brasil.lingua",
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
    let publication = load(&library, &directory.join("Geology.lingua"));
    let records = records_by_slug(&publication);
    assert_eq!(records["geology"].uid, "r_3CK43VQS79NK0STWNPSCS8YM06");
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["geology"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| document.uri.contains("/ergon/gaia/earth/geology/"))
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("geo-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-graphs", "geo-maps", "quantitative-literacy"),
        ("gc-crystals", "geo-mineral-id", "general-chemistry"),
        ("met-sampling-target", "geo-sampling", "metrology"),
        ("gc-nuclear", "geo-age", "general-chemistry"),
        (
            "rc-numericaltests",
            "geo-plane-example",
            "reproducible-computation",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in geology:"));
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(reachable_nodes(&records["geology"].uid, edges), selected);
    assert_eq!(selected.len(), 80);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geo-sources",
        "geo-physical",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "geology" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        all_records(publication)
            .find(|record| record.slug.as_deref() == Some("geo-plane-example"))
            .unwrap()
            .body
            .split("```python file=geological_plane.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def fit_plane(points):"));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(code(&reopened), original_code);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
    });
    assert!(!moved.join("files").exists());
}
