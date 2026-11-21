// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/physics/electromagnetism");
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
        "Electromagnetism.lingua",
        "computation/An-inspectable-conservative-electrostatic-solver.lingua",
        "computation/An-inspectable-periodic-Yee-wave.lingua",
        "research/Advanced-time-domain-dispersion-PML-and-validation-study.lingua",
        "sources/Measuring-Permittivity-and-Permeability-NIST-TN-1536.lingua",
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
    let publication = load(&library, &directory.join("Electromagnetism.lingua"));
    let records = records_by_slug(&publication);
    assert_eq!(
        records["electromagnetism"].uid,
        "r_5TJPESCTTNAKY66JH2QNQYN1PY"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["electromagnetism"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/electromagnetism/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("em-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "em-constants", "quantitative-literacy"),
        ("ql-vectors", "em-vectorfields", "quantitative-literacy"),
        ("ql-small-changes", "em-calculus", "quantitative-literacy"),
        ("ql-rates", "em-continuity", "quantitative-literacy"),
        ("ql-energy", "em-potential", "quantitative-literacy"),
        ("met-electrical-case", "em-probing", "metrology"),
        ("met-time-frequency", "em-phasors", "metrology"),
        ("met-optical", "em-detection", "metrology"),
        (
            "ql-discretization",
            "em-numerical-fields",
            "quantitative-literacy",
        ),
        (
            "rc-numericaltests",
            "em-poisson-example",
            "reproducible-computation",
        ),
        ("met-model-discrepancy", "em-uncertainty", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in electromagnetism:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["electromagnetism"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 116);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "em-sources",
        "em-openstax",
        "em-nist1536",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "electromagnetism" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "em-poisson-example" => "electrostatic_faces.py",
                    "em-layer-example" => "dielectric_stack.py",
                    "em-yee-example" => "periodic_yee.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
    });
    assert!(!moved.join("files").exists());
}
