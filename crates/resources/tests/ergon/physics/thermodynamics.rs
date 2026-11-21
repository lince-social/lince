// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer");
    let expected_documents = [
        directory.clone(),
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
        "Thermodynamics-and-heat-transfer.lingua",
        "computation/An-inspectable-finite-body-work-and-entropy-account.lingua",
        "computation/An-inspectable-conservative-transient-layered-slab.lingua",
        "research/Thermoreflectance-film-interface-and-transducer-identification.lingua",
        "sources/Thermodynamics-and-Chemistry-DeVoe.lingua",
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
    let publication = load(
        &library,
        &directory.join("Thermodynamics-and-heat-transfer.lingua"),
    );
    let records = records_by_slug(&publication);
    assert_eq!(
        records["thermodynamics-and-heat-transfer"].uid,
        "r_6KQBZNJM6WNV9A69R3DZY1NH7E"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "general-chemistry",
        "thermodynamics-and-heat-transfer",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["thermodynamics-and-heat-transfer"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/thermodynamics-and-heat-transfer/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("th-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "th-scales", "quantitative-literacy"),
        ("ql-energy", "th-energy", "quantitative-literacy"),
        ("ql-balances", "th-control", "quantitative-literacy"),
        (
            "ql-small-changes",
            "th-mathematics",
            "quantitative-literacy",
        ),
        ("ql-dimensionless", "th-similarity", "quantitative-literacy"),
        (
            "ql-discretization",
            "th-discretization",
            "quantitative-literacy",
        ),
        ("ql-identifiability", "th-inverse", "quantitative-literacy"),
        (
            "ql-sensitivity",
            "met-model-discrepancy",
            "quantitative-literacy",
        ),
        ("met-temperature", "th-measurement", "metrology"),
        ("met-optical", "th-infrared", "metrology"),
        ("met-model-discrepancy", "th-uncertainty", "metrology"),
        (
            "rc-numericaltests",
            "th-slab-example",
            "reproducible-computation",
        ),
        ("gc-energy", "th-conventions", "general-chemistry"),
        ("gc-calorimetry", "th-capacity", "general-chemistry"),
        ("gc-gases", "th-eos", "general-chemistry"),
        ("gc-entropy", "th-entropy-balance", "general-chemistry"),
        ("gc-gibbs", "th-potentials", "general-chemistry"),
        ("gc-phases", "th-phase-properties", "general-chemistry"),
        ("gc-chemical-potential", "th-mixtures", "general-chemistry"),
        ("em-fresnel", "th-spectral", "electromagnetism"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in thermodynamics and heat transfer:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["thermodynamics-and-heat-transfer"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 160);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "electromagnetism",
        "th-sources",
        "th-yan",
        "th-heatpipe-handbook",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "thermodynamics-and-heat-transfer" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "th-reservoir-example" => "finite_thermal_bodies.py",
                    "th-slab-example" => "layered_slab.py",
                    "th-radiation-example" => "grey_enclosure.py",
                    "th-exchanger-example" => "counterflow_exchanger.py",
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
