#[cfg(target_os = "linux")]
mod linux {
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier};

    use engine::private_files::{
        MAX_CHILD_BYTES, MAX_DIRECTORY_COMPONENTS, MAX_DIRECTORY_PATH_BYTES, PrivateDirectory,
        PrivateFileError, PublishError,
    };

    struct Fixture {
        path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let path = parent.join(format!("lince-private-files-{}", uuid::Uuid::new_v4()));
            DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self { path }
        }
        fn directory(&self) -> PrivateDirectory {
            PrivateDirectory::open_existing(&self.path).unwrap()
        }
        fn child(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
        fn file(&self, name: &str, bytes: &[u8], mode: u32) {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.child(name))
                .unwrap();
            file.write_all(bytes).unwrap();
            file.set_permissions(fs::Permissions::from_mode(mode))
                .unwrap();
        }
        fn names(&self) -> Vec<String> {
            let mut names = fs::read_dir(&self.path)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            names.sort();
            names
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn private_files_publish_exact_key_and_restart_read() {
        let fixture = Fixture::new();
        let key = [0x5a; 32];
        fixture
            .directory()
            .publish_new_key32("cell.key", &key)
            .unwrap();
        let metadata = fs::metadata(fixture.child("cell.key")).unwrap();
        assert_eq!(metadata.len(), 32);
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(fixture.directory().read_key32("cell.key").unwrap(), key);
        assert_eq!(fixture.names(), ["cell.key"]);
    }

    #[test]
    fn private_files_read_only_and_read_write_private_modes_are_accepted() {
        let fixture = Fixture::new();
        for (name, mode) in [("readonly", 0o400), ("writable", 0o600)] {
            fixture.file(name, &[11; 32], mode);
            assert_eq!(fixture.directory().read_key32(name).unwrap(), [11; 32]);
            assert_eq!(
                fs::metadata(fixture.child(name)).unwrap().mode() & 0o7777,
                mode
            );
        }
    }

    #[test]
    fn private_files_missing_established_key_never_creates() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.directory().read_key32("missing"),
            Err(PrivateFileError::Missing)
        );
        assert!(fixture.names().is_empty());
    }

    #[test]
    fn private_files_missing_directory_never_provisions() {
        let fixture = Fixture::new();
        assert_eq!(
            PrivateDirectory::open_existing(fixture.child("missing")).err(),
            Some(PrivateFileError::Missing)
        );
        assert!(!fixture.child("missing").exists());
    }

    #[test]
    fn private_files_partial_and_overlong_established_keys_never_change() {
        let fixture = Fixture::new();
        for length in [0, 1, 16, 31, 33, 4096] {
            let name = format!("key-{length}");
            let bytes = vec![9; length];
            fixture.file(&name, &bytes, 0o600);
            assert_eq!(
                fixture.directory().read_key32(&name),
                Err(PrivateFileError::UnsafeKey)
            );
            assert_eq!(fs::read(fixture.child(&name)).unwrap(), bytes);
        }
        assert_eq!(fixture.names().len(), 6);
    }

    #[test]
    fn private_files_large_sparse_key_is_rejected_without_size_allocation() {
        let fixture = Fixture::new();
        fixture.file("large", &[], 0o600);
        OpenOptions::new()
            .write(true)
            .open(fixture.child("large"))
            .unwrap()
            .set_len(1024 * 1024 * 1024)
            .unwrap();
        assert_eq!(
            fixture.directory().read_key32("large"),
            Err(PrivateFileError::UnsafeKey)
        );
        assert_eq!(
            fs::metadata(fixture.child("large")).unwrap().len(),
            1024 * 1024 * 1024
        );
    }

    #[test]
    fn private_files_unsafe_permissions_refuse_without_repair() {
        let fixture = Fixture::new();
        for mode in [
            0o000, 0o200, 0o444, 0o644, 0o660, 0o700, 0o1600, 0o2600, 0o4600,
        ] {
            let name = format!("mode-{mode:o}");
            fixture.file(&name, &[12; 32], mode);
            assert!(fixture.directory().read_key32(&name).is_err());
            assert_eq!(
                fs::metadata(fixture.child(&name)).unwrap().mode() & 0o7777,
                mode
            );
        }
    }

    #[test]
    fn private_files_directory_requires_exact_private_permissions() {
        for mode in [
            0o000, 0o500, 0o600, 0o750, 0o755, 0o770, 0o1700, 0o2700, 0o4700,
        ] {
            let fixture = Fixture::new();
            fs::set_permissions(&fixture.path, fs::Permissions::from_mode(mode)).unwrap();
            assert!(PrivateDirectory::open_existing(&fixture.path).is_err());
            assert_eq!(fs::metadata(&fixture.path).unwrap().mode() & 0o7777, mode);
        }
    }

    #[test]
    fn private_files_held_directory_permissions_are_rechecked_for_each_operation() {
        let fixture = Fixture::new();
        let directory = fixture.directory();
        directory.publish_new_key32("key", &[1; 32]).unwrap();
        fs::set_permissions(&fixture.path, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            directory.read_key32("key"),
            Err(PrivateFileError::UnsafeDirectory)
        );
        assert_eq!(
            directory.publish_new_key32("other", &[2; 32]),
            Err(PublishError::BeforePublication {
                error: PrivateFileError::UnsafeDirectory,
                cleanup: None
            })
        );
        assert_eq!(fixture.names(), ["key"]);
    }

    #[test]
    fn private_files_final_symlink_and_dangling_symlink_are_not_followed() {
        let fixture = Fixture::new();
        fixture.file("original", &[8; 32], 0o600);
        symlink("original", fixture.child("link")).unwrap();
        symlink("absent", fixture.child("dangling")).unwrap();
        for name in ["link", "dangling"] {
            assert_eq!(
                fixture.directory().read_key32(name),
                Err(PrivateFileError::UnsafeKey)
            );
            assert!(matches!(
                fixture.directory().publish_new_key32(name, &[1; 32]),
                Err(PublishError::AlreadyExists { cleanup: None })
            ));
        }
        assert_eq!(fs::read(fixture.child("original")).unwrap(), [8; 32]);
        assert!(!fixture.child("absent").exists());
    }

    #[test]
    fn private_files_symlink_directories_and_intermediate_links_refuse() {
        let fixture = Fixture::new();
        DirBuilder::new()
            .mode(0o700)
            .create(fixture.child("real"))
            .unwrap();
        DirBuilder::new()
            .mode(0o700)
            .create(fixture.child("real/nested"))
            .unwrap();
        symlink("real", fixture.child("link")).unwrap();
        for name in ["link", "link/nested", "link/."] {
            assert_eq!(
                PrivateDirectory::open_existing(fixture.child(name)).err(),
                Some(PrivateFileError::UnsafeDirectory)
            );
        }
    }

    #[test]
    fn private_files_regular_file_is_not_a_directory() {
        let fixture = Fixture::new();
        fixture.file("file", &[1; 32], 0o600);
        assert_eq!(
            PrivateDirectory::open_existing(fixture.child("file")).err(),
            Some(PrivateFileError::UnsafeDirectory)
        );
    }

    #[test]
    fn private_files_hard_linked_keys_refuse_at_both_names() {
        let fixture = Fixture::new();
        fixture.file("key", &[1; 32], 0o600);
        fs::hard_link(fixture.child("key"), fixture.child("alias")).unwrap();
        for name in ["key", "alias"] {
            assert_eq!(
                fixture.directory().read_key32(name),
                Err(PrivateFileError::UnsafeKey)
            );
        }
        assert_eq!(fs::metadata(fixture.child("key")).unwrap().nlink(), 2);
    }

    #[test]
    fn private_files_fifo_is_opened_nonblocking_and_refused() {
        let fixture = Fixture::new();
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            fixture.child("fifo"),
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        let directory = fixture.directory();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || sender.send(directory.read_key32("fifo")).unwrap());
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            Err(PrivateFileError::UnsafeKey)
        );
        worker.join().unwrap();
    }

    #[test]
    fn private_files_child_directory_is_not_key_material() {
        let fixture = Fixture::new();
        DirBuilder::new()
            .mode(0o700)
            .create(fixture.child("child"))
            .unwrap();
        assert_eq!(
            fixture.directory().read_key32("child"),
            Err(PrivateFileError::UnsafeKey)
        );
    }

    #[test]
    fn private_files_invalid_child_names_never_touch_the_directory() {
        let fixture = Fixture::new();
        let directory = fixture.directory();
        for name in [
            "", ".", "..", "a/../key", "/key", "a/key", "a\\key", "key\0", "key\n", " key", "key ",
        ] {
            assert_eq!(
                directory.read_key32(name),
                Err(PrivateFileError::InvalidChildName)
            );
            assert_eq!(
                directory.publish_new_key32(name, &[0; 32]),
                Err(PublishError::BeforePublication {
                    error: PrivateFileError::InvalidChildName,
                    cleanup: None
                })
            );
        }
        assert!(fixture.names().is_empty());
    }

    #[test]
    fn private_files_child_byte_limit_is_exact_and_unicode_is_supported() {
        let fixture = Fixture::new();
        let directory = fixture.directory();
        let name = "é".repeat(MAX_CHILD_BYTES / 2);
        directory.publish_new_key32(&name, &[3; 32]).unwrap();
        assert_eq!(directory.read_key32(&name).unwrap(), [3; 32]);
        assert_eq!(
            directory.read_key32(&format!("{name}x")),
            Err(PrivateFileError::InvalidChildName)
        );
        directory
            .publish_new_key32(".hidden-key", &[4; 32])
            .unwrap();
    }

    #[test]
    fn private_files_directory_path_limits_and_parent_components_refuse() {
        let fixture = Fixture::new();
        for path in [
            PathBuf::new(),
            fixture.child("../escape"),
            PathBuf::from("x".repeat(MAX_DIRECTORY_PATH_BYTES + 1)),
            PathBuf::from("x/".repeat(MAX_DIRECTORY_COMPONENTS + 1)),
            PathBuf::from("bad\0path"),
        ] {
            assert_eq!(
                PrivateDirectory::open_existing(path).err(),
                Some(PrivateFileError::InvalidDirectoryPath)
            );
        }
        assert!(fixture.names().is_empty());
    }

    #[test]
    fn private_files_existing_valid_and_invalid_destinations_are_unchanged() {
        let fixture = Fixture::new();
        let directory = fixture.directory();
        for (name, bytes) in [("valid", vec![7; 32]), ("partial", vec![8; 3])] {
            fixture.file(name, &bytes, 0o600);
            assert_eq!(
                directory.publish_new_key32(name, &[9; 32]),
                Err(PublishError::AlreadyExists { cleanup: None })
            );
            assert_eq!(fs::read(fixture.child(name)).unwrap(), bytes);
        }
        assert_eq!(fixture.names(), ["partial", "valid"]);
    }

    #[test]
    fn private_files_preexisting_directory_destination_is_not_removed() {
        let fixture = Fixture::new();
        DirBuilder::new()
            .mode(0o700)
            .create(fixture.child("key"))
            .unwrap();
        assert!(matches!(
            fixture.directory().publish_new_key32("key", &[1; 32]),
            Err(PublishError::AlreadyExists { cleanup: None })
        ));
        assert!(fixture.child("key").is_dir());
        assert_eq!(fixture.names(), ["key"]);
    }

    #[test]
    fn private_files_concurrent_publishers_have_exactly_one_complete_winner() {
        let fixture = Fixture::new();
        let directory = Arc::new(fixture.directory());
        let barrier = Arc::new(Barrier::new(8));
        let mut jobs = Vec::new();
        for index in 0..8_u8 {
            let directory = directory.clone();
            let barrier = barrier.clone();
            jobs.push(std::thread::spawn(move || {
                barrier.wait();
                (index, directory.publish_new_key32("key", &[index; 32]))
            }));
        }
        let mut winner = None;
        for job in jobs {
            let (index, result) = job.join().unwrap();
            match result {
                Ok(()) => assert!(winner.replace(index).is_none()),
                Err(PublishError::AlreadyExists { cleanup: None }) => {}
                other => panic!("unexpected publication status: {other:?}"),
            }
        }
        assert_eq!(directory.read_key32("key").unwrap(), [winner.unwrap(); 32]);
        assert_eq!(fixture.names(), ["key"]);
    }

    #[test]
    fn private_files_replaced_directory_path_cannot_redirect_a_held_descriptor() {
        let fixture = Fixture::new();
        for name in ["private", "other"] {
            DirBuilder::new()
                .mode(0o700)
                .create(fixture.child(name))
                .unwrap();
        }
        let held = PrivateDirectory::open_existing(fixture.child("private")).unwrap();
        held.publish_new_key32("key", &[1; 32]).unwrap();
        PrivateDirectory::open_existing(fixture.child("other"))
            .unwrap()
            .publish_new_key32("key", &[2; 32])
            .unwrap();
        fs::rename(fixture.child("private"), fixture.child("moved")).unwrap();
        symlink("other", fixture.child("private")).unwrap();
        assert_eq!(held.read_key32("key").unwrap(), [1; 32]);
        held.publish_new_key32("new", &[3; 32]).unwrap();
        assert_eq!(fs::read(fixture.child("moved/new")).unwrap(), [3; 32]);
        assert!(!fixture.child("other/new").exists());
        assert_eq!(fs::read(fixture.child("other/key")).unwrap(), [2; 32]);
        assert!(PrivateDirectory::open_existing(fixture.child("private")).is_err());
    }

    #[test]
    fn private_files_refusal_text_contains_neither_key_nor_rejected_path() {
        let fixture = Fixture::new();
        let error = fixture
            .directory()
            .publish_new_key32("SECRET/DO_NOT_ECHO", &[0x5a; 32])
            .err()
            .unwrap();
        assert!(!error.to_string().contains("SECRET"));
        assert!(!format!("{error:?}").contains("90, 90"));
        assert!(
            !PrivateDirectory::open_existing(Path::new("../SECRET"))
                .err()
                .unwrap()
                .to_string()
                .contains("SECRET")
        );
    }

    #[test]
    fn private_files_unknown_public_status_retains_names_without_claiming_an_outcome() {
        let error = PublishError::PublicationUnknown {
            error: PrivateFileError::Io { os_error: 5 },
            temporary_name: ".lince-key-evidence.tmp".to_owned(),
            destination_name: "SECRET_NAME_NOT_FOR_DISPLAY".to_owned(),
        };
        let description = error.to_string();
        assert!(description.contains("unknown"));
        assert!(!description.contains("not published"));
        assert!(!description.contains("was published"));
        assert!(!description.contains("SECRET_NAME_NOT_FOR_DISPLAY"));
        let PublishError::PublicationUnknown {
            temporary_name,
            destination_name,
            ..
        } = error
        else {
            panic!("distinct public state required")
        };
        assert_eq!(temporary_name, ".lince-key-evidence.tmp");
        assert_eq!(destination_name, "SECRET_NAME_NOT_FOR_DISPLAY");
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn private_files_unsupported_platform_has_no_path_fallback() {
    use engine::private_files::{PrivateDirectory, PrivateFileError};
    assert_eq!(
        PrivateDirectory::open_existing("not-created").err(),
        Some(PrivateFileError::Unsupported)
    );
}
