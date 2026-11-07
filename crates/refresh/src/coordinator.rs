use crate::checkpoint::invalid;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    os::{
        fd::AsFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_CONTROL_BYTES: usize = 4096;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Status,
    Shutdown,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: Operation,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub schema: u32,
    pub scope: String,
    pub coordinator_pid: u32,
    pub stage: String,
    pub checkpoint_transport_available: bool,
    pub full_session_supported: bool,
    pub swap_complete: bool,
    pub error: Option<String>,
}

impl Status {
    fn idle() -> Self {
        Self {
            schema: 1,
            scope: "refresh_coordinator".into(),
            coordinator_pid: std::process::id(),
            stage: "idle".into(),
            checkpoint_transport_available: true,
            full_session_supported: false,
            swap_complete: false,
            error: None,
        }
    }
}

fn validate_runtime(runtime: &Path, create: bool) -> io::Result<()> {
    if !runtime.is_absolute()
        || runtime
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(invalid(
            "Refresh runtime directory must be absolute without parent traversal",
        ));
    }
    if create {
        match fs::DirBuilder::new().mode(0o700).create(runtime) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = fs::symlink_metadata(runtime)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Refresh runtime directory must be private and owned by this user",
        ));
    }
    Ok(())
}

fn authority_lock(runtime: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(runtime.join("authority.lock"))?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.nlink() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Invalid refresh authority lock",
        ));
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    Ok(lock)
}

struct SocketCleanup {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.file_type().is_socket()
                && metadata.dev() == self.device
                && metadata.ino() == self.inode
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn read_message(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut reader = BufReader::new(stream).take(MAX_CONTROL_BYTES as u64 + 1);
        reader.read_until(b'\n', &mut bytes)?;
    }
    if bytes.len() > MAX_CONTROL_BYTES || bytes.last() != Some(&b'\n') {
        return Err(invalid("Incomplete or excessive refresh control message"));
    }
    Ok(bytes)
}

fn write_message(stream: &mut UnixStream, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= MAX_CONTROL_BYTES {
        return Err(invalid("Refresh control response exceeds limit"));
    }
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

fn control(stream: &mut UnixStream) -> io::Result<bool> {
    let peer = rustix::net::sockopt::socket_peercred(stream.as_fd())?;
    if peer.uid != rustix::process::geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Refresh control peer has a different owner",
        ));
    }
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let request: Request = serde_json::from_slice(&read_message(stream)?)?;
    let stop = matches!(request.operation, Operation::Shutdown);
    let mut status = Status::idle();
    if stop {
        status.stage = "stopped".into();
    }
    write_message(stream, &status)?;
    Ok(stop)
}

pub fn serve(runtime: &Path) -> io::Result<()> {
    validate_runtime(runtime, true)?;
    let _authority = authority_lock(runtime)?;
    let path = runtime.join("control.sock");
    match fs::symlink_metadata(&path) {
        Ok(metadata)
            if metadata.file_type().is_socket()
                && metadata.uid() == rustix::process::geteuid().as_raw() =>
        {
            fs::remove_file(&path)?
        }
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Refresh control path is occupied",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(&path)?;
    let metadata = fs::symlink_metadata(&path)?;
    let _cleanup = SocketCleanup {
        path: path.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    loop {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        match control(&mut stream) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(_) => {
                let mut status = Status::idle();
                status.error = Some("Refresh control request rejected".into());
                let _ = write_message(&mut stream, &status);
            }
        }
    }
}

pub fn request(runtime: &Path, stop: bool) -> io::Result<Status> {
    validate_runtime(runtime, false)?;
    let mut stream = UnixStream::connect(runtime.join("control.sock"))?;
    let credentials = rustix::net::sockopt::socket_peercred(&stream)?;
    if credentials.uid != rustix::process::geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Refresh coordinator has a different owner",
        ));
    }
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    write_message(
        &mut stream,
        &Request {
            operation: if stop {
                Operation::Shutdown
            } else {
                Operation::Status
            },
        },
    )?;
    let status: Status = serde_json::from_slice(&read_message(&mut stream)?)?;
    if status.schema != 1
        || status.scope != "refresh_coordinator"
        || status.coordinator_pid != credentials.pid.as_raw_pid() as u32
    {
        return Err(invalid(
            "Refresh response identity differs from its coordinator",
        ));
    }
    if let Some(error) = &status.error {
        return Err(io::Error::other(error.clone()));
    }
    Ok(status)
}
