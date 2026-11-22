use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd},
        linux::net::SocketAddrExt,
        unix::{
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{SocketAddr, UnixListener},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

use rustix::process::{Pid, PidfdFlags, Signal, kill_process_group, pidfd_open};
use smithay::reexports::calloop::{
    Interest, LoopHandle, Mode, PostAction, RegistrationToken, generic::Generic,
};

use crate::{launch::X11Display, server::Server};

const SATELLITE: &str = "xwayland-satellite";
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub(crate) struct Bridge<'event, Data> {
    handle: LoopHandle<'event, Data>,
    sockets: Sockets,
    authorization: tempfile::NamedTempFile,
    wayland: PathBuf,
    pending: Rc<Cell<bool>>,
    child_exited: Rc<Cell<bool>>,
    listeners: Vec<RegistrationToken>,
    child: Option<Child>,
    child_watch: Option<RegistrationToken>,
    retry_at: Option<Instant>,
}

impl<'event, Data: 'event> Bridge<'event, Data> {
    pub(crate) fn setup(
        handle: &LoopHandle<'event, Data>,
        server: &mut Server,
        wayland: &Path,
    ) -> Option<Self> {
        match Self::new(handle, wayland) {
            Ok(bridge) => {
                let display = bridge.display();
                eprintln!("Park X11 display: {}", display.name);
                server.x11_display(display);
                Some(bridge)
            }
            Err(error) => {
                eprintln!("Park Xwayland is unavailable: {error}");
                None
            }
        }
    }

    fn new(handle: &LoopHandle<'event, Data>, wayland: &Path) -> io::Result<Self> {
        let runtime = wayland
            .parent()
            .ok_or_else(|| io::Error::other("Park's Wayland socket has no runtime directory"))?;
        let sockets = Sockets::reserve(Path::new("/tmp"))?;
        let mut authorization = tempfile::Builder::new()
            .prefix(".park-xauthority-")
            .tempfile_in(runtime)?;
        write_authority(&mut authorization, sockets.number)?;
        let supported = Command::new(SATELLITE)
            .arg(format!(":{}", sockets.number))
            .arg("-auth")
            .arg(authorization.path())
            .arg("--test-listenfd-support")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !supported.success() {
            return Err(io::Error::other(
                "xwayland-satellite must support socket activation and X11 authorization",
            ));
        }
        let mut bridge = Self {
            handle: handle.clone(),
            sockets,
            authorization,
            wayland: wayland.to_owned(),
            pending: Default::default(),
            child_exited: Default::default(),
            listeners: Vec::new(),
            child: None,
            child_watch: None,
            retry_at: None,
        };
        bridge.watch()?;
        Ok(bridge)
    }

    fn display(&self) -> X11Display {
        X11Display {
            name: format!(":{}", self.sockets.number),
            authorization: self.authorization.path().to_owned(),
        }
    }

    fn watch(&mut self) -> io::Result<()> {
        for socket in &self.sockets.listeners {
            let fd = socket.as_fd().try_clone_to_owned()?;
            let pending = self.pending.clone();
            let token = self
                .handle
                .insert_source(
                    Generic::new(fd, Interest::READ, Mode::Level),
                    move |_, _, _| {
                        pending.set(true);
                        Ok(PostAction::Disable)
                    },
                )
                .map_err(io::Error::other)?;
            self.listeners.push(token);
        }
        Ok(())
    }

    pub(crate) fn advance(&mut self) {
        if let Some(child) = &mut self.child {
            if self.child_watch.is_some() && !self.child_exited.replace(false) {
                return;
            }
            match child.try_wait() {
                Ok(None) => return,
                Ok(Some(status)) => eprintln!("Park xwayland-satellite exited with {status}"),
                Err(error) => {
                    eprintln!("Park could not read xwayland-satellite status: {error}");
                    if let Some(watch) = self.child_watch.take() {
                        self.handle.remove(watch);
                    }
                    return;
                }
            }
            self.child = None;
            if let Some(watch) = self.child_watch.take() {
                self.handle.remove(watch);
            }
            self.schedule_retry();
        }
        if self.retry_at.is_some_and(|retry| retry <= Instant::now()) {
            if let Err(error) = self.watch() {
                for watch in self.listeners.drain(..) {
                    self.handle.remove(watch);
                }
                eprintln!("Park could not rearm Xwayland: {error}");
                self.retry_at = Some(Instant::now() + RETRY_DELAY);
            } else {
                self.retry_at = None;
            }
        }
        if self.pending.replace(false) {
            for watch in self.listeners.drain(..) {
                self.handle.remove(watch);
            }
            if let Err(error) = self.spawn() {
                eprintln!("Park could not start xwayland-satellite: {error}");
                self.schedule_retry();
            }
        }
    }

    fn schedule_retry(&mut self) {
        // Drop requests stranded by the failed server. New connections made
        // during the retry delay can stay queued for the replacement.
        if let Err(error) = self.sockets.drain() {
            eprintln!("Park could not clear pending X11 connections: {error}");
        }
        self.retry_at = Some(Instant::now() + RETRY_DELAY);
    }

    fn spawn(&mut self) -> io::Result<()> {
        let [unix, abstract_socket] = &self.sockets.listeners;
        let unix = unix.as_fd().try_clone_to_owned()?;
        let abstract_socket = abstract_socket.as_fd().try_clone_to_owned()?;
        let display = self.display();
        let mut command = Command::new(SATELLITE);
        command
            .arg(display.name)
            .args(["-nolisten", "tcp", "-auth"])
            .arg(&display.authorization)
            .arg("-listenfd")
            .arg(unix.as_raw_fd().to_string())
            .arg("-listenfd")
            .arg(abstract_socket.as_raw_fd().to_string())
            .env("WAYLAND_DISPLAY", &self.wayland)
            .env("XAUTHORITY", display.authorization)
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .env_remove("XDG_ACTIVATION_TOKEN")
            .env_remove("NOTIFY_SOCKET")
            .stdin(Stdio::null())
            .process_group(0);
        crate::launch::restore_graphics_environment(&mut command, |name| std::env::var_os(name))?;
        // SAFETY: Only the two captured socket descriptors are passed across exec.
        // The fcntl calls run in the child and neither allocate nor lock.
        unsafe {
            command.pre_exec(move || {
                rustix::io::fcntl_setfd(&unix, rustix::io::FdFlags::empty())?;
                rustix::io::fcntl_setfd(&abstract_socket, rustix::io::FdFlags::empty())?;
                Ok(())
            });
        }
        let child = command.spawn()?;
        let exited = self.child_exited.clone();
        self.child_watch = child_pid(&child)
            .and_then(|pid| pidfd_open(pid, PidfdFlags::empty()).ok())
            .and_then(|fd| {
                self.handle
                    .insert_source(
                        Generic::new(fd, Interest::READ, Mode::Level),
                        move |_, _, _| {
                            exited.set(true);
                            Ok(PostAction::Disable)
                        },
                    )
                    .ok()
            });
        self.child = Some(child);
        Ok(())
    }

    pub(crate) fn timeout(&self) -> Option<Duration> {
        if self.child.is_some() && self.child_watch.is_none() {
            Some(RETRY_DELAY)
        } else {
            self.retry_at
                .map(|retry| retry.saturating_duration_since(Instant::now()))
        }
    }
}

impl<Data> Drop for Bridge<'_, Data> {
    fn drop(&mut self) {
        for watch in self.listeners.drain(..).chain(self.child_watch.take()) {
            self.handle.remove(watch);
        }
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = child_pid(&child) {
                let _ = kill_process_group(pid, Signal::KILL);
            }
            let _ = child.wait();
        }
    }
}

fn child_pid(child: &Child) -> Option<Pid> {
    Pid::from_raw(i32::try_from(child.id()).ok()?)
}

struct Sockets {
    number: u32,
    listeners: [UnixListener; 2],
    _socket: RemoveFile,
    _lock: RemoveFile,
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl Sockets {
    fn reserve(temporary: &Path) -> io::Result<Self> {
        let directory = temporary.join(".X11-unix");
        match fs::DirBuilder::new().mode(0o1777).create(&directory) {
            Ok(()) => fs::set_permissions(&directory, fs::Permissions::from_mode(0o1777))?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir()
            || ![0, rustix::process::geteuid().as_raw()].contains(&metadata.uid())
            || metadata.mode() & 0o1022 != 0o1022
        {
            return Err(io::Error::other(
                "The X11 socket directory has unsafe ownership or permissions",
            ));
        }
        // Reserve both Linux socket names under the standard X11 display lock.
        // Like Niri, leave existing display locks alone.
        for number in 0..100 {
            let lock = temporary.join(format!(".X{number}-lock"));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o444)
                .open(&lock)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let lock = RemoveFile(lock);
            writeln!(file, "{:>10}", std::process::id())?;
            let path = directory.join(format!("X{number}"));
            let abstract_address =
                SocketAddr::from_abstract_name(path.as_os_str().as_encoded_bytes())?;
            let abstract_socket = match UnixListener::bind_addr(&abstract_address) {
                Ok(listener) => listener,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
                Err(error) => return Err(error),
            };
            let unix = match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
                Err(error) => return Err(error),
            };
            return Ok(Self {
                number,
                listeners: [unix, abstract_socket],
                _socket: RemoveFile(path),
                _lock: lock,
            });
        }
        Err(io::Error::other("No free local X11 display was found"))
    }

    fn drain(&self) -> io::Result<()> {
        for listener in &self.listeners {
            listener.set_nonblocking(true)?;
            for _ in 0..128 {
                if listener.accept().is_err() {
                    break;
                }
            }
            listener.set_nonblocking(false)?;
        }
        Ok(())
    }
}

fn write_authority(mut file: impl Write, number: u32) -> io::Result<()> {
    let mut cookie = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut cookie)?;
    // Xauthority stores big-endian, length-prefixed fields. FamilyWild with a
    // display number lets a local sandbox with a different hostname use the cookie.
    file.write_all(&u16::MAX.to_be_bytes())?;
    for field in [
        b"".as_slice(),
        number.to_string().as_bytes(),
        b"MIT-MAGIC-COOKIE-1",
        &cookie,
    ] {
        let length = u16::try_from(field.len()).map_err(io::Error::other)?;
        file.write_all(&length.to_be_bytes())?;
        file.write_all(field)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use super::*;

    #[test]
    fn reservations_skip_busy_displays_and_clean_up_only_their_own_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let first = Sockets::reserve(root).unwrap();
        let second = Sockets::reserve(root).unwrap();
        let first_socket = root.join(".X11-unix/X0");
        let second_socket = root.join(".X11-unix/X1");
        assert_eq!(first.number, 0);
        assert_eq!(second.number, 1);
        assert_eq!(
            fs::read_to_string(root.join(".X0-lock")).unwrap(),
            format!("{:>10}\n", std::process::id())
        );
        assert!(UnixStream::connect(&second_socket).is_ok());
        drop(second);
        assert!(!second_socket.exists());
        assert!(!root.join(".X1-lock").exists());
        assert!(UnixStream::connect(&first_socket).is_ok());
        assert!(root.join(".X0-lock").exists());
    }
}
