// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory =
        trails.join("ergon/hephaestus/utilities/industrial-gases-vacuum-and-high-purity-supplies");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/engineering/materials-engineering"),
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems"),
        trails.join("ergon/hephaestus/physics/fluid-dynamics"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/hephaestus/chemistry/industrial-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = load(
        &library,
        &directory.join("Industrial-gases-and-vacuum.lingua"),
    );
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    assert!(publication.documents.iter().all(
        |document| document.project.frequencies.is_empty() && document.project.rules.is_empty()
    ));
    let records = records_by_slug(&publication);
    assert_eq!(
        records["industrial-gases-and-vacuum"].uid,
        "r_3GHQJEYMYEN6JDKSF4PPJ403RF"
    );
    let predicates = [
        "industrial-gases-and-vacuum",
        "materials-engineering",
        "mechanical-engineering",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "chemical-supply-systems",
        "fluid-dynamics",
        "electromagnetism",
        "thermodynamics-and-heat-transfer",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "industrial-chemistry",
        "geology",
        "mineralogy-and-petrology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["industrial-gases-and-vacuum"];
    let selected = selected_nodes(edges);
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/utilities/industrial-gases-vacuum-and-high-purity-supplies/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("gas-sources"))
        .map(|record| record.uid.clone())
        .collect();
    assert_eq!(expected.len(), 224);
    for (shared, continuation, original) in [
        (
            "cs-common-cause",
            "gas-redundancy",
            "chemical-supply-systems",
        ),
        (
            "cs-gas-supply",
            "gas-supply-boundary",
            "chemical-supply-systems",
        ),
        ("cs-loading", "gas-cryo-transfer", "chemical-supply-systems"),
        ("fd-compressible-energy", "fd-nozzles", "fluid-dynamics"),
        ("fd-nozzles", "gas-delivery-choking", "fluid-dynamics"),
        ("fd-porous", "gas-bed-pressure", "fluid-dynamics"),
        ("fd-pressure", "gas-pressure", "fluid-dynamics"),
        ("gc-arrhenius", "gas-residence", "general-chemistry"),
        (
            "ic-adsorption-membranes",
            "gas-adsorption-entry",
            "industrial-chemistry",
        ),
        (
            "ic-air-gases",
            "gas-production-entry",
            "industrial-chemistry",
        ),
        ("ic-control", "gas-operating-states", "industrial-chemistry"),
        ("ic-high-purity", "gas-specialty", "industrial-chemistry"),
        (
            "ic-hydrogen",
            "gas-hydrogen-service",
            "industrial-chemistry",
        ),
        (
            "ic-phase-equilibrium",
            "gas-phase-separation",
            "industrial-chemistry",
        ),
        ("ic-quantities", "gas-amount", "industrial-chemistry"),
        (
            "mat-diffusion-example",
            "gas-bulk-diffusion",
            "materials-engineering",
        ),
        (
            "mat-diffusion-law",
            "mat-diffusion-solutions",
            "materials-engineering",
        ),
        (
            "mat-diffusion-solutions",
            "mat-diffusion-example",
            "materials-engineering",
        ),
        (
            "mech-commissioning",
            "gas-commissioning",
            "mechanical-engineering",
        ),
        (
            "mech-condition-evidence",
            "gas-condition",
            "mechanical-engineering",
        ),
        (
            "mech-failure-investigation",
            "gas-failure-evidence",
            "mechanical-engineering",
        ),
        (
            "mech-human-access",
            "gas-failure-modes",
            "mechanical-engineering",
        ),
        (
            "mech-maintenance",
            "gas-maintenance",
            "mechanical-engineering",
        ),
        (
            "mech-restoration",
            "gas-restoration",
            "mechanical-engineering",
        ),
        ("mech-seals", "gas-vacuum-joints", "mechanical-engineering"),
        ("met-conformity", "gas-measurement-decision", "metrology"),
        ("met-inverse-bayesian", "gas-spectrum-inverse", "metrology"),
        ("met-measurand", "gas-trace-evidence", "metrology"),
        ("met-regression", "met-regression-inverse", "metrology"),
        ("met-regression-inverse", "gas-gauge-factor", "metrology"),
        ("met-response", "gas-force-gauges", "metrology"),
        ("met-sensitivity", "gas-measurement-budget", "metrology"),
        ("met-trace-chain", "gas-static-standard", "metrology"),
        ("ql-units", "fd-pressure", "quantitative-literacy"),
        (
            "rc-numericaltests",
            "gas-study-adsorption",
            "reproducible-computation",
        ),
        ("rc-paper", "gas-reproduce", "reproducible-computation"),
        (
            "th-coexistence",
            "gas-phase-inventory",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-control",
            "gas-storage-balance",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-flow-devices",
            "gas-compression",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-fourier",
            "gas-heat-paths",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-statistical",
            "gas-kinetics",
            "thermodynamics-and-heat-transfer",
        ),
    ] {
        expected.insert(records[shared].uid.clone());
        assert!(
            edges.contains(&(
                records[shared].uid.clone(),
                records[continuation].uid.clone()
            )),
            "{shared}"
        );
        assert!(
            routes[original]
                .iter()
                .any(|(from, _)| *from == records[shared].uid),
            "{shared}"
        );
    }
    assert_eq!(selected, expected);
    assert_eq!(selected.len(), 265);
    assert_eq!(
        reachable_nodes(&records["industrial-gases-and-vacuum"].uid, edges),
        selected
    );
    assert!(
        !edges
            .iter()
            .any(|(from, _)| *from == records["gas-completion"].uid)
    );
    for (start, branches, end) in [
        (
            "gas-stateful-collisions",
            [
                "gas-study-measure-pressure",
                "gas-study-measure-flow",
                "gas-study-measure-composition",
            ],
            "gas-measurement-review",
        ),
        (
            "gas-service-knowledge",
            [
                "gas-study-operation",
                "gas-study-failure",
                "gas-study-recovery",
            ],
            "gas-operation-review",
        ),
        (
            "gas-integrated-entry",
            [
                "gas-study-clean-system",
                "gas-study-helium-system",
                "gas-study-air-system",
            ],
            "gas-integrated-review",
        ),
    ] {
        for branch in branches {
            assert!(edges.contains(&(records[start].uid.clone(), records[branch].uid.clone())));
            assert!(edges.contains(&(records[branch].uid.clone(), records[end].uid.clone())));
        }
    }
    let own_sources: Vec<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/utilities/industrial-gases-vacuum-and-high-purity-supplies/sources/")
        })
        .flat_map(|document| &document.project.records)
        .collect();
    assert_eq!(own_sources.len(), 35);
    assert!(
        own_sources
            .iter()
            .all(|record| !selected.contains(&record.uid) && record.body.contains("](https://"))
    );
    for slug in [
        "gas-sources",
        "mechanical-engineering",
        "materials-engineering",
        "metrology",
        "rc-completion",
    ] {
        assert!(!selected.contains(&records[slug].uid), "{slug}");
    }
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-gas-and-vacuum-systems-from-everyday-use-to-research.lingua",
        "computation/An-inspectable-network-pumpdown-and-gas-balance.lingua",
        "sources/Reliability-Centered-Maintenance-NASA.lingua",
        "recovery/A-lower-pressure-and-a-purer-refill-are-different-objectives.lingua",
        "research/Completion-and-continuing-gas-and-vacuum-studies.lingua",
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
    let examples = |p: &Publication| -> BTreeMap<String, Vec<String>> {
        p.documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let blocks: Vec<_> = record
                    .body
                    .split("```python\n")
                    .skip(1)
                    .map(|block| block.split("\n```").next().unwrap().to_owned())
                    .collect();
                (!blocks.is_empty()).then(|| (record.uid.clone(), blocks))
            })
            .collect()
    };
    let original_examples = examples(&publication);
    assert_eq!(
        original_examples[&records["gas-network-example"].uid].len(),
        1
    );
    assert!(original_examples.contains_key(&records["mat-diffusion-example"].uid));
    let references = external_references(&publication);
    let (reopened, moved) = relocate(&publication, &library, temporary.path());
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    assert_local_targets(&reopened, |assertion| {
        predicates.contains(&assertion.predicate.as_str())
            || (assertion.predicate == "source" && assertion.object_uri.is_some())
    });
    assert!(!moved.join("files").exists());
}
