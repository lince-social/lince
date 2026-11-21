// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/engineering/electrical-power-engineering");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
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
        "Electrical-power-engineering.lingua",
        "computation/An-inspectable-two-bus-AC-power-flow.lingua",
        "computation/An-inspectable-constrained-three-bus-dispatch.lingua",
        "research/Advanced-islanding-black-start-and-restoration-study.lingua",
        "sources/MATPOWER-User-Manual-8-1.lingua",
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
        &directory.join("Electrical-power-engineering.lingua"),
    );
    let records = records_by_slug(&publication);
    assert_eq!(
        records["electrical-power-engineering"].uid,
        "r_300EPNGATK7PHWNZCQFBXGZFCQ"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "electrical-power-engineering",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["electrical-power-engineering"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/engineering/electrical-power-engineering/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("pe-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "pe-basis", "quantitative-literacy"),
        ("ql-energy", "pe-energy-account", "quantitative-literacy"),
        ("em-dc-network", "pe-circuits", "electromagnetism"),
        ("em-phasors", "pe-conventions", "electromagnetism"),
        ("em-impedance", "pe-complex-power", "electromagnetism"),
        ("em-inductance", "pe-transformers", "electromagnetism"),
        ("em-current-model", "pe-conductors", "electromagnetism"),
        (
            "rc-numericaltests",
            "pe-two-bus-example",
            "reproducible-computation",
        ),
        ("met-electrical-case", "pe-metering", "metrology"),
        ("met-time-frequency", "pe-pmu", "metrology"),
        ("em-quasistatic", "pe-fault-transient", "electromagnetism"),
        (
            "em-magnetic-matter",
            "pe-magnetic-design",
            "electromagnetism",
        ),
        ("em-electromechanics", "pe-synchronous", "electromagnetism"),
        ("em-poynting", "pe-dc-link", "electromagnetism"),
        (
            "ql-sensitivity",
            "met-model-discrepancy",
            "quantitative-literacy",
        ),
        ("met-model-discrepancy", "pe-uncertainty", "metrology"),
        ("met-conformity", "pe-quality-evidence", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in electrical power engineering:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["electrical-power-engineering"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 149);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "pe-sources",
        "pe-matpower",
        "pe-bichels",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "electrical-power-engineering" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "pe-two-bus-example" => "two_bus_ac.py",
                    "pe-dc-dispatch-example" => "three_bus_dispatch.py",
                    "pe-swing-example" => "fault_clearing_swing.py",
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
