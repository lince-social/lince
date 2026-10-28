#![cfg(feature = "publication")]

use lince_resources::{Library, publication::Publication};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn medicine_entries_discover_the_readings_and_export_external_resource_references() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails/nature/medicine");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let expected = Publication::files(&directory).unwrap().len();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert_eq!(audit.documents, expected);
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
        "Medicine.lingua",
        "musculoskeletal/Musculoskeletal-system.lingua",
        "orthopedics/Orthopedics.lingua",
        "orthopedics/shoulder/Shoulder.lingua",
        "orthopedics/shoulder/anatomy/Rotator-cuff.lingua",
        "foundations/Reading-from-zero.lingua",
        "orthopedics/shoulder/conditions/Pain-near-end-of-rotation.lingua",
    ] {
        let source = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(source.documents.len(), expected, "{entry}");
        assert!(source.parameters.is_empty());
        assert!(source.instances.is_empty());
    }
    let source = Publication::load(
        &library,
        directory.join("Medicine.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let resources = source.resources().unwrap();
    assert_eq!(resources.len(), 3);
    assert!(!directory.join("books").exists());
    let mut references = BTreeMap::new();
    for (_, resource) in &resources {
        assert!(resource.uri.starts_with("https://"));
        assert!(resource.hash.is_some());
        assert!(!resource.attribution.is_empty());
        assert!(!resource.license.is_empty());
        assert!(matches!(
            resource.media_type.as_str(),
            "application/pdf" | "model/gltf-binary"
        ));
        references.insert(resource.uri.clone(), resource.hash.clone());
    }
    let destination = temporary.path().join("export");
    let exported = source.export(&library, &destination, false).unwrap();
    let entry_name = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(&destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let relocated = Publication::load(
        &relocated_library,
        moved.join(entry_name).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(relocated.documents.len(), expected);
    let relocated_references: BTreeMap<_, _> = relocated
        .resources()
        .unwrap()
        .into_iter()
        .map(|(_, resource)| (resource.uri, resource.hash))
        .collect();
    assert_eq!(relocated_references, references);
    assert!(!moved.join("files").exists());
}
