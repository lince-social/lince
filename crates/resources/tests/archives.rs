use lince_resources::{Library, Reference};
use std::{fs, io::Write};

fn archive(path: &std::path::Path, name: &str) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"Book bytes").unwrap();
    zip.finish().unwrap();
}

#[test]
fn extracted_folders_move_and_books_register_in_place_before_explicit_retention() {
    let directory = tempfile::tempdir().unwrap();
    let zip = directory.path().join("publication.zip");
    archive(&zip, "files/book.epub");
    let library = Library::new(directory.path().join("cell"));
    let extracted = library
        .extract_zip(
            &Reference::path(&zip).unwrap().target,
            &directory.path().join("content"),
        )
        .unwrap();
    let moved = directory.path().join("moved");
    fs::rename(extracted, &moved).unwrap();
    let book = moved.join("files/book.epub");
    let uri = Reference::path(&book).unwrap().target;
    let registered = library
        .register_in_place(&book, &uri, "application/epub+zip", None)
        .unwrap();
    assert_eq!(registered.path, fs::canonicalize(&book).unwrap());
    assert!(!library.root().join("files").join(&registered.hash).exists());
    let retained = library
        .fetch(&lince_resources::Resource {
            uri,
            hash: Some(registered.hash),
            media_type: "application/epub+zip".into(),
            ..Default::default()
        })
        .unwrap();
    fs::remove_dir_all(moved).unwrap();
    assert!(retained.path.is_file());
}

#[test]
fn archive_parent_paths_and_duplicate_entries_do_not_escape_or_replace_files() {
    let directory = tempfile::tempdir().unwrap();
    let zip = directory.path().join("unsafe.zip");
    archive(&zip, "../outside");
    let library = Library::new(directory.path().join("cell"));
    assert!(
        library
            .extract_zip(
                &Reference::path(&zip).unwrap().target,
                &directory.path().join("extracted")
            )
            .is_err()
    );
    assert!(!directory.path().join("outside").exists());
    assert!(!directory.path().join("extracted").exists());
}

#[test]
fn archive_entries_cannot_expand_past_their_declared_size() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corrupt.zip");
    archive(&path, "book.epub");
    let mut bytes = fs::read(&path).unwrap();
    let local = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x03\x04")
        .unwrap();
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    for start in [local + 22, central + 24] {
        bytes[start..start + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    fs::write(&path, bytes).unwrap();
    let destination = directory.path().join("extracted");
    let library = Library::new(directory.path().join("cell"));
    assert!(
        library
            .extract_zip(&Reference::path(&path).unwrap().target, &destination)
            .is_err()
    );
    assert!(!destination.exists());
}
