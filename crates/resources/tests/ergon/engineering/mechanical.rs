// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory =
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = load(
        &library,
        &directory.join("Mechanical-engineering-and-machine-design.lingua"),
    );
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records = records_by_slug(&publication);
    let predicates = [
        "mechanical-engineering",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["mechanical-engineering"];
    let selected = selected_nodes(edges);
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/engineering/mechanical-engineering-and-machine-design/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mech-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "mech-language", "quantitative-literacy"),
        ("ql-geometry", "mech-dimensions", "quantitative-literacy"),
        ("ql-vectors", "mech-force", "quantitative-literacy"),
        (
            "ql-small-changes",
            "mech-kinematics",
            "quantitative-literacy",
        ),
        ("ql-rates", "mech-initial-value", "quantitative-literacy"),
        ("ql-energy", "mech-work-energy", "quantitative-literacy"),
        (
            "metal-tensile",
            "mech-material-state",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-fracture-impact",
            "mech-fracture-plastic",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-fatigue",
            "mech-fatigue-models",
            "metals-and-metallurgical-production",
        ),
        (
            "met-dimensional-case",
            "mech-inspection-decision",
            "metrology",
        ),
        (
            "metal-joining",
            "mech-final-material-state",
            "metals-and-metallurgical-production",
        ),
    ] {
        expected.insert(records[shared].uid.clone());
        assert!(edges.contains(&(
            records[shared].uid.clone(),
            records[continuation].uid.clone()
        )));
        assert!(
            routes[original]
                .iter()
                .any(|(from, _)| *from == records[shared].uid)
        );
    }
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["mechanical-engineering"].uid, edges),
        selected
    );
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-machines-and-mechanical-design-from-zero.lingua",
        "computation/An-inspectable-thermoelastic-axial-bar.lingua",
        "computation/An-inspectable-feed-axis-motion-and-energy-account.lingua",
        "sources/Precision-Machine-Design-Hale-Thesis.lingua",
        "research/Independent-mechanical-investigation-and-revision.lingua",
    ] {
        let discovered = load(&library, &directory.join(entry));
        assert_eq!(discovered.documents.len(), expected_documents, "{entry}");
        let found: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(found, identities, "{entry}");
    }
    let examples: BTreeMap<_, _> = [
        "mech-beam-example",
        "mech-oscillator-example",
        "mech-bar-example",
        "mech-axis-example",
    ]
    .into_iter()
    .map(|slug| {
        let example = records[slug];
        let code = example
            .body
            .split("```python\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned();
        (example.uid.clone(), code)
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
        if let Some(code) = examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
    });
    assert!(!moved.join("files").exists());
}
