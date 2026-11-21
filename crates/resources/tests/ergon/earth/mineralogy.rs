// non-lean-yet

use std::collections::{BTreeMap, BTreeSet};

use lince_resources::{Library, publication::Publication};

use crate::ergon::fixture::{
    all_records, assert_local_targets, external_references, learning_edges, load, reachable_nodes,
    records_by_slug, relocate, selected_nodes, trails,
};

pub(crate) fn run() {
    let trails = trails();
    let directory = trails.join("ergon/gaia/earth/mineralogy-and-petrology");
    let expected_documents = [
        directory.clone(),
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
        "Mineralogy-and-petrology.lingua",
        "crystals/Space-groups-settings-and-structural-models.lingua",
        "analysis/An-inspectable-oxide-normalization.lingua",
        "research/Advanced-metamorphic-model-and-petrochronology-study.lingua",
        "sources/Guide-to-Thin-Section-Microscopy.lingua",
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
    let publication = load(&library, &directory.join("Mineralogy-and-petrology.lingua"));
    let records = records_by_slug(&publication);
    assert_eq!(
        records["mineralogy-and-petrology"].uid,
        "r_44F7X6GPMW6ZSCCN6D71QSVKBN"
    );
    let predicates = [
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
    let edges = &routes["mineralogy-and-petrology"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/gaia/earth/mineralogy-and-petrology/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mp-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("geo-mineral-id", "mp-definition", "geology"),
        ("gc-crystals", "mp-lattice", "general-chemistry"),
        ("geo-rock-texture", "mp-modal", "geology"),
        ("met-chemistry-case", "mp-microprobe", "metrology"),
        ("gc-phases", "mp-phase-rule", "general-chemistry"),
        (
            "rc-numericaltests",
            "mp-equilibrium-example",
            "reproducible-computation",
        ),
        ("geo-igneous", "mp-magma", "geology"),
        ("geo-metamorphic", "mp-assemblages", "geology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in mineralogy and petrology:")
        );
    }
    let selected = selected_nodes(edges);
    assert_eq!(selected, expected);
    assert_eq!(
        reachable_nodes(&records["mineralogy-and-petrology"].uid, edges),
        selected
    );
    assert_eq!(selected.len(), 97);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mp-sources",
        "mp-mineralogy",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "mineralogy-and-petrology" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        all_records(publication)
            .filter_map(|record| {
                let slug = record.slug.as_deref()?;
                let file = match slug {
                    "mp-formula-example" => "mineral_formula.py",
                    "mp-equilibrium-example" => "two_phase.py",
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
    assert_eq!(original_examples.len(), 2);
    assert!(
        original_examples["mineral_formula.py"].contains("def normalize(oxides, oxygen_basis):")
    );
    assert!(
        original_examples["two_phase.py"].contains("def equilibrate(bulk, offset=math.log(4)):")
    );
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
