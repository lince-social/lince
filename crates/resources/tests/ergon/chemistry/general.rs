// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/chemistry/general-chemistry");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
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
        "General-chemistry.lingua",
        "reactions/Reaction-extent-and-species-balances.lingua",
        "equilibrium/An-inspectable-equilibrium-solver.lingua",
        "research/Independent-scholarship-and-revision.lingua",
        "sources/Quimica-Geral-I-UECE.lingua",
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
    let publication = load(&library, &directory.join("General-chemistry.lingua"));
    let records = records_by_slug(&publication);
    assert_eq!(
        records["general-chemistry"].uid,
        "r_206GS6CVXD8WSY1VX7PRF7136V"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["general-chemistry"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/chemistry/general-chemistry/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("gc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "gc-amount", "quantitative-literacy"),
        ("ql-balances", "gc-extent", "quantitative-literacy"),
        (
            "ql-concentrations",
            "gc-preparation",
            "quantitative-literacy",
        ),
        ("met-chemistry-case", "gc-spectra", "metrology"),
        ("rc-numericaltests", "gc-solver", "reproducible-computation"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in general chemistry:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["general-chemistry"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 92);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "gc-sources",
        "gc-openstax",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "general-chemistry" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        all_records(publication)
            .find(|record| record.slug.as_deref() == Some("gc-solver"))
            .unwrap()
            .body
            .split("```python file=equilibrium.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def solve(added_base_ml):"));
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
