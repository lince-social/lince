// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/physics/fluid-dynamics");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
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
        "Fluid-dynamics.lingua",
        "computation/An-inspectable-nozzle-with-an-internal-normal-shock.lingua",
        "computation/An-inspectable-conservative-wet-shallow-water-solver.lingua",
        "research/Advanced-rheometry-constitutive-inference-and-processing-flow-study.lingua",
        "sources/Mecanica-dos-Fluidos-e-Transportes-Gobbi-Dias.lingua",
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
    let publication = load(&library, &directory.join("Fluid-dynamics.lingua"));
    let records = records_by_slug(&publication);
    assert_eq!(
        records["fluid-dynamics"].uid,
        "r_7PQW1WDHK708BFV23MD8ET61VZ"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "general-chemistry",
        "thermodynamics-and-heat-transfer",
        "fluid-dynamics",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["fluid-dynamics"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/fluid-dynamics/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("fd-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "fd-vocabulary", "quantitative-literacy"),
        ("ql-vectors", "ql-small-changes", "quantitative-literacy"),
        ("ql-small-changes", "fd-fields", "quantitative-literacy"),
        (
            "ql-balances",
            "fd-transport-theorem",
            "quantitative-literacy",
        ),
        (
            "ql-dimensionless",
            "fd-dimensionless",
            "quantitative-literacy",
        ),
        ("ql-scaling", "fd-model-similarity", "quantitative-literacy"),
        (
            "ql-discretization",
            "fd-finite-volume",
            "quantitative-literacy",
        ),
        ("ql-identifiability", "fd-inverse", "quantitative-literacy"),
        ("met-force-pressure", "fd-pressure", "metrology"),
        ("met-mass-flow", "fd-flow-measurement", "metrology"),
        ("met-dynamic", "fd-dynamic-evidence", "metrology"),
        ("met-optical", "fd-piv-methods", "metrology"),
        ("met-model-discrepancy", "fd-uncertainty", "metrology"),
        (
            "rc-numericaltests",
            "fd-fluid-verification",
            "reproducible-computation",
        ),
        ("em-calculus", "fd-tensors", "electromagnetism"),
        (
            "th-control",
            "fd-mechanical-energy",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-local-entropy",
            "fd-compressibility",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-property-model",
            "fd-sound",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-fluid-energy",
            "fd-scalar-transport",
            "thermodynamics-and-heat-transfer",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in fluid dynamics:"));
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["fluid-dynamics"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 167);
    for excluded in [
        "thermodynamics-and-heat-transfer",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "electromagnetism",
        "fd-sources",
        "fd-tong",
        "fd-rheology",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "fluid-dynamics" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "fd-pipe-example" => "pipe_operating_point.py",
                    "fd-nozzle-example" => "nozzle_with_shock.py",
                    "fd-blasius-example" => "blasius.py",
                    "fd-shallow-example" => "shallow_water.py",
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
    assert_eq!(original_examples.len(), 4);
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
