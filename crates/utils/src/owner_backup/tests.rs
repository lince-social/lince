use super::*;
use std::{cell::RefCell, collections::BTreeMap, io::Cursor, rc::Rc};

const PASSWORD: &str = "a private owner backup test passphrase";
type Files = BTreeMap<FileKind, Vec<u8>>;

fn fixture() -> (Manifest, Files) {
    let mut database = vec![0x65; CHUNK_BYTES * 2 + 53];
    database[..31].copy_from_slice(b"Private retained message marker");
    let files = BTreeMap::from([
        (FileKind::Database, database),
        (FileKind::OwnerRoot, vec![0x38; 32]),
        (FileKind::AuthorityWallet, vec![0x42; 32]),
        (
            FileKind::RecordKeyring,
            b"private record sealing material".to_vec(),
        ),
    ]);
    let manifest = Manifest {
        organ: "r_owner_identity_test".into(),
        cell: "r_cell_identity_test".into(),
        created_at: 1_790_899_200,
        files: files
            .iter()
            .map(|(kind, body)| Entry {
                kind: *kind,
                bytes: body.len() as u64,
                sha256: Sha256::digest(body).into(),
            })
            .collect(),
    };
    (manifest, files)
}

struct BoundedRead(Cursor<Vec<u8>>);

impl Read for BoundedRead {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        assert!(bytes.len() <= CHUNK_BYTES + 16);
        self.0.read(bytes)
    }
}

struct Staging {
    files: Rc<RefCell<Files>>,
    kind: FileKind,
}

impl Write for Staging {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert!(bytes.len() <= CHUNK_BYTES);
        self.files
            .borrow_mut()
            .get_mut(&self.kind)
            .unwrap()
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn seal(manifest: &Manifest, files: &Files) -> Vec<u8> {
    let mut encrypted = Vec::new();
    seal_archive(&mut encrypted, PASSWORD, manifest, |entry| {
        Ok(BoundedRead(Cursor::new(files[&entry.kind].clone())))
    })
    .unwrap();
    encrypted
}

fn open(encrypted: Vec<u8>, password: &str) -> io::Result<(Manifest, Files)> {
    let files = Rc::new(RefCell::new(BTreeMap::new()));
    let manifest = open_archive_into_staging(
        &mut BoundedRead(Cursor::new(encrypted)),
        password,
        |entry| {
            assert!(files.borrow_mut().insert(entry.kind, Vec::new()).is_none());
            Ok(Staging {
                files: files.clone(),
                kind: entry.kind,
            })
        },
    )?;
    let result = files.borrow().clone();
    Ok((manifest, result))
}

fn frames(encrypted: &[u8]) -> Vec<std::ops::Range<usize>> {
    let mut result = Vec::new();
    let mut index = 48;
    while index < encrypted.len() {
        let size = u32::from_le_bytes(encrypted[index..index + 4].try_into().unwrap()) as usize;
        result.push(index..index + 4 + size);
        index += 4 + size;
    }
    assert_eq!(index, encrypted.len());
    result
}

#[test]
fn encrypted_owner_archive_streams_multiple_chunks_and_hides_identity_and_content() {
    let (manifest, files) = fixture();
    let first = seal(&manifest, &files);
    let second = seal(&manifest, &files);
    assert_ne!(first, second);
    assert!(first.len() as u64 <= MAX_ARCHIVE_BYTES);
    for marker in [
        manifest.organ.as_bytes(),
        manifest.cell.as_bytes(),
        b"Private retained message marker",
        &files[&FileKind::OwnerRoot],
        &files[&FileKind::AuthorityWallet],
    ] {
        assert!(!first.windows(marker.len()).any(|bytes| bytes == marker));
    }
    assert_eq!(open(first, PASSWORD).unwrap(), (manifest, files));
}

#[test]
fn wrong_password_or_damaged_manifest_never_creates_staging_entries() {
    let (manifest, files) = fixture();
    let encrypted = seal(&manifest, &files);
    let mut bad_manifest = encrypted.clone();
    bad_manifest[52] ^= 1;
    let mut oversized = encrypted.clone();
    oversized[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
    for (mut source, password) in [
        (encrypted.clone(), "wrong"),
        (encrypted.clone(), ""),
        (encrypted[..47].to_vec(), PASSWORD),
        (bad_manifest, PASSWORD),
        (oversized, PASSWORD),
    ] {
        let mut created = 0;
        let result = open_archive_into_staging(&mut Cursor::new(&mut source), password, |_| {
            created += 1;
            Ok(io::sink())
        });
        assert!(result.is_err());
        assert_eq!(created, 0);
    }
}

#[test]
fn frame_order_splicing_truncation_and_trailing_bytes_fail_authentication() {
    let (manifest, files) = fixture();
    let encrypted = seal(&manifest, &files);
    let another = seal(&manifest, &files);
    let ranges = frames(&encrypted);
    let other_ranges = frames(&another);
    let mut reordered = encrypted.clone();
    let first = reordered[ranges[1].clone()].to_vec();
    let second = reordered[ranges[2].clone()].to_vec();
    assert_eq!(first.len(), second.len());
    reordered[ranges[1].clone()].copy_from_slice(&second);
    reordered[ranges[2].clone()].copy_from_slice(&first);
    let mut spliced = encrypted.clone();
    spliced[ranges[1].clone()].copy_from_slice(&another[other_ranges[1].clone()]);
    let mut trailing = encrypted.clone();
    trailing.push(0);
    let mut damaged = encrypted.clone();
    damaged[ranges.last().unwrap().end - 1] ^= 1;
    let mut modified_header = encrypted.clone();
    modified_header[32] ^= 1;
    for source in [
        reordered,
        spliced,
        trailing,
        damaged,
        modified_header,
        encrypted[..encrypted.len() - 1].to_vec(),
        encrypted[..ranges[1].end].to_vec(),
    ] {
        assert!(open(source, PASSWORD).is_err());
    }
}

#[test]
fn untrusted_manifest_entries_are_validated_before_staging_even_with_a_valid_password() {
    let (manifest, _) = fixture();
    let mut invalid = Vec::new();
    let mut duplicate = serde_json::to_value(&manifest).unwrap();
    duplicate["files"][1] = duplicate["files"][0].clone();
    invalid.push(duplicate);
    let mut unknown = serde_json::to_value(&manifest).unwrap();
    unknown["files"][0]["kind"] = serde_json::json!("../../active/lince.db");
    invalid.push(unknown);
    let mut oversized = serde_json::to_value(&manifest).unwrap();
    oversized["files"][0]["bytes"] = serde_json::json!(MAX_DATABASE_BYTES + 1);
    invalid.push(oversized);
    let mut missing_root = serde_json::to_value(&manifest).unwrap();
    missing_root["files"].as_array_mut().unwrap().remove(1);
    invalid.push(missing_root);
    for value in invalid {
        let mut header = [0; 48];
        header[..16].copy_from_slice(MAGIC);
        getrandom::fill(&mut header[16..]).unwrap();
        let mut encrypted = header.to_vec();
        Frames::new(header, PASSWORD)
            .unwrap()
            .write(&mut encrypted, &serde_json::to_vec(&value).unwrap())
            .unwrap();
        let mut created = false;
        assert!(
            open_archive_into_staging(&mut Cursor::new(encrypted), PASSWORD, |_| {
                created = true;
                Ok(io::sink())
            })
            .is_err()
        );
        assert!(!created);
    }
}

#[test]
fn source_length_and_hash_mismatches_never_produce_a_valid_archive() {
    let (manifest, files) = fixture();
    for change in ["short", "long", "hash"] {
        let mut changed = files.clone();
        let database = changed.get_mut(&FileKind::Database).unwrap();
        match change {
            "short" => {
                database.pop();
            }
            "long" => database.push(0),
            "hash" => database[0] ^= 1,
            _ => unreachable!(),
        }
        let mut encrypted = Vec::new();
        assert!(
            seal_archive(&mut encrypted, PASSWORD, &manifest, |entry| {
                Ok(Cursor::new(changed[&entry.kind].clone()))
            })
            .is_err()
        );
        assert!(open(encrypted, PASSWORD).is_err());
    }
}

#[test]
fn invalid_manifest_is_rejected_before_output_or_source_access() {
    let (mut manifest, _) = fixture();
    manifest.files[0].bytes = MAX_DATABASE_BYTES + 1;
    let mut output = Vec::new();
    assert!(
        seal_archive::<Cursor<Vec<u8>>, _>(&mut output, PASSWORD, &manifest, |_| {
            panic!("Invalid manifests must fail before opening sources");
        })
        .is_err()
    );
    assert!(output.is_empty());
}
