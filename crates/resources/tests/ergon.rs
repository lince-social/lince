#![cfg(feature = "publication")]

use lince_resources::{Library, publication::Publication};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

#[test]
fn mining_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering");
    let expected_documents = [
        directory.clone(),
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
        "Mining-and-mineral-engineering.lingua",
        "planning/An-exhaustively-checkable-small-mine-schedule.lingua",
        "processing/Flotation-kinetics-residence-time-and-parameter-evidence.lingua",
        "research/Advanced-waste-water-tailings-and-closure-study.lingua",
        "sources/Tratamento-de-Minerios-CETEM.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Mining-and-mineral-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["mining-and-mineral-engineering"].uid,
        "r_22HJPS3RTK5EX1J55YJP75PHYA"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["mining-and-mineral-engineering"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/engineering/mining-and-mineral-engineering/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mine-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("geo-maps", "mine-survey", "geology"),
        ("met-sampling-target", "mine-sampling", "metrology"),
        (
            "mp-liberation",
            "mine-geometallurgy",
            "mineralogy-and-petrology",
        ),
        (
            "rc-numericaltests",
            "mine-schedule-example",
            "reproducible-computation",
        ),
        ("geo-stress-strain", "mine-rock-mass", "geology"),
        ("geo-groundwater-flow", "mine-dewatering", "geology"),
        ("ql-balances", "mine-flowsheet", "quantitative-literacy"),
        (
            "mp-tailings",
            "mine-waste-geochemistry",
            "mineralogy-and-petrology",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in mining and mineral engineering:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mining-and-mineral-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 116);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mine-sources",
        "mine-psu",
        "mine-haul-road-manual",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "mining-and-mineral-engineering" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "mine-schedule-example" => "mine_schedule.py",
                    "mine-network-example" => "ventilation_network.py",
                    "mine-balance-example" => "two_products.py",
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn mineralogy_petrology_preserves_shared_learning_and_two_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Mineralogy-and-petrology.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mineralogy-and-petrology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn quantitative_readings_discover_relocate_and_select_without_absorbing_sources() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry/quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let files = Publication::files(&directory).unwrap();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert_eq!(audit.documents, files.len());
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
        "Quantitative-literacy.lingua",
        "foundations/Units-and-dimensions.lingua",
        "models/Dimensionless-groups.lingua",
        "study/Advanced-quantitative-study.lingua",
        "sources/SI-Brochure.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), files.len(), "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Quantitative-literacy.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.uid.clone(), record))
        .collect();
    let mut expected = BTreeSet::new();
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    for document in &publication.documents {
        let path = PathBuf::from(document.uri.strip_prefix("file://").unwrap());
        if path.parent().unwrap().file_name().unwrap() != "sources" {
            for record in &document.project.records {
                if record.slug.as_deref() != Some("ql-sources") {
                    expected.insert(record.uid.clone());
                }
            }
        }
        for record in &document.project.records {
            for assertion in &record.assertions {
                if assertion.predicate == "quantitative-literacy" {
                    let target = assertion
                        .object_uri
                        .as_ref()
                        .unwrap()
                        .rsplit_once('#')
                        .unwrap()
                        .1;
                    assert!(records.contains_key(target), "{target}");
                    edges
                        .entry(record.uid.clone())
                        .or_default()
                        .insert(target.to_owned());
                }
            }
        }
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| std::iter::once(from.clone()).chain(to.iter().cloned()))
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec!["r_4XVN56PQC13JGNFPV0CSGXDCZ2".to_owned()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            if let Some(next) = edges.get(&uid) {
                pending.extend(next.iter().cloned());
            }
        }
    }
    assert_eq!(reached, selected);
    let external_links: BTreeSet<_> = records
        .values()
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect();
    assert!(!external_links.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("relocated");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let relocated_identities: BTreeSet<_> = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| record.uid.clone())
        .collect();
    assert_eq!(relocated_identities, records.keys().cloned().collect());
    let relocated_links: BTreeSet<_> = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect();
    assert_eq!(relocated_links, external_links);
    assert!(!moved.join("files").exists());
}

#[test]
fn geology_reuses_foundations_and_preserves_its_selection_and_geometry_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/gaia/earth/geology");
    let expected_documents = [
        directory.clone(),
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
        "Geology.lingua",
        "field/Field-records-and-specimen-identity.lingua",
        "structures/An-inspectable-three-point-plane.lingua",
        "research/Independent-geological-scholarship.lingua",
        "sources/Geodiversidade-do-Brasil.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Geology.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(records["geology"].uid, "r_3CK43VQS79NK0STWNPSCS8YM06");
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["geology"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| document.uri.contains("/ergon/gaia/earth/geology/"))
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("geo-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-graphs", "geo-maps", "quantitative-literacy"),
        ("gc-crystals", "geo-mineral-id", "general-chemistry"),
        ("met-sampling-target", "geo-sampling", "metrology"),
        ("gc-nuclear", "geo-age", "general-chemistry"),
        (
            "rc-numericaltests",
            "geo-plane-example",
            "reproducible-computation",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in geology:"));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["geology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 80);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geo-sources",
        "geo-physical",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "geology" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| record.slug.as_deref() == Some("geo-plane-example"))
            .unwrap()
            .body
            .split("```python file=geological_plane.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def fit_plane(points):"));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(code(&reopened), original_code);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

fn learning_edges(publication: &Publication, predicate: &str) -> BTreeSet<(String, String)> {
    publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .assertions
                .iter()
                .filter(|assertion| assertion.predicate == predicate)
                .map(|assertion| {
                    (
                        record.uid.clone(),
                        assertion
                            .object_uri
                            .as_ref()
                            .unwrap()
                            .rsplit_once('#')
                            .unwrap()
                            .1
                            .to_owned(),
                    )
                })
        })
        .collect()
}

fn external_references(publication: &Publication) -> BTreeSet<String> {
    publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect()
}

#[test]
fn metrology_reuses_shared_steps_with_its_own_continuations_and_relocates_them() {
    let inquiry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry");
    let directory = inquiry.join("metrology");
    let quantitative = inquiry.join("quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let expected_documents = Publication::files(&directory).unwrap().len()
        + Publication::files(&quantitative).unwrap().len();
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
        "Metrology.lingua",
        "foundations/Question-and-measurand.lingua",
        "uncertainty/Multivariate-and-constraints.lingua",
        "research/Independent-research.lingua",
        "sources/GUM-and-Amendment.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Metrology.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let reused = [
        ("ql-quantity", "met-measurand"),
        ("ql-units", "met-si-realizations"),
        ("ql-repeated-observations", "met-rr"),
        ("ql-small-changes", "met-sensitivity"),
    ];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/metrology/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("met-sources"))
        .map(|record| record.uid.clone())
        .collect();
    let edges = learning_edges(&publication, "metrology");
    let quantitative_edges = learning_edges(&publication, "quantitative-literacy");
    for (shared, continuation) in reused {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(quantitative_edges.iter().any(|(from, _)| *from == uid));
        assert!(edges.is_disjoint(&quantitative_edges));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["metrology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert!(!selected.contains(&records["quantitative-literacy"].uid));
    assert!(!selected.contains(&records["met-gum"].uid));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(learning_edges(&reopened, "metrology"), edges);
    assert_eq!(
        learning_edges(&reopened, "quantitative-literacy"),
        quantitative_edges
    );
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if matches!(
                assertion.predicate.as_str(),
                "metrology" | "quantitative-literacy"
            ) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn reproducible_computation_preserves_shared_routes_and_inline_examples_after_relocation() {
    let inquiry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry");
    let directory = inquiry.join("reproducible-computation");
    let expected_documents = [
        &directory,
        &inquiry.join("quantitative-literacy"),
        &inquiry.join("metrology"),
    ]
    .into_iter()
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
        "Reproducible-computation.lingua",
        "data/A-complete-small-analysis.lingua",
        "numerics/Random-streams-and-task-identity.lingua",
        "research/Independent-investigation-and-revision.lingua",
        "sources/Scientific-Python-Sundnes.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Reproducible-computation.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["reproducible-computation"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/reproducible-computation/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("rc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-data", "rc-schema", "quantitative-literacy"),
        ("ql-sensitivity", "rc-conditioning", "quantitative-literacy"),
        (
            "ql-discretization",
            "rc-convergence",
            "quantitative-literacy",
        ),
        ("met-monte-carlo", "rc-streams", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in reproducible computation:")
        );
    }
    assert!(edges.is_disjoint(&routes["quantitative-literacy"]));
    assert!(edges.is_disjoint(&routes["metrology"]));
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["reproducible-computation"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "rc-sources",
        "rc-sundnes",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    let examples = |publication: &Publication| -> BTreeMap<String, Vec<String>> {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter(|record| record.body.contains("```"))
            .map(|record| {
                (
                    record.uid.clone(),
                    record
                        .body
                        .split("```")
                        .skip(1)
                        .step_by(2)
                        .map(str::to_owned)
                        .collect(),
                )
            })
            .collect()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 7);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(examples(&reopened), original_examples);
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn chemistry_discovers_shared_foundations_and_relocates_its_bounded_learning_selection() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/chemistry/general-chemistry");
    let expected_documents = [
        directory.clone(),
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
        "General-chemistry.lingua",
        "reactions/Reaction-extent-and-species-balances.lingua",
        "equilibrium/An-inspectable-equilibrium-solver.lingua",
        "research/Independent-scholarship-and-revision.lingua",
        "sources/Quimica-Geral-I-UECE.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("General-chemistry.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["general-chemistry"].uid,
        "r_206GS6CVXD8WSY1VX7PRF7136V"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["general-chemistry"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/chemistry/general-chemistry/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("gc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "gc-amount", "quantitative-literacy"),
        ("ql-balances", "gc-extent", "quantitative-literacy"),
        (
            "ql-concentrations",
            "gc-preparation",
            "quantitative-literacy",
        ),
        ("met-chemistry-case", "gc-spectra", "metrology"),
        ("rc-numericaltests", "gc-solver", "reproducible-computation"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in general chemistry:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["general-chemistry"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 92);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "gc-sources",
        "gc-openstax",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "general-chemistry" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| record.slug.as_deref() == Some("gc-solver"))
            .unwrap()
            .body
            .split("```python file=equilibrium.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def solve(added_base_ml):"));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(code(&reopened), original_code);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn metallurgy_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Metals-and-metallurgical-production.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["metals-and-metallurgical-production"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn industrial_chemistry_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/chemistry/industrial-chemistry");
    let expected_documents = [
        directory.clone(),
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
        "Industrial-chemistry.lingua",
        "balances/An-inspectable-recycle-and-purge-account.lingua",
        "separations/An-inspectable-fixed-ratio-flash.lingua",
        "research/Advanced-industrial-thermodynamics-and-phase-model-study.lingua",
        "sources/Chemical-Reactor-Analysis-Rawlings-Ekerdt.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Industrial-chemistry.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["industrial-chemistry"].uid,
        "r_3AKBAFB2ABJ0671QW6ZBQ0B618"
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
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["industrial-chemistry"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/chemistry/industrial-chemistry/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("ic-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "ic-quantities", "quantitative-literacy"),
        ("met-sampling-target", "ic-feed", "metrology"),
        ("ql-balances", "ic-elements", "quantitative-literacy"),
        ("gc-extent", "ic-stoichiometry", "general-chemistry"),
        ("gc-yield", "ic-yield", "general-chemistry"),
        (
            "rc-numericaltests",
            "ic-recycle-example",
            "reproducible-computation",
        ),
        (
            "gc-chemical-potential",
            "ic-reaction-equilibrium",
            "general-chemistry",
        ),
        ("gc-phases", "ic-phase-equilibrium", "general-chemistry"),
        ("gc-catalysis", "ic-catalyst-life", "general-chemistry"),
        ("gc-transport", "ic-scale-transfer", "general-chemistry"),
        (
            "gc-separations",
            "ic-separation-trains",
            "general-chemistry",
        ),
        ("met-chemistry-case", "ic-process-analysis", "metrology"),
        ("met-conformity", "ic-release", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in industrial chemistry:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["industrial-chemistry"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 140);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "ic-sources",
        "ic-noun-one",
        "ic-reactor-book",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "industrial-chemistry" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "ic-recycle-example" => "recycle_purge.py",
                    "ic-flash-example" => "fixed_ratio_flash.py",
                    "ic-cascade-example" => "consecutive_cascade.py",
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn chemical_supply_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Chemical-feedstocks-and-supply-systems.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["chemical-supply-systems"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn electromagnetism_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/physics/electromagnetism");
    let expected_documents = [
        directory.clone(),
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
        "Electromagnetism.lingua",
        "computation/An-inspectable-conservative-electrostatic-solver.lingua",
        "computation/An-inspectable-periodic-Yee-wave.lingua",
        "research/Advanced-time-domain-dispersion-PML-and-validation-study.lingua",
        "sources/Measuring-Permittivity-and-Permeability-NIST-TN-1536.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Electromagnetism.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["electromagnetism"].uid,
        "r_5TJPESCTTNAKY66JH2QNQYN1PY"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["electromagnetism"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/electromagnetism/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("em-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "em-constants", "quantitative-literacy"),
        ("ql-vectors", "em-vectorfields", "quantitative-literacy"),
        ("ql-small-changes", "em-calculus", "quantitative-literacy"),
        ("ql-rates", "em-continuity", "quantitative-literacy"),
        ("ql-energy", "em-potential", "quantitative-literacy"),
        ("met-electrical-case", "em-probing", "metrology"),
        ("met-time-frequency", "em-phasors", "metrology"),
        ("met-optical", "em-detection", "metrology"),
        (
            "ql-discretization",
            "em-numerical-fields",
            "quantitative-literacy",
        ),
        (
            "rc-numericaltests",
            "em-poisson-example",
            "reproducible-computation",
        ),
        ("met-model-discrepancy", "em-uncertainty", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in electromagnetism:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["electromagnetism"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 116);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "em-sources",
        "em-openstax",
        "em-nist1536",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "electromagnetism" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "em-poisson-example" => "electrostatic_faces.py",
                    "em-layer-example" => "dielectric_stack.py",
                    "em-yee-example" => "periodic_yee.py",
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn power_engineering_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Electrical-power-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["electrical-power-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn thermodynamics_preserves_six_routes_and_four_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Thermodynamics-and-heat-transfer.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["thermodynamics-and-heat-transfer"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn fluid_dynamics_preserves_seven_routes_and_four_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/physics/fluid-dynamics");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
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
        "Fluid-dynamics.lingua",
        "computation/An-inspectable-nozzle-with-an-internal-normal-shock.lingua",
        "computation/An-inspectable-conservative-wet-shallow-water-solver.lingua",
        "research/Advanced-rheometry-constitutive-inference-and-processing-flow-study.lingua",
        "sources/Mecanica-dos-Fluidos-e-Transportes-Gobbi-Dias.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Fluid-dynamics.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["fluid-dynamics"].uid,
        "r_7PQW1WDHK708BFV23MD8ET61VZ"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "general-chemistry",
        "thermodynamics-and-heat-transfer",
        "fluid-dynamics",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["fluid-dynamics"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/fluid-dynamics/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("fd-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "fd-vocabulary", "quantitative-literacy"),
        ("ql-vectors", "ql-small-changes", "quantitative-literacy"),
        ("ql-small-changes", "fd-fields", "quantitative-literacy"),
        (
            "ql-balances",
            "fd-transport-theorem",
            "quantitative-literacy",
        ),
        (
            "ql-dimensionless",
            "fd-dimensionless",
            "quantitative-literacy",
        ),
        ("ql-scaling", "fd-model-similarity", "quantitative-literacy"),
        (
            "ql-discretization",
            "fd-finite-volume",
            "quantitative-literacy",
        ),
        ("ql-identifiability", "fd-inverse", "quantitative-literacy"),
        ("met-force-pressure", "fd-pressure", "metrology"),
        ("met-mass-flow", "fd-flow-measurement", "metrology"),
        ("met-dynamic", "fd-dynamic-evidence", "metrology"),
        ("met-optical", "fd-piv-methods", "metrology"),
        ("met-model-discrepancy", "fd-uncertainty", "metrology"),
        (
            "rc-numericaltests",
            "fd-fluid-verification",
            "reproducible-computation",
        ),
        ("em-calculus", "fd-tensors", "electromagnetism"),
        (
            "th-control",
            "fd-mechanical-energy",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-local-entropy",
            "fd-compressibility",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-property-model",
            "fd-sound",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-fluid-energy",
            "fd-scalar-transport",
            "thermodynamics-and-heat-transfer",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in fluid dynamics:"));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["fluid-dynamics"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 167);
    for excluded in [
        "thermodynamics-and-heat-transfer",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "electromagnetism",
        "fd-sources",
        "fd-tong",
        "fd-rheology",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "fluid-dynamics" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "fd-pipe-example" => "pipe_operating_point.py",
                    "fd-nozzle-example" => "nozzle_with_shock.py",
                    "fd-blasius-example" => "blasius.py",
                    "fd-shallow-example" => "shallow_water.py",
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn mechanical_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
    let publication = Publication::load(
        &library,
        directory
            .join("Mechanical-engineering-and-machine-design.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
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
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mechanical-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-machines-and-mechanical-design-from-zero.lingua",
        "computation/An-inspectable-thermoelastic-axial-bar.lingua",
        "computation/An-inspectable-feed-axis-motion-and-energy-account.lingua",
        "sources/Precision-Machine-Design-Hale-Thesis.lingua",
        "research/Independent-mechanical-investigation-and-revision.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn materials_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
    let publication = Publication::load(
        &library,
        directory
            .join("Materials-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
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
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["materials-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-materials-from-everyday-objects-to-research.lingua",
        "computation/An-inspectable-cubic-diffraction-and-basis-account.lingua",
        "sources/Mesoscale-Tensile-Blind-Predictions-AM-Bench.lingua",
        "selection/An-inspectable-beam-selection-example.lingua",
        "research/Materials-engineering-completion-and-specialist-continuations.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn water_and_sanitation_preserves_its_selection_and_shared_routes_after_relocation() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/utilities/water-and-sanitation");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory
            .join("Water-sanitation-and-industrial-utilities.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let edges = learning_edges(&publication, "water-and-sanitation");
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
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
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["water-and-sanitation"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = original_examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn industrial_gases_preserves_the_complete_route_and_evidence_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
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
    let publication = Publication::load(
        &library,
        directory
            .join("Industrial-gases-and-vacuum.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    assert!(publication.documents.iter().all(
        |document| document.project.frequencies.is_empty() && document.project.rules.is_empty()
    ));
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
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
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
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
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["industrial-gases-and-vacuum"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
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
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
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
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
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
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str())
                || (assertion.predicate == "source" && assertion.object_uri.is_some())
            {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}
