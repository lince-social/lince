use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::{fd::{AsFd, AsRawFd, BorrowedFd}, unix::{fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt}, process::CommandExt}},
    path::{Path, PathBuf},
    process::Command,
};
use sha2::{Digest, Sha256};
use crate::{authority::ProcessIdentity, checkpoint::{CheckpointScope, ExecutableIdentity, invalid}};

const MAX_EXECUTABLE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

fn native_target() -> String {
    format!("{}-unknown-linux-{}", std::env::consts::ARCH, if cfg!(target_env = "musl") { "musl" } else { "gnu" })
}

fn hash_file(file: &File) -> io::Result<String> {
    let size = file.metadata()?.len();
    if size == 0 || size > MAX_EXECUTABLE_BYTES { return Err(invalid("Executable size exceeds refresh limits")); }
    let mut digest = Sha256::new();
    let mut offset = 0;
    let mut bytes = [0_u8; 65536];
    while offset < size {
        let length = ((size - offset) as usize).min(bytes.len());
        file.read_exact_at(&mut bytes[..length], offset)?;
        digest.update(&bytes[..length]);
        offset += length as u64;
    }
    if file.metadata()?.len() != size { return Err(invalid("Executable changed while hashing")); }
    Ok(digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

fn elf(file: &File) -> io::Result<()> {
    let mut header = [0_u8; 20];
    file.read_exact_at(&mut header, 0)?;
    if &header[..4] != b"\x7fELF" || header[4] != if cfg!(target_pointer_width = "64") { 2 } else { 1 }
        || header[5] != if cfg!(target_endian = "little") { 1 } else { 2 } {
        return Err(invalid("Refresh requires a native ELF executable"));
    }
    let machine = if cfg!(target_endian = "little") { u16::from_le_bytes([header[18], header[19]]) } else { u16::from_be_bytes([header[18], header[19]]) };
    let expected = match std::env::consts::ARCH { "x86_64" => 62, "aarch64" => 183, "x86" => 3, "riscv64" => 243, _ => return Err(invalid("Unsupported refresh executable architecture")) };
    if machine != expected { return Err(invalid("Refresh executable has a different machine architecture")); }
    Ok(())
}

pub struct VerifiedImage {
    file: File,
    path: PathBuf,
    identity: ExecutableIdentity,
}

impl std::fmt::Debug for VerifiedImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("VerifiedImage").field("identity", &self.identity).finish_non_exhaustive()
    }
}

impl VerifiedImage {
    pub fn open(path: &Path, expected: &ExecutableIdentity, scope: CheckpointScope) -> io::Result<Self> {
        expected.validate()?;
        if !path.is_absolute() || expected.target != native_target() { return Err(invalid("Refresh executable path or target is invalid")); }
        let path = fs::canonicalize(path)?;
        let file = OpenOptions::new().read(true).custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32).open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 { return Err(invalid("Refresh image is not an executable file")); }
        if scope == CheckpointScope::FullSession && (!path.starts_with("/nix/store") || metadata.uid() != 0
            || metadata.permissions().mode() & 0o222 != 0 || expected.revision.is_none()) {
            return Err(invalid("Full session refresh requires an immutable identified Nix image"));
        }
        if metadata.dev() != expected.device || metadata.ino() != expected.inode { return Err(invalid("Refresh executable file identity changed")); }
        elf(&file)?;
        if hash_file(&file)? != expected.sha256 { return Err(invalid("Refresh executable checksum differs from its candidate")); }
        let (file, identity) = if scope == CheckpointScope::ProtocolExperiment {
            let descriptor = rustix::fs::memfd_create("lince-refresh-private-image", rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING)?;
            let mut frozen = File::from(descriptor);
            let mut offset = 0;
            let mut bytes = [0_u8; 65536];
            while offset < metadata.len() {
                let length = ((metadata.len() - offset) as usize).min(bytes.len());
                file.read_exact_at(&mut bytes[..length], offset)?;
                frozen.write_all(&bytes[..length])?;
                offset += length as u64;
            }
            rustix::fs::fcntl_add_seals(&frozen, rustix::fs::SealFlags::WRITE | rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::GROW | rustix::fs::SealFlags::SEAL)?;
            if hash_file(&frozen)? != expected.sha256 { return Err(invalid("Private image changed while freezing it")); }
            let metadata = frozen.metadata()?;
            let mut identity = expected.clone();
            identity.device = metadata.dev(); identity.inode = metadata.ino();
            (frozen, identity)
        } else { (file, expected.clone()) };
        Ok(Self { file, path, identity })
    }

    pub fn identity(&self) -> &ExecutableIdentity { &self.identity }
    pub fn path(&self) -> &Path { &self.path }
    pub fn descriptor(&self) -> BorrowedFd<'_> { self.file.as_fd() }

    pub fn observe_process(owner: &ProcessIdentity, expected: &ExecutableIdentity) -> io::Result<()> {
        if ProcessIdentity::observe(owner.pid)? != *owner { return Err(invalid("Refresh session owner identity changed")); }
        let path = format!("/proc/{}/exe", owner.pid);
        let file = File::open(&path)?;
        let metadata = file.metadata()?;
        if metadata.dev() != expected.device || metadata.ino() != expected.inode || hash_file(&file)? != expected.sha256 {
            return Err(invalid("The active process is running a different executable"));
        }
        let after = fs::metadata(path)?;
        if after.dev() != metadata.dev() || after.ino() != metadata.ino() || ProcessIdentity::observe(owner.pid)? != *owner {
            return Err(invalid("Refresh process changed while verifying its executable"));
        }
        Ok(())
    }

    pub fn replace_current(&self, arguments: &[OsString], environment: &[(OsString, OsString)]) -> io::Error {
        let thread = match fs::read_to_string("/proc/thread-self/stat") {
            Ok(thread) => thread,
            Err(error) => return error,
        };
        let thread_id = thread.split_whitespace().next().and_then(|value| value.parse::<u32>().ok());
        if thread_id != Some(std::process::id()) { return invalid("Replace Park from its main thread after quiescing its owners"); }
        let mut command = Command::new(format!("/proc/self/fd/{}", self.file.as_raw_fd()));
        command.arg0(&self.path).args(arguments).env_clear().envs(environment.iter().map(|(key, value)| (key, value)));
        command.exec()
    }
}
