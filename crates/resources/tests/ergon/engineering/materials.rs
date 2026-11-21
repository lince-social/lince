// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/engineering/materials-engineering");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/fluid-dynamics"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = load(&library, &directory.join("Materials-engineering.lingua"));
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records = records_by_slug(&publication);
    assert_eq!(
        records["materials-engineering"].uid,
        "r_05BF5JM8141RKF1VT5998YFZQ9"
    );
    let predicates = [
        "materials-engineering",
        "fluid-dynamics",
        "electromagnetism",
        "thermodynamics-and-heat-transfer",
        "mechanical-engineering",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["materials-engineering"];
    let selected = selected_nodes(edges);
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document.uri.contains("/engineering/materials-engineering/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mat-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        (
            "rc-paper",
            "mat-independent-investigation",
            "reproducible-computation",
        ),
        (
            "mech-design-alternatives",
            "mat-selection-uncertainty",
            "mechanical-engineering",
        ),
        (
            "mech-production-resources",
            "mat-service-state",
            "mechanical-engineering",
        ),
        (
            "mech-condition-evidence",
            "mech-maintenance",
            "mechanical-engineering",
        ),
        (
            "mech-maintenance",
            "mat-material-failure",
            "mechanical-engineering",
        ),
        (
            "mech-failure-investigation",
            "mat-material-repair",
            "mechanical-engineering",
        ),
        (
            "mech-restoration",
            "mat-material-reuse",
            "mechanical-engineering",
        ),
        (
            "mech-recovery",
            "mat-recovery-entry",
            "mechanical-engineering",
        ),
        (
            "metal-recycling",
            "mat-recovery-rates",
            "metals-and-metallurgical-production",
        ),
        ("gc-redox-balance", "gc-nernst", "general-chemistry"),
        ("gc-nernst", "mat-degradation-kinetics", "general-chemistry"),
        (
            "metal-electrode-kinetics",
            "mat-mixed-balance",
            "metals-and-metallurgical-production",
        ),
        ("gc-corrosion", "mat-local-corrosion", "general-chemistry"),
        (
            "metal-corrosion",
            "mat-hydrogen",
            "metals-and-metallurgical-production",
        ),
        ("mech-tribology", "metal-wear", "mechanical-engineering"),
        (
            "metal-wear",
            "mat-erosion",
            "metals-and-metallurgical-production",
        ),
        (
            "fd-cavitation-dynamics",
            "mat-coupled-damage",
            "fluid-dynamics",
        ),
        (
            "mech-process-plan",
            "mat-processing-balance",
            "mechanical-engineering",
        ),
        (
            "th-lumped",
            "mat-process-atmosphere",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "metal-casting",
            "mat-shape-deformation",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-forging-rolling",
            "metal-extrusion-drawing-sheet",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-extrusion-drawing-sheet",
            "mat-thermal-processing",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-surface-finishing",
            "mat-processing-rheology",
            "mechanical-engineering",
        ),
        (
            "fd-generalized-newtonian",
            "fd-objective-models",
            "fluid-dynamics",
        ),
        ("fd-objective-models", "fd-rheometry", "fluid-dynamics"),
        ("fd-rheometry", "mat-polymer-shaping", "fluid-dynamics"),
        ("fd-porous", "mat-composite-process", "fluid-dynamics"),
        (
            "metal-powder",
            "mat-slurry-processing",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-joining",
            "mat-coatings",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-additive",
            "mat-process-qualification",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-process-variation",
            "mat-process-scaleup",
            "mechanical-engineering",
        ),
        (
            "th-statistical",
            "mat-capacity",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-kinetic-transport",
            "th-interfaces-nano",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-interfaces-nano",
            "mat-thermal-measure",
            "thermodynamics-and-heat-transfer",
        ),
        ("em-current-model", "em-hall", "electromagnetism"),
        ("em-hall", "mat-disordered-transport", "electromagnetism"),
        ("em-polarization", "em-matter-models", "electromagnetism"),
        ("em-matter-models", "em-loss", "electromagnetism"),
        ("em-loss", "mat-dielectric-loss", "electromagnetism"),
        ("em-magnetic-matter", "mat-exchange", "electromagnetism"),
        (
            "th-coupled",
            "mat-functional-inference",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "metal-metallurgical-states",
            "mat-ceramic-families",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-tensile-measures",
            "metal-tensile",
            "mechanical-engineering",
        ),
        (
            "metal-tensile",
            "mech-strain-field",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-strain-field",
            "mech-stress-field",
            "mechanical-engineering",
        ),
        (
            "mech-stress-field",
            "mech-elastic-law",
            "mechanical-engineering",
        ),
        (
            "mech-elastic-law",
            "mat-elastic-energy",
            "mechanical-engineering",
        ),
        (
            "mech-time-dependent",
            "mat-hereditary",
            "mechanical-engineering",
        ),
        (
            "metal-dislocations",
            "metal-strengthening",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-strengthening",
            "metal-precipitation-structure",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-precipitation-structure",
            "mat-obstacle-comparison",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-fracture-linear",
            "metal-fracture-impact",
            "mechanical-engineering",
        ),
        (
            "metal-fracture-impact",
            "mech-fracture-plastic",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-fracture-plastic",
            "mat-toughening",
            "mechanical-engineering",
        ),
        (
            "metal-fatigue",
            "mat-cyclic-families",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-creep",
            "mat-creep-damage",
            "metals-and-metallurgical-production",
        ),
        ("ql-units", "mat-language", "quantitative-literacy"),
        ("gc-energy", "mat-material-energy", "general-chemistry"),
        ("gc-entropy", "mat-potentials", "general-chemistry"),
        (
            "gc-chemical-potential",
            "mat-partial-properties",
            "general-chemistry",
        ),
        ("gc-arrhenius", "mat-random-walk", "general-chemistry"),
        (
            "metal-restoration",
            "mat-thermal-analysis",
            "metals-and-metallurgical-production",
        ),
        ("gc-matter", "mat-fractions", "general-chemistry"),
        ("met-measurand", "mat-representative", "metrology"),
        ("gc-mo", "mat-structure-entry", "general-chemistry"),
        ("gc-crystals", "mp-lattice", "general-chemistry"),
        ("mp-lattice", "mat-packing", "mineralogy-and-petrology"),
        ("mp-metric", "mp-indices", "mineralogy-and-petrology"),
        (
            "mp-indices",
            "mat-plane-mechanisms",
            "mineralogy-and-petrology",
        ),
        ("mp-symmetry", "mat-anisotropy", "mineralogy-and-petrology"),
        (
            "mp-bragg",
            "mat-structure-factor",
            "mineralogy-and-petrology",
        ),
        (
            "mp-cif",
            "mat-structure-evidence",
            "mineralogy-and-petrology",
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
        reachable_nodes(&records["materials-engineering"].uid, edges),
        selected
    );
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-materials-from-everyday-objects-to-research.lingua",
        "computation/An-inspectable-cubic-diffraction-and-basis-account.lingua",
        "sources/Mesoscale-Tensile-Blind-Predictions-AM-Bench.lingua",
        "selection/An-inspectable-beam-selection-example.lingua",
        "research/Materials-engineering-completion-and-specialist-continuations.lingua",
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
        "mat-diffraction-example",
        "mat-regular-example",
        "mat-diffusion-example",
        "mat-history-example",
        "mat-bands-example",
        "mat-cure-example",
        "mat-corrosion-example",
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
    for slug in [
        "metal-calphad",
        "metal-phase-book",
        "metal-cambridge",
        "mech-roylance",
        "mech-bower-solids",
        "mat-doitpoms-response",
        "mat-wood-handbook",
        "mat-ceramic-breviary",
        "th-nano",
        "mat-tong-quantum",
        "mat-tong-statistical",
        "mat-tong-solid",
        "mat-simon-solid",
        "mat-mit-functional",
        "mat-ralph-magnetism",
        "mat-doitpoms-functional",
        "mat-ifusp-solid",
        "mat-mit-processing",
        "mat-ou-manufacturing",
        "mat-rheo-cure",
        "mat-cure-inference",
        "mat-vcctl",
        "mat-mit-microfab",
        "mat-ald",
        "mat-aqueous",
        "mat-lister",
        "mat-npl-degradation",
        "mat-hydrogen-source",
        "mat-oxide-source",
        "mat-weather-protocol",
        "mat-weather-life",
        "mat-glass-corrosion",
        "mat-aar",
        "mat-nasa-durability",
        "mat-selection-mit",
        "mat-data-book",
        "mat-indices-book",
        "mat-selection-doe",
        "mat-ilcd",
        "mat-sustainable-metals",
        "mat-plastics-recovery",
        "mat-aggregate-recovery",
        "mat-atomistic-course",
        "mat-dft-precision",
        "mat-pfhub",
        "mat-am-residual",
        "mat-am-tensile",
        "mat-informatics",
    ] {
        assert!(records.contains_key(slug));
        assert!(!selected.contains(&records[slug].uid));
    }
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
