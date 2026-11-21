// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::Library;

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let directory = trails().join("ergon/hephaestus/utilities/water-and-sanitation");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = load(
        &library,
        &directory.join("Water-sanitation-and-industrial-utilities.lingua"),
    );
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records = records_by_slug(&publication);
    let edges = learning_edges(&publication, "water-and-sanitation");
    let selected = selected_nodes(&edges);
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document.uri.contains("/utilities/water-and-sanitation/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("wu-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for slug in [
        "ql-units",
        "ql-balances",
        "gc-solubility",
        "gc-carbonate",
        "gc-activities",
        "gc-balances-equilibrium",
        "met-chemistry-case",
        "met-sampling-target",
        "met-sampling-uncertainty",
        "gc-rate-laws",
        "fd-fittings",
        "th-exchanger-reality",
        "pe-feedback-control",
    ] {
        expected.insert(records[slug].uid.clone());
    }
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["water-and-sanitation"].uid, &edges),
        selected
    );
    for (shared, next, original) in [
        ("gc-rate-laws", "wu-mixed-tank", "general-chemistry"),
        ("fd-fittings", "wu-treatment-dosing", "fluid-dynamics"),
        (
            "th-exchanger-reality",
            "wu-steam-balance",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "pe-feedback-control",
            "wu-water-control",
            "electrical-power-engineering",
        ),
    ] {
        assert!(edges.contains(&(records[shared].uid.clone(), records[next].uid.clone())));
        assert!(
            learning_edges(&publication, original)
                .iter()
                .any(|(from, _)| *from == records[shared].uid)
        );
    }
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "industrial/An-inspectable-reuse-loop-and-concentrate-account.lingua",
        "operations/An-original-storage-and-service-history.lingua",
        "sources/Benchmark-Simulation-Model-2-IWA.lingua",
    ] {
        let discovered = load(&library, &directory.join(entry));
        let discovered_identities: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(discovered_identities, identities, "{entry}");
    }
    let predicates = [
        "water-and-sanitation",
        "general-chemistry",
        "fluid-dynamics",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "geology",
        "electromagnetism",
        "electrical-power-engineering",
        "thermodynamics-and-heat-transfer",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let original_examples: BTreeMap<_, _> = [
        "wu-reactor-comparison",
        "wu-distribution-example",
        "wu-biomass-example",
        "wu-batch-example",
        "wu-reuse-example",
        "wu-service-example",
    ]
    .into_iter()
    .map(|slug| {
        (
            records[slug].uid.clone(),
            records[slug]
                .body
                .split("```python\n")
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned(),
        )
    })
    .collect();
    let references = external_references(&publication);
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    for record in all_records(&reopened) {
        if let Some(code) = original_examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
    });
    assert!(!moved.join("files").exists());
}
