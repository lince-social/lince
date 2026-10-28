#![cfg(feature = "publication")]

use lince_resources::{Library, Resource, publication::Publication};
use std::{fs, path::PathBuf};

fn philosophy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/philosophy/Philosophy.lingua")
}

#[test]
fn philosophy_is_a_navigable_graph_of_separate_ordinary_documents() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path());
    let content_documents = Publication::files(philosophy().parent().unwrap())
        .unwrap()
        .len();
    for (file, title) in [
        ("Philosophy.lingua", "Philosophy"),
        (
            "ancient-greek-philosophy/Philosophers.lingua",
            "Ancient Greek philosophers",
        ),
        ("ancient-greek-philosophy/plato/Plato.lingua", "Plato"),
    ] {
        let source = Publication::load(
            &library,
            philosophy().with_file_name(file).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(source.documents.len(), content_documents);
        assert_eq!(source.documents[0].project.records.len(), 1);
        let record = &source.documents[0].project.records[0];
        assert_eq!(record.head, title);
        assert!(!record.body.is_empty());
        assert!(!record.assertions.is_empty());
        assert!(source.parameters.is_empty());
        assert!(source.instances.is_empty());
    }
    let book = Publication::load(
        &library,
        philosophy()
            .with_file_name("ancient-greek-philosophy/sources/Socratic-dialogues-Cary.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let resources = book.resources().unwrap();
    for uid in [
        "r_00000000000000000000001005",
        "r_00000000000000000000001006",
        "r_00000000000000000000001007",
        "r_00000000000000000000001008",
    ] {
        assert!(resources.iter().any(|(record, _)| record == uid));
    }
    for (name, expected_documents) in [
        ("Development-example.lingua", content_documents + 1),
        ("Checklist-example.lingua", content_documents + 1),
        ("Assessment-example.lingua", 1),
    ] {
        let path = philosophy()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("shared")
            .join(name);
        let scheme = Publication::load(&library, path.to_str().unwrap(), &[], false).unwrap();
        assert_eq!(scheme.documents.len(), expected_documents);
        assert!(!scheme.instances.is_empty());
        assert!(
            scheme
                .documents
                .iter()
                .any(|document| !document.project.rules.is_empty())
        );
    }
}

#[test]
fn several_definitions_can_reference_the_same_nearby_image() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("Root.lingua");
    fs::write(&root, "Root (@root: 0, #contains @child) { r_00000000000000000000008801\n![First caption](figure.png)\n} r_00000000000000000000008801\nExtension @root \"lince.source\" { dependencies [{ uri \"Child.lingua\" }] }").unwrap();
    fs::write(
        temporary.path().join("Child.lingua"),
        "Child (@child: 0) { r_00000000000000000000008802\n![Second caption](figure.png)\n} r_00000000000000000000008802",
    )
    .unwrap();
    let library = Library::new(temporary.path().join("library"));
    let source = Publication::load(&library, root.to_str().unwrap(), &[], false).unwrap();
    let resources = source.resources().unwrap();
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].1.uri, resources[1].1.uri);
    assert_ne!(resources[0].0, resources[1].0);
}

#[test]
fn source_revisions_remain_available_by_hash_after_a_local_file_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("Source.lingua");
    let library = Library::new(temporary.path().join("library"));
    let uri = lince_resources::address(path.to_str().unwrap())
        .unwrap()
        .to_string();
    fs::write(&path, "First source revision").unwrap();
    let original = library.source_text(&uri, 1024, false).unwrap();
    let hash = lince_resources::fingerprint(original.as_bytes());
    fs::write(&path, "Second source revision").unwrap();
    assert_eq!(
        library.source_text(&uri, 1024, true).unwrap(),
        "Second source revision"
    );
    let previous = library
        .find(&Resource {
            uri: uri.clone(),
            hash: Some(hash),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
    assert_eq!(fs::read_to_string(previous.path).unwrap(), original);
    fs::write(&path, &original).unwrap();
    assert_eq!(library.source_text(&uri, 1024, true).unwrap(), original);
    let current = library
        .find(&Resource {
            uri,
            ..Default::default()
        })
        .unwrap()
        .unwrap();
    assert_eq!(fs::read_to_string(current.path).unwrap(), original);
}

#[test]
fn offline_exports_pin_resources_that_did_not_declare_a_hash() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("Root.lingua");
    fs::write(&path, "Reference (@reference: 0) { r_00000000000000000000008803\n} r_00000000000000000000008803\nExtension @reference \"lince.resource\" { uri \"data.bin\" }\n").unwrap();
    fs::write(temporary.path().join("data.bin"), b"original data").unwrap();
    let library = Library::new(temporary.path().join("library"));
    let source = Publication::load(&library, path.to_str().unwrap(), &[], false).unwrap();
    let entry = source
        .export(&library, &temporary.path().join("export"), true)
        .unwrap();
    let snapshot = Publication::load(&library, entry.to_str().unwrap(), &[], false).unwrap();
    let resource = snapshot.resources().unwrap().remove(0).1;
    assert_eq!(
        resource.hash.as_deref(),
        Some(lince_resources::fingerprint(b"original data").as_str())
    );
    fs::write(
        lince_resources::address(&resource.uri)
            .unwrap()
            .to_file_path()
            .unwrap(),
        b"changed data",
    )
    .unwrap();
    assert!(
        Library::new(temporary.path().join("another-cell"))
            .fetch(&resource)
            .is_err()
    );
}

#[test]
fn export_moves_without_losing_the_book_or_declared_dependencies() {
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let source_path = philosophy()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("shared/Development-example.lingua");
    let source = Publication::load(&library, source_path.to_str().unwrap(), &[], false).unwrap();
    let destination = temporary.path().join("export");
    let exported = source.export(&library, &destination, true).unwrap();
    let entry_name = exported.strip_prefix(&destination).unwrap().to_owned();
    assert!(!destination.join("Root.lingua").exists());
    let moved = temporary.path().join("moved");
    fs::rename(&destination, &moved).unwrap();
    let local = Publication::load(
        &library,
        moved.join(&entry_name).to_str().unwrap(),
        &["../shared/Development-example.lingua".into()],
        false,
    )
    .unwrap();
    assert_eq!(local.documents.len(), source.documents.len());
    let origin = local.documents[0]
        .project
        .extensions
        .iter()
        .find(|extension| extension.namespace == lince_resources::SOURCE_NAMESPACE)
        .unwrap()
        .fields["origin"]
        .clone();
    assert_eq!(origin["entry"], source.entry);
    assert_eq!(origin["document"], source.documents[0].uri);
    assert_eq!(
        origin["hash"],
        lince_resources::fingerprint(&fs::read(&source_path).unwrap())
    );
    for (_, resource) in local.resources().unwrap() {
        assert!(
            lince_resources::address(&resource.uri)
                .unwrap()
                .to_file_path()
                .unwrap()
                .is_file()
        );
        assert!(!resource.source_uri.is_empty());
        assert!(library.fetch(&resource).unwrap().bytes > 0);
    }
}

#[test]
fn dependency_cycles_terminate_and_missing_required_documents_fail() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path().join("library"));
    fs::write(dir.path().join("A.lingua"),"A (@a: 0) { r_00000000000000000000000001\n} r_00000000000000000000000001\nExtension @a \"lince.source\" { dependencies [{ uri \"B.lingua\" }] }").unwrap();
    fs::write(dir.path().join("B.lingua"),"B (@b: 0) { r_00000000000000000000000002\n} r_00000000000000000000000002\nExtension @b \"lince.source\" { dependencies [{ uri \"A.lingua\" }] }").unwrap();
    assert_eq!(
        Publication::load(
            &library,
            dir.path().join("A.lingua").to_str().unwrap(),
            &[],
            false
        )
        .unwrap()
        .documents
        .len(),
        2
    );
    fs::remove_file(dir.path().join("B.lingua")).unwrap();
    assert!(
        Publication::load(
            &library,
            dir.path().join("A.lingua").to_str().unwrap(),
            &[],
            false
        )
        .is_err()
    );
}

#[test]
fn registered_content_is_reused_by_hash_and_rejects_a_wrong_identity() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::new(dir.path().join("library"));
    let path = dir.path().join("original");
    fs::write(&path, b"the same original").unwrap();
    let first = library
        .register(
            &path,
            "https://one.invalid/book",
            "application/epub+zip",
            None,
        )
        .unwrap();
    let second = library
        .fetch(&Resource {
            uri: "https://two.invalid/book".into(),
            hash: Some(first.hash.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.path, second.path);
    assert!(
        library
            .register(
                &path,
                "https://one.invalid/other",
                "",
                Some(&"0".repeat(64))
            )
            .is_err()
    );
}

#[test]
fn every_philosophy_node_can_reach_the_other_known_nodes_without_an_index() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let content = philosophy();
    let root = content.parent().unwrap();
    let audit = Publication::audit(&library, root, &[]).unwrap();
    assert_eq!(audit.documents, Publication::files(root).unwrap().len());
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
}

#[test]
fn exports_and_audits_preserve_wiki_links_and_transclusions() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("A.lingua"), "Import \"B.lingua\"\nA (@a: 0) { r_00000000000000000000008811\n[[B|B.lingua#r_00000000000000000000008812]]\n\n![[B.lingua#r_00000000000000000000008812]]\n} r_00000000000000000000008811").unwrap();
    fs::write(root.join("B.lingua"), "B (@b: 0) { r_00000000000000000000008812\n[[A|A.lingua#r_00000000000000000000008811]]\n} r_00000000000000000000008812").unwrap();
    let library = Library::new(temporary.path().join("library"));
    let source = Publication::load(
        &library,
        root.join("A.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let output = temporary.path().join("export");
    let entry = source.export(&library, &output, false).unwrap();
    let name = entry.strip_prefix(&output).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(output, &moved).unwrap();
    fs::remove_dir_all(root).unwrap();
    let exported =
        Publication::load(&library, moved.join(name).to_str().unwrap(), &[], false).unwrap();
    assert_eq!(exported.documents.len(), 2);
    assert!(exported.documents[0].raw.contains("[[B|B.lingua#"));
    assert!(exported.documents[0].raw.contains("![[B.lingua#"));
    let report = Publication::audit(&library, &moved, &[]).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.links, 4);
}
