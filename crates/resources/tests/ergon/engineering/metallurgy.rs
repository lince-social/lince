// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
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
        "Metals-and-metallurgical-production.lingua",
        "balances/An-inspectable-three-feed-charge.lingua",
        "solidification/An-inspectable-equilibrium-and-Scheil-comparison.lingua",
        "research/Advanced-thermodynamic-diffusion-and-phase-field-model-study.lingua",
        "sources/Phase-Transitions-Bhadeshia-Yan.lingua",
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
        &directory.join("Metals-and-metallurgical-production.lingua"),
    );
    let records = records_by_slug(&publication);
    assert_eq!(
        records["metals-and-metallurgical-production"].uid,
        "r_59BDAQXT81552YF5F8TH4KQQH1"
    );
    let predicates = [
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
    let edges = &routes["metals-and-metallurgical-production"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/materials/metals-and-metallurgical-production/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("metal-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        (
            "mine-handoffs",
            "metal-feed",
            "mining-and-mineral-engineering",
        ),
        ("ql-balances", "metal-balance", "quantitative-literacy"),
        (
            "gc-chemical-potential",
            "metal-activities",
            "general-chemistry",
        ),
        (
            "mp-lever",
            "metal-phase-diagrams",
            "mineralogy-and-petrology",
        ),
        ("gc-transport", "metal-transport", "general-chemistry"),
        (
            "rc-numericaltests",
            "metal-diffusion-example",
            "reproducible-computation",
        ),
        (
            "mine-calcination",
            "metal-roasting",
            "mining-and-mineral-engineering",
        ),
        ("gc-nernst", "metal-electrode-kinetics", "general-chemistry"),
        (
            "gc-electrolysis",
            "metal-current-efficiency",
            "general-chemistry",
        ),
        (
            "mp-mapping",
            "metal-metallography",
            "mineralogy-and-petrology",
        ),
        (
            "mp-microprobe",
            "metal-chemical-analysis",
            "mineralogy-and-petrology",
        ),
        ("met-force-pressure", "metal-tensile", "metrology"),
        (
            "mine-accounting",
            "metal-plant-accounting",
            "mining-and-mineral-engineering",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in metals and metallurgical production:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["metals-and-metallurgical-production"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 136);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metal-sources",
        "metal-phase-book",
        "metal-extract-course",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "metals-and-metallurgical-production" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "metal-charge-example" => "three_feed_charge.py",
                    "metal-diffusion-example" => "periodic_diffusion.py",
                    "metal-segregation-example" => "segregation_limits.py",
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
