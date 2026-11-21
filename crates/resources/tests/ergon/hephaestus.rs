// non-lean-yet

mod programs;
mod routes;

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let directory = trails().join("ergon/hephaestus");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = load(&library, &directory.join("Hephaestus.lingua"));
    let records = records_by_slug(&publication);
    let area_slugs = [
        "mathematics-map",
        "physics-map",
        "chemistry-map",
        "computing-map",
        "engineering-map",
        "inquiry-map",
    ];
    let core_slugs = [
        "math-core",
        "physics-core",
        "chemistry-core",
        "computing-core",
        "engineering-core",
        "inquiry-core",
    ];
    let root = records["hephaestus"].uid.clone();
    let expected_map: BTreeSet<_> = area_slugs
        .iter()
        .map(|slug| (root.clone(), records[slug].uid.clone()))
        .collect();
    assert_eq!(learning_edges(&publication, "hephaestus"), expected_map);
    let mut expected_core = expected_map.clone();
    expected_core.extend(
        area_slugs
            .iter()
            .zip(core_slugs)
            .map(|(area, core)| (records[area].uid.clone(), records[core].uid.clone())),
    );
    routes::extend_core(&records, &mut expected_core);
    assert_eq!(
        learning_edges(&publication, "hephaestus-core"),
        expected_core
    );
    let selected = selected_nodes(&expected_core);
    assert_eq!(selected.len(), 644);
    assert_eq!(reachable_nodes(&root, &expected_core), selected);
    for excluded in [
        "engineering-source-human-factors",
        "computing-source-interaction",
        "computing-source-accessibility",
        "computing-source-protection",
        "computing-source-privacy",
        "physics-source-quantum",
        "math-source-operators",
        "inquiry-source-causal",
        "computing-source-abstraction",
        "rc-rse",
        "mat-mit-fundamentals",
        "gc-webbook",
        "th-devoe",
        "mat-tong-statistical",
        "math-source-analysis",
        "inquiry-source-learning",
        "inquiry-source-assessment",
        "inquiry-source-retrieval",
        "engineering-source-production",
        "engineering-source-little",
        "computing-source-tcp",
        "computing-source-rpc",
        "chemistry-source-organic",
        "math-source-discrete",
        "engineering-source-process-control",
        "engineering-source-digital-control",
        "water-and-sanitation",
        "computing-source-structures",
        "gc-harvey",
        "em-openstax",
        "th-yan",
        "thermodynamics-and-heat-transfer",
        "electromagnetism",
        "ic-calculus",
        "industrial-chemistry",
        "inquiry-source-probability",
        "mech-npl-drawings",
        "ql-source-blueprint",
        "mech-desenho-pt",
        "computing-source-algorithms",
        "pe-linear-algebra",
        "electrical-power-engineering",
        "math-source-proof",
        "ql-source-algebra",
        "ql-source-prealgebra",
        "quantitative-literacy",
        "ql-sources",
        "fd-openstax",
        "fluid-dynamics",
        "rc-python",
        "rc-sundnes",
        "reproducible-computation",
        "gc-openstax",
        "general-chemistry",
        "met-bell",
        "met-handbook",
        "ql-source-vim",
        "rc-practices",
        "rc-peng",
        "metrology",
        "mech-nasa-systems",
        "mech-slocum",
        "mech-statics-book",
        "mechanical-engineering",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    let engineering = learning_edges(&publication, "engineering");
    for nested in [
        "resources-materials-map",
        "utilities",
        "production-quality-map",
    ] {
        assert!(engineering.contains(&(
            records["engineering-map"].uid.clone(),
            records[nested].uid.clone(),
        )));
        assert!(!selected.contains(&records[nested].uid));
    }
    assert!(!selected.contains(&records["clean-manufacturing"].uid));
    assert_eq!(publication.documents.len(), 3752);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    assert_eq!(identities.len(), 3752);
    for document in publication.documents.iter().filter(|document| {
        document.project.records.iter().any(|record| {
            record.slug.as_deref().is_some_and(|slug| {
                [
                    "math-source-proof",
                    "physics-spatial-review",
                    "math-structures-review",
                ]
                .contains(&slug)
            })
        })
    }) {
        let from_entry = Publication::load(&library, &document.uri, &[], false).unwrap();
        let discovered: BTreeSet<_> = all_records(&from_entry)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(discovered, identities, "{}", document.uri);
    }
    let standalone = load(
        &library,
        &directory.join("inquiry/quantitative-literacy/foundations/Quantity-and-reference.lingua"),
    );
    assert_eq!(standalone.documents.len(), 53);
    let original_quantitative = learning_edges(&standalone, "quantitative-literacy");
    assert_eq!(
        learning_edges(&publication, "quantitative-literacy"),
        original_quantitative
    );
    let mut shared_routes = BTreeMap::new();
    for (entry, predicate, count) in [
        (
            "chemistry/general-chemistry/sources/Analytical-Chemistry-Harvey.lingua",
            "general-chemistry",
            319,
        ),
        (
            "physics/electromagnetism/sources/University-Physics-Volume-2.lingua",
            "electromagnetism",
            346,
        ),
        (
            "physics/thermodynamics-and-heat-transfer/sources/Introduction-to-Engineering-Thermodynamics-Yan.lingua",
            "thermodynamics-and-heat-transfer",
            623,
        ),
        (
            "chemistry/industrial-chemistry/sources/Calculus-and-Differential-Equations-OpenStax.lingua",
            "industrial-chemistry",
            997,
        ),
        (
            "engineering/electrical-power-engineering/sources/Introduction-to-Applied-Linear-Algebra-Boyd.lingua",
            "electrical-power-engineering",
            525,
        ),
        (
            "inquiry/metrology/foundations/Question-and-measurand.lingua",
            "metrology",
            132,
        ),
        (
            "inquiry/reproducible-computation/foundations/Files-paths-and-processes.lingua",
            "reproducible-computation",
            212,
        ),
        (
            "chemistry/general-chemistry/foundations/A-chemical-question.lingua",
            "general-chemistry",
            319,
        ),
        (
            "engineering/mechanical-engineering-and-machine-design/statics/Forces-are-interactions-across-the-chosen-boundary.lingua",
            "mechanical-engineering",
            1018,
        ),
        (
            "physics/fluid-dynamics/sources/University-Physics-Volume-1.lingua",
            "fluid-dynamics",
            802,
        ),
        (
            "utilities/water-and-sanitation/operations/A-tank-balance-explains-control-offset-delay-and-saturation.lingua",
            "water-and-sanitation",
            1247,
        ),
    ] {
        let original = load(&library, &directory.join(entry));
        assert_eq!(original.documents.len(), count);
        assert!(!original.documents.iter().any(|document| {
            document
                .project
                .records
                .iter()
                .any(|record| record.slug.as_deref() == Some("hephaestus"))
        }));
        let edges = learning_edges(&original, predicate);
        assert!(!edges.is_empty());
        assert_eq!(learning_edges(&publication, predicate), edges);
        shared_routes.insert(predicate, edges);
    }
    let floating_program = records["rc-floating"]
        .body
        .split("```python file=floating.py\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap()
        .to_owned();
    assert!(
        records["computing-map"]
            .body
            .contains("**Developed sixth common foundation:** concurrent actors and atomic steps")
    );
    assert!(
        records["engineering-map"]
            .body
            .contains("**Developed sixth common foundation:** feedback boundaries")
    );
    assert!(
        records["inquiry-map"]
            .body
            .contains("**Developed sixth common foundation:** scientific questions and audiences")
    );
    assert!(
        records["mathematics-map"]
            .body
            .contains("**Developed seventh common foundation:** finite objects and equality")
    );
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed seventh common foundation:** orientation and angular motion")
    );
    assert!(records["chemistry-map"].body.contains(
        "**Developed seventh common foundation:** molecular identity across composition"
    ));
    assert!(
        records["computing-map"]
            .body
            .contains("**Developed seventh common foundation:** communication boundaries")
    );
    assert!(
        records["engineering-map"]
            .body
            .contains("**Developed seventh common foundation:** useful production")
    );
    let learning_link = regex::Regex::new(r"(\[[^\]\n]*\]\()[^)\n]+#(r_[A-Z0-9]+)\)").unwrap();
    let learning_body = |body: &str| learning_link.replace_all(body, "$1#$2)").into_owned();
    let learning_material: Vec<_> = routes::LEARNING
        .iter()
        .skip(1)
        .map(|slug| (*slug, learning_body(&records[slug].body)))
        .collect();
    assert!(
        records["inquiry-learning-assessment"]
            .body
            .contains("| O | 1, 2 | 0, 1 | 2 | 0 |")
    );
    assert!(
        records["inquiry-learning-study"]
            .body
            .contains("fork-and-join dependency")
    );
    assert!(records["inquiry-learning-scoring"].body.contains("(1,1,0)"));
    assert!(
        records["inquiry-map"]
            .body
            .contains("**Developed seventh common foundation:** learning capabilities")
    );
    assert!(
        records["mathematics-map"]
            .body
            .contains("**Developed eighth common foundation:** sequences")
    );
    assert!(
        records["ic-calculus"]
            .body
            .contains("**Eighth mathematics-core inspection")
    );
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed eighth common foundation:** microscopic states")
    );
    assert!(
        records["mat-tong-statistical"]
            .body
            .contains("**Eighth physics-core inspection")
    );
    assert!(
        records["chemistry-map"]
            .body
            .contains("**Developed eighth common foundation:** phases and constraints")
    );
    for slug in ["th-devoe", "gc-webbook", "mat-mit-fundamentals"] {
        assert!(
            records[slug]
                .body
                .contains("**Eighth chemistry-core inspection")
        );
    }
    assert!(
        records["computing-map"]
            .body
            .contains("**Developed eighth common foundation:** names and captured environments")
    );
    assert!(
        records["rc-modules"]
            .body
            .contains("**Eighth computing-block reading:")
    );
    for slug in ["rc-python", "rc-rse"] {
        assert!(
            records[slug]
                .body
                .contains("**Eighth computing-core inspection")
        );
    }
    assert!(
        records["engineering-map"]
            .body
            .contains("**Developed eighth common foundation:** operating histories")
    );
    for slug in [
        "mech-maintenance",
        "mech-condition-evidence",
        "mech-failure-investigation",
        "mech-restoration",
    ] {
        assert!(
            records[slug]
                .body
                .contains("**Eighth engineering-block reading:")
        );
    }
    for slug in ["met-handbook", "mech-nasa-systems"] {
        assert!(
            records[slug]
                .body
                .contains("**Eighth engineering-core inspection")
        );
    }
    assert!(
        records["inquiry-map"]
            .body
            .contains("**Developed eighth common foundation:** causal questions")
    );
    assert!(
        records["inquiry-source-causal"]
            .body
            .contains("19 August 2026")
    );
    assert!(
        records["mathematics-map"]
            .body
            .contains("**Developed common operator bridge:**")
    );
    assert!(
        records["math-source-operators"]
            .body
            .contains("30 October 2026")
    );
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed common quantum foundations:**")
    );
    assert!(
        records["physics-source-quantum"]
            .body
            .contains("17 September 2013")
    );
    assert!(
        records["computing-map"]
            .body
            .contains("**Developed common human-use and trust foundations:**")
    );
    assert!(
        records["computing-source-accessibility"]
            .body
            .contains("12 December 2024")
    );
    assert!(
        records["engineering-map"]
            .body
            .contains("**Developed common people and operation foundations:**")
    );
    assert!(
        records["engineering-source-human-factors"]
            .body
            .contains("Open Government Licence")
    );
    assert!(
        records["mathematics-map"]
            .body
            .contains("**Developed common oriented-field foundations:**")
    );
    assert!(!selected.contains(&records["em-vector-book"].uid));
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed common magnetic and inductive foundations:**")
    );
    assert!(!selected.contains(&records["em-tong"].uid));
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed common spacetime and gravity foundations:**")
    );
    for slug in [
        "physics-source-dynrel",
        "physics-source-optical-clocks",
        "physics-source-equivalence",
    ] {
        assert!(!selected.contains(&records[slug].uid));
    }
    assert!(
        records["physics-map"]
            .body
            .contains("**Developed common astronomical foundations:**")
    );
    for slug in [
        "physics-source-astronomy",
        "physics-source-gaia-distances",
        "physics-source-hubble1929",
    ] {
        assert!(!selected.contains(&records[slug].uid));
    }
    let structure_slugs = [
        "math-structures",
        "math-structures-language",
        "math-structures-models",
        "math-structures-consequence",
        "math-structures-proof",
        "math-structures-operations",
        "math-structures-maps",
        "math-structures-quotients",
        "math-structures-evidence",
        "math-structures-study",
        "math-structures-review",
    ];
    let structure_edges: BTreeSet<_> = structure_slugs
        .array_windows()
        .map(|[from, to]| (records[from].uid.clone(), records[to].uid.clone()))
        .collect();
    assert_eq!(
        learning_edges(&publication, "math-structures"),
        structure_edges
    );
    let mathematics_branches = BTreeSet::from([(
        records["mathematics-map"].uid.clone(),
        records["math-structures"].uid.clone(),
    )]);
    assert_eq!(
        learning_edges(&publication, "mathematics-branches"),
        mathematics_branches
    );
    for slug in structure_slugs
        .into_iter()
        .chain(["math-source-forallx", "math-source-judson"])
    {
        assert!(!selected.contains(&records[slug].uid));
    }
    let original_sources = external_references(&publication);
    let contextual_links = |publication: &Publication| {
        let included: BTreeSet<_> = publication
            .documents
            .iter()
            .map(|document| document.base.as_str())
            .collect();
        let mut links = BTreeSet::new();
        for document in &publication.documents {
            let base = lince_resources::Url::parse(&document.base).unwrap();
            for record in &document.project.records {
                for target in record
                    .body
                    .split("](")
                    .skip(1)
                    .filter_map(|value| value.split_once(')').map(|(target, _)| target))
                {
                    let resolved = base.join(target).unwrap();
                    let mut location = resolved.clone();
                    location.set_fragment(None);
                    if !included.contains(location.as_str()) {
                        links.insert(resolved.to_string());
                    }
                }
            }
        }
        links
    };
    let original_context = contextual_links(&publication);
    assert!(!original_context.is_empty());
    let (reopened, _) = relocate(&publication, &library, temporary.path());
    assert_eq!(learning_edges(&reopened, "hephaestus"), expected_map);
    assert_eq!(learning_edges(&reopened, "hephaestus-core"), expected_core);
    assert_eq!(reopened.documents.len(), 3752);
    programs::assert_relocated(&records, &reopened);
    assert_eq!(
        learning_edges(&reopened, "math-structures"),
        structure_edges
    );
    assert_eq!(
        learning_edges(&reopened, "mathematics-branches"),
        mathematics_branches
    );
    assert_eq!(
        learning_edges(&reopened, "quantitative-literacy"),
        original_quantitative
    );
    assert_eq!(contextual_links(&reopened), original_context);
    assert_eq!(external_references(&reopened), original_sources);
    assert!(
        all_records(&reopened).any(|record| record.slug.as_deref() == Some("rc-floating")
            && record.body.contains(&floating_program))
    );
    for (slug, body) in learning_material {
        let restored = all_records(&reopened)
            .find(|record| record.slug.as_deref() == Some(slug))
            .unwrap();
        assert_eq!(learning_body(&restored.body), body, "{slug}");
    }
    for (predicate, original) in shared_routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    assert_local_targets(&reopened, |assertion| {
        [
            "contains",
            "math-structures",
            "mathematics-branches",
            "hephaestus",
            "hephaestus-core",
            "quantitative-literacy",
            "mechanical-engineering",
            "fluid-dynamics",
            "general-chemistry",
            "reproducible-computation",
            "metrology",
            "electrical-power-engineering",
            "water-and-sanitation",
        ]
        .contains(&assertion.predicate.as_str())
    });
    assert!(reopened.resources().unwrap().is_empty());
}
