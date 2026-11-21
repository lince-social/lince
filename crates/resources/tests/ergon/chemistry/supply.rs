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
        trails.join("ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/chemistry/industrial-chemistry"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
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
        "Chemical-feedstocks-and-supply-systems.lingua",
        "inventory/An-inspectable-lot-release-and-expiry-account.lingua",
        "inventory/An-inspectable-exact-replenishment-demand-distribution.lingua",
        "research/Advanced-common-dependency-and-restoration-study.lingua",
        "sources/Inventory-Analytics-Rossi.lingua",
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
        &directory.join("Chemical-feedstocks-and-supply-systems.lingua"),
    );
    let records = records_by_slug(&publication);
    assert_eq!(
        records["chemical-supply-systems"].uid,
        "r_05V6FYWMN0VND552CQE94HHA2Q"
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
        "industrial-chemistry",
        "chemical-supply-systems",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["chemical-supply-systems"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("cs-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "cs-quantity", "quantitative-literacy"),
        ("ic-specification", "cs-agreement", "industrial-chemistry"),
        ("ic-air-gases", "cs-gas-supply", "industrial-chemistry"),
        (
            "ic-chloralkali-process",
            "cs-coproduct",
            "industrial-chemistry",
        ),
        ("ic-methanol", "cs-organic-supply", "industrial-chemistry"),
        ("ic-monomers", "cs-stabilized-feed", "industrial-chemistry"),
        ("met-sampling-target", "cs-receiving-sampling", "metrology"),
        ("met-conformity", "cs-release", "metrology"),
        (
            "ic-transfers",
            "cs-storage-envelope",
            "industrial-chemistry",
        ),
        (
            "ic-equipment",
            "cs-contact-materials",
            "industrial-chemistry",
        ),
        (
            "ql-balances",
            "cs-inventory-account",
            "quantitative-literacy",
        ),
        (
            "rc-numericaltests",
            "cs-lot-example",
            "reproducible-computation",
        ),
        ("ic-utilities", "cs-dependencies", "industrial-chemistry"),
        ("ic-recovery", "cs-circular", "industrial-chemistry"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in chemical supply systems:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["chemical-supply-systems"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 104);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "industrial-chemistry",
        "cs-sources",
        "cs-rossi",
        "cs-warehouse",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "chemical-supply-systems" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "cs-lot-example" => "lot_release_expiry.py",
                    "cs-lead-example" => "exact_replenishment_demand.py",
                    "cs-dual-source" => "two_source_reservation.py",
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
