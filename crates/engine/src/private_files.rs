use std::fmt;
use std::path::Path;

pub const MAX_CHILD_BYTES: usize = 128;
pub const MAX_DIRECTORY_PATH_BYTES: usize = 4096;
pub const MAX_DIRECTORY_COMPONENTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateFileError {
    Unsupported,
    InvalidDirectoryPath,
    InvalidChildName,
    Missing,
    UnsafeDirectory,
    UnsafeKey,
    ChangedDuringOperation,
    EntropyUnavailable,
    TemporaryNameUnavailable,
    IncompleteWrite,
    Io { os_error: i32 },
}

impl fmt::Display for PrivateFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Private file operation refused: {self:?}")
    }
}

impl std::error::Error for PrivateFileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupFailure {
    pub temporary_name: String,
    pub error: PrivateFileError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    BeforePublication {
        error: PrivateFileError,
        cleanup: Option<CleanupFailure>,
    },
    AlreadyExists {
        cleanup: Option<CleanupFailure>,
    },
    PublicationUnknown {
        error: PrivateFileError,
        temporary_name: String,
        destination_name: String,
    },
    PublishedButUnconfirmed {
        error: PrivateFileError,
    },
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeforePublication { error, cleanup } => {
                write!(
                    formatter,
                    "Key was not published: {error}; cleanup: {cleanup:?}"
                )
            }
            Self::AlreadyExists { cleanup } => {
                write!(
                    formatter,
                    "Key destination already exists; cleanup: {cleanup:?}"
                )
            }
            Self::PublicationUnknown { error, .. } => {
                write!(
                    formatter,
                    "Key publication outcome is unknown; preserve both paths and reconcile before retrying: {error}"
                )
            }
            Self::PublishedButUnconfirmed { error } => {
                write!(
                    formatter,
                    "Key was published but durability is unconfirmed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for PublishError {}

pub struct PrivateDirectory {
    #[cfg(target_os = "linux")]
    directory: std::os::fd::OwnedFd,
    #[cfg(not(target_os = "linux"))]
    unavailable: (),
}

#[cfg(not(target_os = "linux"))]
impl PrivateDirectory {
    pub fn open_existing(_path: impl AsRef<Path>) -> Result<Self, PrivateFileError> {
        Err(PrivateFileError::Unsupported)
    }

    pub fn read_key32(&self, _child: &str) -> Result<[u8; 32], PrivateFileError> {
        let _ = self.unavailable;
        Err(PrivateFileError::Unsupported)
    }

    pub fn publish_new_key32(&self, _child: &str, _key: &[u8; 32]) -> Result<(), PublishError> {
        Err(PublishError::BeforePublication {
            error: PrivateFileError::Unsupported,
            cleanup: None,
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;

    use rustix::fs::{self, AtFlags, CWD, FileType, Mode, OFlags, RenameFlags, Stat};
    use rustix::io::{self, Errno};
    use rustix::process::geteuid;

    use super::*;

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Metadata {
        kind: FileType,
        device: u64,
        inode: u64,
        links: u64,
        owner: u32,
        group: u32,
        mode: u32,
        size: i64,
        modified: (i64, u64),
        changed: (i64, u64),
    }

    impl Metadata {
        fn from_stat(stat: Stat) -> Self {
            Self {
                kind: FileType::from_raw_mode(stat.st_mode),
                device: stat.st_dev as u64,
                inode: stat.st_ino as u64,
                links: stat.st_nlink as u64,
                owner: stat.st_uid,
                group: stat.st_gid,
                mode: stat.st_mode & 0o7777,
                size: stat.st_size as i64,
                modified: (stat.st_mtime as i64, stat.st_mtime_nsec as u64),
                changed: (stat.st_ctime as i64, stat.st_ctime_nsec as u64),
            }
        }

        fn read(fd: impl AsFd) -> Result<Self, PrivateFileError> {
            fs::fstat(fd).map(Self::from_stat).map_err(file_error)
        }

        fn same_inode(self, other: Self) -> bool {
            self.device == other.device && self.inode == other.inode
        }

        fn require_directory(self, effective_uid: u32) -> Result<(), PrivateFileError> {
            if self.kind != FileType::Directory
                || self.owner != effective_uid
                || self.mode != 0o700
                || self.links == 0
            {
                return Err(PrivateFileError::UnsafeDirectory);
            }
            Ok(())
        }

        fn require_key(self, effective_uid: u32) -> Result<(), PrivateFileError> {
            if self.kind != FileType::RegularFile
                || self.owner != effective_uid
                || !matches!(self.mode, 0o400 | 0o600)
                || self.links != 1
                || self.size != 32
            {
                return Err(PrivateFileError::UnsafeKey);
            }
            Ok(())
        }
    }

    fn file_error(error: Errno) -> PrivateFileError {
        match error {
            Errno::NOSYS | Errno::OPNOTSUPP => PrivateFileError::Unsupported,
            Errno::NOENT => PrivateFileError::Missing,
            _ => PrivateFileError::Io {
                os_error: error.raw_os_error(),
            },
        }
    }

    fn required_syscall_error(error: Errno) -> PrivateFileError {
        if error == Errno::INVAL {
            PrivateFileError::Unsupported
        } else {
            file_error(error)
        }
    }

    fn child_name(child: &str) -> Result<(), PrivateFileError> {
        if child.is_empty()
            || child.len() > MAX_CHILD_BYTES
            || matches!(child, "." | "..")
            || child.contains(['/', '\\'])
            || child.trim() != child
            || child.chars().any(char::is_control)
        {
            return Err(PrivateFileError::InvalidChildName);
        }
        Ok(())
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Stage {
        BeforeWrite,
        BeforeFileSync,
        BeforeRename,
        Rename,
        BeforeDirectorySync,
        BeforeCleanup,
        BeforeCleanupSync,
        AfterOpenMetadata,
        AfterKeyRead,
        BeforeFinalMetadata,
    }

    struct Temporary {
        name: String,
        file: OwnedFd,
    }

    impl PrivateDirectory {
        pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, PrivateFileError> {
            let path = path.as_ref();
            let bytes = path.as_os_str().as_bytes();
            if bytes.is_empty() || bytes.len() > MAX_DIRECTORY_PATH_BYTES || bytes.contains(&0) {
                return Err(PrivateFileError::InvalidDirectoryPath);
            }
            let mut components = 0;
            for component in path.components() {
                components += 1;
                if components > MAX_DIRECTORY_COMPONENTS
                    || matches!(component, Component::ParentDir | Component::Prefix(_))
                    || matches!(component, Component::Normal(name) if name.as_bytes().len() > 255)
                {
                    return Err(PrivateFileError::InvalidDirectoryPath);
                }
            }
            let initial = if path.is_absolute() { "/" } else { "." };
            let mut directory =
                fs::openat(CWD, initial, DIRECTORY_FLAGS, Mode::empty()).map_err(file_error)?;
            for component in path.components() {
                if let Component::Normal(name) = component {
                    directory = fs::openat(&directory, name, DIRECTORY_FLAGS, Mode::empty())
                        .map_err(|error| match error {
                            Errno::LOOP | Errno::NOTDIR => PrivateFileError::UnsafeDirectory,
                            _ => file_error(error),
                        })?;
                }
            }
            let opened = Self { directory };
            opened.require_private()?;
            Ok(opened)
        }

        fn require_private(&self) -> Result<(), PrivateFileError> {
            Metadata::read(&self.directory)?.require_directory(geteuid().as_raw())
        }

        fn named_metadata(&self, child: &str) -> Result<Metadata, PrivateFileError> {
            fs::statat(&self.directory, child, AtFlags::SYMLINK_NOFOLLOW)
                .map(Metadata::from_stat)
                .map_err(|error| {
                    if error == Errno::NOENT {
                        PrivateFileError::ChangedDuringOperation
                    } else {
                        file_error(error)
                    }
                })
        }

        pub fn read_key32(&self, child: &str) -> Result<[u8; 32], PrivateFileError> {
            self.read_with(child, |_, _| Ok(()))
        }

        fn read_with(
            &self,
            child: &str,
            mut checkpoint: impl FnMut(Stage, BorrowedFd<'_>) -> Result<(), PrivateFileError>,
        ) -> Result<[u8; 32], PrivateFileError> {
            child_name(child)?;
            self.require_private()?;
            let file = fs::openat(
                &self.directory,
                child,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| {
                if error == Errno::LOOP {
                    PrivateFileError::UnsafeKey
                } else {
                    file_error(error)
                }
            })?;
            let before = Metadata::read(&file)?;
            before.require_key(geteuid().as_raw())?;
            if self.named_metadata(child)? != before {
                return Err(PrivateFileError::ChangedDuringOperation);
            }
            checkpoint(Stage::AfterOpenMetadata, file.as_fd())?;
            let mut key = [0; 32];
            let mut filled = 0;
            while filled < key.len() {
                let count = io::read(&file, &mut key[filled..]).map_err(file_error)?;
                if count == 0 {
                    return Err(PrivateFileError::ChangedDuringOperation);
                }
                filled += count;
            }
            checkpoint(Stage::AfterKeyRead, file.as_fd())?;
            let mut extra = [0; 1];
            if io::read(&file, &mut extra[..]).map_err(file_error)? != 0 {
                return Err(PrivateFileError::ChangedDuringOperation);
            }
            checkpoint(Stage::BeforeFinalMetadata, file.as_fd())?;
            let after = Metadata::read(&file)?;
            if after != before || self.named_metadata(child)? != after {
                return Err(PrivateFileError::ChangedDuringOperation);
            }
            self.require_private()?;
            Ok(key)
        }

        pub fn publish_new_key32(&self, child: &str, key: &[u8; 32]) -> Result<(), PublishError> {
            self.publish_with(child, key, |_, _| Ok(()))
        }

        fn temporary(&self, child: &str) -> Result<Temporary, PrivateFileError> {
            for _ in 0..8 {
                let mut random = [0; 16];
                getrandom::fill(&mut random).map_err(|_| PrivateFileError::EntropyUnavailable)?;
                let name = format!(".lince-key-{}.tmp", uuid::Uuid::from_bytes(random).simple());
                if name == child {
                    continue;
                }
                let file = fs::openat(
                    &self.directory,
                    name.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK
                        | OFlags::CLOEXEC,
                    Mode::RUSR | Mode::WUSR,
                );
                match file {
                    Ok(file) => return Ok(Temporary { name, file }),
                    Err(Errno::EXIST) => continue,
                    Err(error) => return Err(file_error(error)),
                }
            }
            Err(PrivateFileError::TemporaryNameUnavailable)
        }

        fn cleanup(
            &self,
            temporary: &Temporary,
            checkpoint: &mut impl FnMut(Stage, BorrowedFd<'_>) -> Result<(), PrivateFileError>,
        ) -> Option<CleanupFailure> {
            let result = (|| {
                checkpoint(Stage::BeforeCleanup, temporary.file.as_fd())?;
                self.require_private()?;
                let opened = Metadata::read(&temporary.file)?;
                let named = self.named_metadata(&temporary.name)?;
                if !opened.same_inode(named) {
                    return Err(PrivateFileError::ChangedDuringOperation);
                }
                fs::unlinkat(&self.directory, temporary.name.as_str(), AtFlags::empty())
                    .map_err(file_error)?;
                checkpoint(Stage::BeforeCleanupSync, self.directory.as_fd())?;
                fs::fsync(&self.directory).map_err(required_syscall_error)
            })();
            result.err().map(|error| CleanupFailure {
                temporary_name: temporary.name.clone(),
                error,
            })
        }

        fn publish_with(
            &self,
            child: &str,
            key: &[u8; 32],
            checkpoint: impl FnMut(Stage, BorrowedFd<'_>) -> Result<(), PrivateFileError>,
        ) -> Result<(), PublishError> {
            self.publish_using(
                child,
                key,
                checkpoint,
                |directory, temporary, destination| {
                    fs::renameat_with(
                        directory,
                        temporary,
                        directory,
                        destination,
                        RenameFlags::NOREPLACE,
                    )
                },
            )
        }

        fn publish_using(
            &self,
            child: &str,
            key: &[u8; 32],
            mut checkpoint: impl FnMut(Stage, BorrowedFd<'_>) -> Result<(), PrivateFileError>,
            mut rename: impl FnMut(BorrowedFd<'_>, &str, &str) -> Result<(), Errno>,
        ) -> Result<(), PublishError> {
            let before_error = |error| PublishError::BeforePublication {
                error,
                cleanup: None,
            };
            child_name(child).map_err(before_error)?;
            self.require_private().map_err(before_error)?;
            let temporary = self.temporary(child).map_err(before_error)?;
            let prepare = (|| {
                fs::fchmod(&temporary.file, Mode::RUSR | Mode::WUSR).map_err(file_error)?;
                checkpoint(Stage::BeforeWrite, temporary.file.as_fd())?;
                let mut written = 0;
                while written < key.len() {
                    let count = io::write(&temporary.file, &key[written..]).map_err(file_error)?;
                    if count == 0 {
                        return Err(PrivateFileError::IncompleteWrite);
                    }
                    written += count;
                }
                let complete = Metadata::read(&temporary.file)?;
                complete.require_key(geteuid().as_raw())?;
                if complete.mode != 0o600 {
                    return Err(PrivateFileError::UnsafeKey);
                }
                checkpoint(Stage::BeforeFileSync, temporary.file.as_fd())?;
                fs::fsync(&temporary.file).map_err(required_syscall_error)?;
                checkpoint(Stage::BeforeRename, temporary.file.as_fd())?;
                self.require_private()?;
                if Metadata::read(&temporary.file)? != complete
                    || self.named_metadata(&temporary.name)? != complete
                {
                    return Err(PrivateFileError::ChangedDuringOperation);
                }
                Ok(())
            })();
            if let Err(error) = prepare {
                return Err(PublishError::BeforePublication {
                    error,
                    cleanup: self.cleanup(&temporary, &mut checkpoint),
                });
            }
            if let Err(error) = checkpoint(Stage::Rename, temporary.file.as_fd()) {
                return Err(PublishError::BeforePublication {
                    error,
                    cleanup: self.cleanup(&temporary, &mut checkpoint),
                });
            }
            match rename(self.directory.as_fd(), temporary.name.as_str(), child) {
                Ok(()) => {}
                Err(Errno::EXIST) => {
                    return Err(PublishError::AlreadyExists {
                        cleanup: self.cleanup(&temporary, &mut checkpoint),
                    });
                }
                Err(Errno::NOSYS | Errno::OPNOTSUPP | Errno::INVAL) => {
                    return Err(PublishError::BeforePublication {
                        error: PrivateFileError::Unsupported,
                        cleanup: self.cleanup(&temporary, &mut checkpoint),
                    });
                }
                Err(error) => {
                    return Err(PublishError::PublicationUnknown {
                        error: file_error(error),
                        temporary_name: temporary.name,
                        destination_name: child.to_owned(),
                    });
                }
            }
            checkpoint(Stage::BeforeDirectorySync, self.directory.as_fd())
                .and_then(|()| self.require_private())
                .and_then(|()| fs::fsync(&self.directory).map_err(required_syscall_error))
                .map_err(|error| PublishError::PublishedButUnconfirmed { error })
        }
    }

    #[cfg(test)]
    mod tests {
        use std::fs::{self as stdfs, DirBuilder, OpenOptions};
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
        use std::path::PathBuf;

        use super::*;

        struct Fixture {
            path: PathBuf,
            directory: PrivateDirectory,
        }

        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                    "lince-private-files-fault-{}",
                    uuid::Uuid::new_v4()
                ));
                DirBuilder::new().mode(0o700).create(&path).unwrap();
                let directory = PrivateDirectory::open_existing(&path).unwrap();
                Self { path, directory }
            }
            fn names(&self) -> Vec<String> {
                let mut names = stdfs::read_dir(&self.path)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                    .collect::<Vec<_>>();
                names.sort();
                names
            }
            fn temporary_name(&self, file: BorrowedFd<'_>) -> String {
                let inode = Metadata::read(file).unwrap().inode;
                self.names()
                    .into_iter()
                    .find(|name| stdfs::metadata(self.path.join(name)).unwrap().ino() == inode)
                    .unwrap()
            }
            fn create_file(&self, name: &str, key: &[u8]) {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(self.path.join(name))
                    .unwrap();
                file.write_all(key).unwrap();
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = stdfs::set_permissions(&self.path, stdfs::Permissions::from_mode(0o700));
                let _ = stdfs::remove_dir_all(&self.path);
            }
        }

        fn injected() -> PrivateFileError {
            file_error(Errno::IO)
        }

        #[test]
        fn private_files_fault_publication_sequence_is_complete_before_final_name() {
            let fixture = Fixture::new();
            let mut stages = Vec::new();
            fixture
                .directory
                .publish_with("key", &[21; 32], |stage, file| {
                    stages.push(stage);
                    if stage == Stage::BeforeDirectorySync {
                        assert_eq!(stdfs::read(fixture.path.join("key")).unwrap(), [21; 32]);
                        assert_eq!(fixture.names(), ["key"]);
                    } else {
                        assert!(!fixture.path.join("key").exists());
                        let metadata = Metadata::read(file).unwrap();
                        assert_eq!(metadata.mode, 0o600);
                        assert_eq!(metadata.links, 1);
                        assert_eq!(
                            metadata.size,
                            if stage == Stage::BeforeWrite { 0 } else { 32 }
                        );
                    }
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                stages,
                [
                    Stage::BeforeWrite,
                    Stage::BeforeFileSync,
                    Stage::BeforeRename,
                    Stage::Rename,
                    Stage::BeforeDirectorySync
                ]
            );
        }

        #[test]
        fn private_files_fault_partial_write_cleans_only_its_own_temporary() {
            let fixture = Fixture::new();
            fixture.create_file(".lince-key-retained.tmp", &[77; 9]);
            let error = fixture
                .directory
                .publish_with("key", &[12; 32], |stage, file| {
                    if stage == Stage::BeforeWrite {
                        assert_eq!(io::write(file, &[12; 7]).unwrap(), 7);
                        return Err(injected());
                    }
                    Ok(())
                })
                .unwrap_err();
            assert_eq!(
                error,
                PublishError::BeforePublication {
                    error: injected(),
                    cleanup: None
                }
            );
            assert_eq!(fixture.names(), [".lince-key-retained.tmp"]);
            assert_eq!(
                stdfs::read(fixture.path.join(".lince-key-retained.tmp")).unwrap(),
                [77; 9]
            );
        }

        #[test]
        fn private_files_fault_file_sync_and_rename_never_publish() {
            for fail in [Stage::BeforeFileSync, Stage::Rename] {
                let fixture = Fixture::new();
                let result = fixture.directory.publish_with("key", &[3; 32], |stage, _| {
                    if stage == fail {
                        Err(injected())
                    } else {
                        Ok(())
                    }
                });
                assert_eq!(
                    result,
                    Err(PublishError::BeforePublication {
                        error: injected(),
                        cleanup: None
                    })
                );
                assert!(fixture.names().is_empty());
                assert_eq!(
                    fixture.directory.read_key32("key"),
                    Err(PrivateFileError::Missing)
                );
            }
        }

        #[test]
        fn private_files_fault_unsupported_rename_refuses_without_weak_fallback() {
            let fixture = Fixture::new();
            let result = fixture.directory.publish_with("key", &[4; 32], |stage, _| {
                if stage == Stage::Rename {
                    Err(required_syscall_error(Errno::NOSYS))
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                result,
                Err(PublishError::BeforePublication {
                    error: PrivateFileError::Unsupported,
                    cleanup: None
                })
            );
            assert!(fixture.names().is_empty());
            for error in [Errno::NOSYS, Errno::OPNOTSUPP, Errno::INVAL] {
                assert_eq!(required_syscall_error(error), PrivateFileError::Unsupported);
            }
        }

        #[test]
        fn private_files_unknown_result_after_real_rename_preserves_complete_final() {
            let fixture = Fixture::new();
            let mut calls = 0;
            let mut observed_temporary = None;
            let result = fixture.directory.publish_using(
                "key",
                &[0x5a; 32],
                |stage, _| {
                    assert!(!matches!(
                        stage,
                        Stage::BeforeCleanup
                            | Stage::BeforeCleanupSync
                            | Stage::BeforeDirectorySync
                    ));
                    Ok(())
                },
                |directory, temporary, destination| {
                    calls += 1;
                    observed_temporary = Some(temporary.to_owned());
                    fs::renameat_with(
                        directory,
                        temporary,
                        directory,
                        destination,
                        RenameFlags::NOREPLACE,
                    )
                    .unwrap();
                    Err(Errno::IO)
                },
            );
            let Err(PublishError::PublicationUnknown {
                error,
                temporary_name,
                destination_name,
            }) = result
            else {
                panic!("attempted rename IO must not claim known publication or absence")
            };
            assert_eq!(error, injected());
            assert_eq!(calls, 1);
            assert_eq!(Some(&temporary_name), observed_temporary.as_ref());
            assert_eq!(destination_name, "key");
            assert!(!fixture.path.join(temporary_name).exists());
            assert_eq!(fixture.names(), ["key"]);
            assert_eq!(
                fixture.directory.read_key32(&destination_name).unwrap(),
                [0x5a; 32]
            );
        }

        #[test]
        fn private_files_unknown_result_after_failed_real_rename_preserves_both_paths() {
            let fixture = Fixture::new();
            fixture.create_file("key", &[31; 32]);
            let mut calls = 0;
            let result = fixture.directory.publish_using(
                "key",
                &[32; 32],
                |stage, _| {
                    assert!(!matches!(
                        stage,
                        Stage::BeforeCleanup
                            | Stage::BeforeCleanupSync
                            | Stage::BeforeDirectorySync
                    ));
                    Ok(())
                },
                |directory, temporary, destination| {
                    calls += 1;
                    assert_eq!(
                        fs::renameat_with(
                            directory,
                            temporary,
                            directory,
                            destination,
                            RenameFlags::NOREPLACE
                        ),
                        Err(Errno::EXIST)
                    );
                    Err(Errno::IO)
                },
            );
            let Err(PublishError::PublicationUnknown {
                error,
                temporary_name,
                destination_name,
            }) = result
            else {
                panic!("reported IO cannot certify that the attempt did not apply")
            };
            assert_eq!(error, injected());
            assert_eq!(calls, 1);
            assert_eq!(fixture.names().len(), 2);
            assert_eq!(
                fixture.directory.read_key32(&temporary_name).unwrap(),
                [32; 32]
            );
            assert_eq!(
                fixture.directory.read_key32(&destination_name).unwrap(),
                [31; 32]
            );
        }

        #[test]
        fn private_files_unknown_attempt_without_final_retains_complete_temporary() {
            for returned_error in [Errno::IO, Errno::NOENT, Errno::ACCESS] {
                let fixture = Fixture::new();
                let mut calls = 0;
                let result = fixture.directory.publish_using(
                    "key",
                    &[33; 32],
                    |stage, _| {
                        assert!(!matches!(
                            stage,
                            Stage::BeforeCleanup
                                | Stage::BeforeCleanupSync
                                | Stage::BeforeDirectorySync
                        ));
                        Ok(())
                    },
                    |_, _, _| {
                        calls += 1;
                        Err(returned_error)
                    },
                );
                let Err(PublishError::PublicationUnknown {
                    error,
                    temporary_name,
                    destination_name,
                }) = result
                else {
                    panic!("unclassified attempted failures must preserve possible evidence")
                };
                assert_eq!(error, file_error(returned_error));
                assert_eq!(calls, 1);
                assert_eq!(fixture.names(), [temporary_name.clone()]);
                assert!(!fixture.path.join(destination_name).exists());
                assert_eq!(
                    fixture.directory.read_key32(&temporary_name).unwrap(),
                    [33; 32]
                );
            }
        }

        #[test]
        fn private_files_known_pre_call_failure_does_not_invoke_rename() {
            let fixture = Fixture::new();
            let mut calls = 0;
            let result = fixture.directory.publish_using(
                "key",
                &[34; 32],
                |stage, _| {
                    if stage == Stage::Rename {
                        Err(injected())
                    } else {
                        Ok(())
                    }
                },
                |_, _, _| {
                    calls += 1;
                    Ok(())
                },
            );
            assert_eq!(
                result,
                Err(PublishError::BeforePublication {
                    error: injected(),
                    cleanup: None
                })
            );
            assert_eq!(calls, 0);
            assert!(fixture.names().is_empty());
        }

        #[test]
        fn private_files_known_unsupported_syscall_result_is_not_publication_unknown() {
            for returned_error in [Errno::NOSYS, Errno::OPNOTSUPP, Errno::INVAL] {
                let fixture = Fixture::new();
                let mut calls = 0;
                let result = fixture.directory.publish_using(
                    "key",
                    &[35; 32],
                    |_, _| Ok(()),
                    |_, _, _| {
                        calls += 1;
                        Err(returned_error)
                    },
                );
                assert_eq!(
                    result,
                    Err(PublishError::BeforePublication {
                        error: PrivateFileError::Unsupported,
                        cleanup: None
                    })
                );
                assert_eq!(calls, 1);
                assert!(fixture.names().is_empty());
            }
        }

        #[test]
        fn private_files_fault_directory_sync_reports_published_uncertainty() {
            let fixture = Fixture::new();
            let result = fixture.directory.publish_with("key", &[5; 32], |stage, _| {
                if stage == Stage::BeforeDirectorySync {
                    Err(injected())
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                result,
                Err(PublishError::PublishedButUnconfirmed { error: injected() })
            );
            assert_eq!(fixture.names(), ["key"]);
            assert_eq!(fixture.directory.read_key32("key").unwrap(), [5; 32]);
            assert_eq!(
                fixture.directory.publish_new_key32("key", &[6; 32]),
                Err(PublishError::AlreadyExists { cleanup: None })
            );
            assert_eq!(fixture.directory.read_key32("key").unwrap(), [5; 32]);
        }

        #[test]
        fn private_files_fault_cleanup_failure_is_reported_with_exact_temporary_name() {
            let fixture = Fixture::new();
            let result = fixture.directory.publish_with("key", &[7; 32], |stage, _| {
                if matches!(stage, Stage::BeforeWrite | Stage::BeforeCleanup) {
                    Err(injected())
                } else {
                    Ok(())
                }
            });
            let Err(PublishError::BeforePublication {
                error,
                cleanup: Some(cleanup),
            }) = result
            else {
                panic!("cleanup failure must remain explicit")
            };
            assert_eq!(error, injected());
            assert_eq!(cleanup.error, injected());
            assert_eq!(fixture.names(), [cleanup.temporary_name.clone()]);
            assert!(cleanup.temporary_name.starts_with(".lince-key-"));
            assert!(!fixture.path.join("key").exists());
            assert_eq!(
                stdfs::metadata(fixture.path.join(cleanup.temporary_name))
                    .unwrap()
                    .len(),
                0
            );
        }

        #[test]
        fn private_files_fault_cleanup_sync_failure_does_not_claim_confirmed_cleanup() {
            let fixture = Fixture::new();
            let result = fixture.directory.publish_with("key", &[7; 32], |stage, _| {
                if matches!(stage, Stage::BeforeWrite | Stage::BeforeCleanupSync) {
                    Err(injected())
                } else {
                    Ok(())
                }
            });
            assert!(
                matches!(result, Err(PublishError::BeforePublication { error, cleanup: Some(CleanupFailure { error: cleanup_error, .. }) }) if error == injected() && cleanup_error == injected())
            );
            assert!(fixture.names().is_empty());
        }

        #[test]
        fn private_files_fault_existing_destination_retains_cleanup_failure_status() {
            let fixture = Fixture::new();
            fixture.create_file("key", &[1; 32]);
            let result = fixture.directory.publish_with("key", &[2; 32], |stage, _| {
                if stage == Stage::BeforeCleanup {
                    Err(injected())
                } else {
                    Ok(())
                }
            });
            assert!(matches!(
                result,
                Err(PublishError::AlreadyExists { cleanup: Some(_) })
            ));
            assert_eq!(stdfs::read(fixture.path.join("key")).unwrap(), [1; 32]);
            assert_eq!(fixture.names().len(), 2);
        }

        #[test]
        fn private_files_fault_temporary_path_replacement_is_neither_published_nor_deleted() {
            let fixture = Fixture::new();
            let mut replaced = None;
            let result = fixture
                .directory
                .publish_with("key", &[8; 32], |stage, file| {
                    if stage == Stage::BeforeRename {
                        let name = fixture.temporary_name(file);
                        stdfs::rename(
                            fixture.path.join(&name),
                            fixture.path.join("moved-original"),
                        )
                        .unwrap();
                        fixture.create_file(&name, &[9; 32]);
                        replaced = Some(name);
                    }
                    Ok(())
                });
            assert!(matches!(
                result,
                Err(PublishError::BeforePublication {
                    error: PrivateFileError::ChangedDuringOperation,
                    cleanup: Some(CleanupFailure {
                        error: PrivateFileError::ChangedDuringOperation,
                        ..
                    })
                })
            ));
            assert!(!fixture.path.join("key").exists());
            assert_eq!(
                stdfs::read(fixture.path.join(replaced.unwrap())).unwrap(),
                [9; 32]
            );
            assert_eq!(
                stdfs::read(fixture.path.join("moved-original")).unwrap(),
                [8; 32]
            );
        }

        #[test]
        fn private_files_fault_read_metadata_changes_refuse_even_when_new_mode_is_safe() {
            for change_at in [
                Stage::AfterOpenMetadata,
                Stage::AfterKeyRead,
                Stage::BeforeFinalMetadata,
            ] {
                let fixture = Fixture::new();
                fixture
                    .directory
                    .publish_new_key32("key", &[10; 32])
                    .unwrap();
                let result = fixture.directory.read_with("key", |stage, file| {
                    if stage == change_at {
                        fs::fchmod(file, Mode::RUSR).unwrap();
                    }
                    Ok(())
                });
                assert_eq!(result, Err(PrivateFileError::ChangedDuringOperation));
                assert_eq!(fixture.directory.read_key32("key").unwrap(), [10; 32]);
            }
        }

        #[test]
        fn private_files_fault_growth_is_caught_by_fixed_one_byte_eof_check() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[11; 32])
                .unwrap();
            let result = fixture.directory.read_with("key", |stage, _| {
                if stage == Stage::AfterKeyRead {
                    OpenOptions::new()
                        .append(true)
                        .open(fixture.path.join("key"))
                        .unwrap()
                        .write_all(&[12])
                        .unwrap();
                }
                Ok(())
            });
            assert_eq!(result, Err(PrivateFileError::ChangedDuringOperation));
            assert_eq!(stdfs::metadata(fixture.path.join("key")).unwrap().len(), 33);
        }

        #[test]
        fn private_files_fault_truncation_after_metadata_refuses_partial_key() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[12; 32])
                .unwrap();
            let result = fixture.directory.read_with("key", |stage, _| {
                if stage == Stage::AfterOpenMetadata {
                    OpenOptions::new()
                        .write(true)
                        .open(fixture.path.join("key"))
                        .unwrap()
                        .set_len(7)
                        .unwrap();
                }
                Ok(())
            });
            assert_eq!(result, Err(PrivateFileError::ChangedDuringOperation));
        }

        #[test]
        fn private_files_fault_child_replacement_during_read_refuses_detached_old_key() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[13; 32])
                .unwrap();
            let result = fixture.directory.read_with("key", |stage, _| {
                if stage == Stage::BeforeFinalMetadata {
                    stdfs::rename(fixture.path.join("key"), fixture.path.join("old")).unwrap();
                    fixture.create_file("key", &[14; 32]);
                }
                Ok(())
            });
            assert_eq!(result, Err(PrivateFileError::ChangedDuringOperation));
            assert_eq!(fixture.directory.read_key32("key").unwrap(), [14; 32]);
        }

        #[test]
        fn private_files_fault_directory_safety_change_during_read_is_rechecked() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[15; 32])
                .unwrap();
            let result = fixture.directory.read_with("key", |stage, _| {
                if stage == Stage::BeforeFinalMetadata {
                    stdfs::set_permissions(&fixture.path, stdfs::Permissions::from_mode(0o755))
                        .unwrap();
                }
                Ok(())
            });
            assert_eq!(result, Err(PrivateFileError::UnsafeDirectory));
        }

        #[test]
        fn private_files_metadata_validators_refuse_other_owners_without_privileged_chown() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[16; 32])
                .unwrap();
            let owner = geteuid().as_raw();
            let directory = Metadata::read(&fixture.directory.directory).unwrap();
            assert_eq!(
                directory.require_directory(owner.wrapping_add(1)),
                Err(PrivateFileError::UnsafeDirectory)
            );
            let file = fs::openat(
                &fixture.directory.directory,
                "key",
                OFlags::RDONLY,
                Mode::empty(),
            )
            .unwrap();
            let metadata = Metadata::read(&file).unwrap();
            assert_eq!(
                metadata.require_key(owner.wrapping_add(1)),
                Err(PrivateFileError::UnsafeKey)
            );
            let mut corrupt = metadata;
            corrupt.owner = owner.wrapping_add(1);
            assert_eq!(corrupt.require_key(owner), Err(PrivateFileError::UnsafeKey));
            for kind in [
                FileType::Directory,
                FileType::Fifo,
                FileType::Socket,
                FileType::CharacterDevice,
                FileType::BlockDevice,
                FileType::Symlink,
                FileType::Unknown,
            ] {
                corrupt = metadata;
                corrupt.kind = kind;
                assert_eq!(corrupt.require_key(owner), Err(PrivateFileError::UnsafeKey));
            }
            for size in [-1, 0, 31, 33, i64::MAX] {
                corrupt = metadata;
                corrupt.size = size;
                assert_eq!(corrupt.require_key(owner), Err(PrivateFileError::UnsafeKey));
            }
            for links in [0, 2, u64::MAX] {
                corrupt = metadata;
                corrupt.links = links;
                assert_eq!(corrupt.require_key(owner), Err(PrivateFileError::UnsafeKey));
            }
        }

        #[test]
        fn private_files_metadata_fingerprint_excludes_only_read_induced_atime() {
            let fixture = Fixture::new();
            fixture
                .directory
                .publish_new_key32("key", &[17; 32])
                .unwrap();
            let file = fs::openat(
                &fixture.directory.directory,
                "key",
                OFlags::RDONLY,
                Mode::empty(),
            )
            .unwrap();
            let original = fs::fstat(&file).unwrap();
            let expected = Metadata::from_stat(original);
            let mut changed = original;
            changed.st_atime = changed.st_atime.wrapping_add(1);
            assert_eq!(Metadata::from_stat(changed), expected);
            changed = original;
            changed.st_mtime = changed.st_mtime.wrapping_add(1);
            assert_ne!(Metadata::from_stat(changed), expected);
            changed = original;
            changed.st_ctime = changed.st_ctime.wrapping_add(1);
            assert_ne!(Metadata::from_stat(changed), expected);
            changed = original;
            changed.st_gid = changed.st_gid.wrapping_add(1);
            assert_ne!(Metadata::from_stat(changed), expected);
        }
    }
}
