use lince_resources::{Library, Reference};

#[test]
fn contextual_addresses_preserve_fragments_queries_and_native_paths() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let target = Reference::at(
        "../Parts.lingua?edition=2#r_00000000000000000000000001",
        "https://example.org/content/Plato.lingua",
    );
    let address = target.addresses(&library).unwrap().remove(0);
    assert_eq!(address.uri, "https://example.org/Parts.lingua?edition=2");
    assert_eq!(address.uid.as_deref(), Some("r_00000000000000000000000001"));
    let root = Reference::at("/content/Parts.lingua", "https://example.org/a/Root.lingua");
    assert_eq!(
        root.addresses(&library).unwrap()[0].uri,
        "https://example.org/content/Parts.lingua"
    );
    let native = Reference::path(&directory.path().join("Part #1?.lingua")).unwrap();
    let address = native.addresses(&library).unwrap().remove(0);
    assert!(address.uri.contains("%23"));
    assert!(address.uri.contains("%3F"));
    assert!(address.fragment.is_none());
    let literal = directory.path().join("Part#r_00000000000000000000000001");
    let address = Reference::new(literal.to_string_lossy())
        .addresses(&library)
        .unwrap()
        .remove(0);
    assert!(address.uid.is_none());
    assert!(address.fragment.is_none());
    assert!(
        Reference::at("file:///private/book", "https://example.org/Root.lingua")
            .addresses(&library)
            .is_err()
    );
}

#[test]
fn known_ids_and_declared_alternatives_share_one_resolution_path() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let uid = "r_00000000000000000000000001";
    assert!(
        Reference::new(format!("@{uid}"))
            .addresses(&library)
            .is_err()
    );
    library
        .remember_reference(uid, "https://example.org/Parts.lingua")
        .unwrap();
    let mut target = Reference::at(format!("@{uid}"), "file:///content/Root.lingua");
    target.locations.push("./Parts.lingua".into());
    let addresses = target.addresses(&library).unwrap();
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0].uri, "file:///content/Parts.lingua");
    assert_eq!(addresses[1].uri, "https://example.org/Parts.lingua");
    assert!(
        addresses
            .iter()
            .all(|address| address.uid.as_deref() == Some(uid))
    );
    target.locations[0] = "./Parts.lingua#r_00000000000000000000000002".into();
    assert!(target.addresses(&library).is_err());
}

#[cfg(feature = "publication")]
#[test]
fn explicit_imports_compose_once_with_their_own_bases_and_navigation_stays_unloaded() {
    use lince_resources::publication::Publication;
    use std::fs;
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("parts")).unwrap();
    fs::write(directory.path().join("Entry.lingua"), "Import \"./parts/Part.lingua\"\nImport \"./parts/Part.lingua#r_00000000000000000000000001\"\n").unwrap();
    fs::write(directory.path().join("parts/Part.lingua"), "Import \"../Entry.lingua\"\nPart (@part: 0, #parent \"../Not-visited.lingua#r_00000000000000000000000002\") { r_00000000000000000000000001\n} r_00000000000000000000000001\nExtension @part \"lince.resource\" { uri \"./book.epub\" }\n").unwrap();
    let library = Library::new(directory.path().join("library"));
    let source = Publication::load(
        &library,
        directory.path().join("Entry.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(source.documents.len(), 2);
    assert!(source.documents[0].project.records.is_empty());
    assert!(
        source.documents[1].project.records[0].assertions[0]
            .object_uri
            .as_deref()
            .unwrap()
            .contains("Not-visited.lingua#")
    );
    assert!(
        source.resources().unwrap()[0]
            .1
            .uri
            .ends_with("/parts/book.epub")
    );
    let uid = "r_00000000000000000000000001";
    let by_id = Publication::load(&library, &format!("@{uid}"), &[], false).unwrap();
    assert_eq!(by_id.documents.len(), 2);
    fs::write(
        directory.path().join("Fallback.lingua"),
        format!("Import @{uid} from [\"./missing.lingua\", \"./parts/Part.lingua\"]\n"),
    )
    .unwrap();
    assert_eq!(
        Publication::load(
            &library,
            directory.path().join("Fallback.lingua").to_str().unwrap(),
            &[],
            false
        )
        .unwrap()
        .documents
        .len(),
        3
    );
}

#[test]
fn known_revision_conflicts_need_an_explicit_source_or_hash_and_remote_access_skips_local_files() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let uid = "r_00000000000000000000000001";
    let first = lince_resources::fingerprint(b"first");
    let second = lince_resources::fingerprint(b"second");
    library
        .remember_definition(uid, "file:///content/First.lingua", Some(&first))
        .unwrap();
    library
        .remember_definition(uid, "https://example.org/Second.lingua", Some(&second))
        .unwrap();
    assert!(
        Reference::new(format!("@{uid}"))
            .addresses(&library)
            .is_err()
    );
    let target = Reference {
        target: format!("@{uid}"),
        hash: Some(second.clone()),
        ..Default::default()
    };
    let addresses = target.addresses(&library).unwrap();
    assert_eq!(addresses.len(), 1);
    assert_eq!(addresses[0].hash.as_deref(), Some(second.as_str()));
    let mut target = Reference::new(format!("@{uid}"));
    target.locations = vec![
        "file:///content/First.lingua".into(),
        "https://example.org/Second.lingua".into(),
    ];
    let addresses = target.addresses(&library.remote()).unwrap();
    assert_eq!(addresses.len(), 1);
    assert!(addresses[0].uri.starts_with("https:"));
    assert_eq!(addresses[0].hash.as_deref(), Some(second.as_str()));
}

#[test]
fn a_retained_hash_keeps_its_source_context_and_a_relative_hash_filename_remains_a_url() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let uri = "https://example.org/content/Part.lingua";
    let entry = library.retain_bytes(b"source", uri, uri).unwrap();
    let reference = Reference::new(&entry.hash);
    let resolved = reference.addresses(&library).unwrap();
    assert_eq!(resolved[0].uri, uri);
    assert_eq!(resolved[0].hash.as_deref(), Some(entry.hash.as_str()));
    let relative = Reference::at(&entry.hash, "https://example.org/content/Root.lingua");
    assert!(
        relative.addresses(&library).unwrap()[0]
            .uri
            .ends_with(&entry.hash)
    );
}

#[cfg(feature = "publication")]
#[test]
fn location_fallback_keeps_the_primary_record_identity_and_reports_the_obtained_entry() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path().join("library"));
    let identity = "r_00000000000000000000007771";
    let mirror = directory.path().join("Mirror.lingua");
    let mut reference = Reference::new(format!(
        "{}#{identity}",
        Reference::path(&directory.path().join("Gone.lingua"))
            .unwrap()
            .target
    ));
    reference
        .locations
        .push(Reference::path(&mirror).unwrap().target);
    std::fs::write(
        &mirror,
        "Wrong (@wrong: 0) { r_00000000000000000000007772\n} r_00000000000000000000007772",
    )
    .unwrap();
    assert!(
        lince_resources::publication::Publication::load_reference(
            &library,
            reference.clone(),
            &[],
            false
        )
        .is_err()
    );
    std::fs::write(
        &mirror,
        format!("Right (@right: 0) {{ {identity}\n}} {identity}"),
    )
    .unwrap();
    let loaded =
        lince_resources::publication::Publication::load_reference(&library, reference, &[], false)
            .unwrap();
    assert_eq!(loaded.entry, Reference::path(&mirror).unwrap().target);
    assert_eq!(loaded.fragment.as_deref(), Some(identity));
}
