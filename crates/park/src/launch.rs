use std::{
    ffi::{OsStr, OsString},
    io,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[cfg_attr(
    feature = "native-presentation",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "native-presentation", serde(rename_all = "kebab-case"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Clipboard,
    WindowList,
    WindowControl,
    OutputControl,
    Capture,
    InputMethod,
}

#[cfg_attr(
    feature = "native-presentation",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "native-presentation", serde(deny_unknown_fields))]
pub struct Description {
    pub program: String,
    #[cfg_attr(feature = "native-presentation", serde(default))]
    pub arguments: Vec<String>,
    #[cfg_attr(feature = "native-presentation", serde(default))]
    pub directory: Option<PathBuf>,
    #[cfg_attr(feature = "native-presentation", serde(default))]
    pub capabilities: Vec<Capability>,
}

impl Description {
    pub fn valid(&self) -> bool {
        !self.program.is_empty()
            && self.capabilities.len() <= 6
            && self.program.len() <= 4096
            && !self.program.contains('\0')
            && self.arguments.len() <= 4096
            && self
                .arguments
                .iter()
                .all(|argument| !argument.contains('\0'))
            && self.arguments.iter().map(String::len).sum::<usize>() <= 262144
    }

    pub fn application(&self) -> io::Result<Application> {
        if !self.valid() {
            return Err(io::Error::other("invalid application launch description"));
        }
        let mut application = Application::new(&self.program)?;
        for argument in &self.arguments {
            application = application.arg(argument);
        }
        if let Some(directory) = &self.directory {
            application = application.current_dir(directory);
        }
        Ok(application)
    }
}

pub struct Application {
    program: OsString,
    arguments: Vec<OsString>,
    directory: Option<PathBuf>,
}

fn display_name<'a>(socket: &'a Path, runtime: Option<&Path>) -> &'a OsStr {
    if socket.parent() == runtime {
        socket.file_name().unwrap_or(socket.as_os_str())
    } else {
        socket.as_os_str()
    }
}

fn restore_graphics_environment(
    command: &mut Command,
    value: impl Fn(&str) -> Option<OsString>,
) -> io::Result<()> {
    for name in [
        "LD_LIBRARY_PATH",
        "GBM_BACKENDS_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
    ] {
        let saved = format!("LINCE_HOST_{name}");
        if let Some(original) = value(&saved) {
            use std::os::unix::ffi::{OsStrExt, OsStringExt};
            let original = original.as_bytes();
            match original.split_first() {
                Some((b'0', [])) => {
                    command.env_remove(name);
                }
                Some((b'1', bytes)) => {
                    command.env(name, OsString::from_vec(bytes.to_vec()));
                }
                _ => {
                    return Err(io::Error::other(format!(
                        "Invalid saved environment for {name}"
                    )));
                }
            }
        }
        command.env_remove(saved);
    }
    Ok(())
}

impl Application {
    pub fn new(program: impl Into<OsString>) -> io::Result<Self> {
        let program = program.into();
        if program.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "application executable must not be empty",
            ));
        }
        Ok(Self {
            program,
            arguments: Vec::new(),
            directory: None,
        })
    }

    pub fn arg(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    pub fn current_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    pub fn command(&self, socket: &Path) -> io::Result<Command> {
        if !socket.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Park requires an absolute Wayland socket path",
            ));
        }
        let mut command = Command::new(&self.program);
        restore_graphics_environment(&mut command, |key| std::env::var_os(key))?;
        command.args(&self.arguments);
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        command
            .env("WAYLAND_DISPLAY", display_name(socket, runtime.as_deref()))
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("SDL_VIDEODRIVER", "wayland")
            .env("SDL_VIDEO_DRIVER", "wayland")
            .env("XDG_SESSION_TYPE", "wayland")
            .env("XDG_CURRENT_DESKTOP", "Lince")
            .env("XDG_SESSION_DESKTOP", "Lince")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .env_remove("XDG_ACTIVATION_TOKEN")
            .env_remove("NIRI_SOCKET")
            .env_remove("SWAYSOCK")
            .env_remove("I3SOCK")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("WAYFIRE_SOCKET")
            .env_remove("WAYLAND_SOCKET");
        Ok(command)
    }

    pub fn spawn(&self, socket: &Path) -> io::Result<Child> {
        let mut command = self.command(socket)?;
        if !socket.metadata()?.file_type().is_socket() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Park display path is not a Unix socket",
            ));
        }
        command.spawn()
    }

    #[cfg(feature = "server")]
    pub(crate) fn spawn_connected(
        &self,
        socket: &Path,
        connection: std::os::unix::net::UnixStream,
    ) -> io::Result<Child> {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        let mut command = self.command(socket)?;
        command.env("WAYLAND_SOCKET", connection.as_raw_fd().to_string());
        unsafe {
            command.pre_exec(move || {
                rustix::io::fcntl_setfd(&connection, rustix::io::FdFlags::empty())?;
                Ok(())
            });
        }
        command.spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, ffi::OsStr};

    #[test]
    fn graphics_environment_preserves_unset_empty_and_literal_overrides() {
        let mut command = Command::new("sh");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("LINCE_HOST_LD_LIBRARY_PATH", "0")
            .env("LINCE_HOST_GBM_BACKENDS_PATH", "1")
            .env("LINCE_HOST___EGL_VENDOR_LIBRARY_DIRS", "1original:$value with spaces")
            .env("LD_LIBRARY_PATH", "private");
        restore_graphics_environment(&mut command, |key| match key {
            "LINCE_HOST_LD_LIBRARY_PATH" => Some("0".into()),
            "LINCE_HOST_GBM_BACKENDS_PATH" => Some("1".into()),
            "LINCE_HOST___EGL_VENDOR_LIBRARY_DIRS" => Some("1original:$value with spaces".into()),
            _ => None,
        })
        .unwrap();
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert!(environment.get(OsStr::new("LD_LIBRARY_PATH")).is_none());
        assert_eq!(
            environment.get(OsStr::new("GBM_BACKENDS_PATH")),
            Some(&Some(OsStr::new("")))
        );
        assert_eq!(
            environment.get(OsStr::new("__EGL_VENDOR_LIBRARY_DIRS")),
            Some(&Some(OsStr::new("original:$value with spaces")))
        );
        assert!(
            environment
                .iter()
                .filter(|(key, _)| key.to_string_lossy().starts_with("LINCE_HOST_"))
                .all(|(_, value)| value.is_none())
        );
        drop(environment);
        command.args([
            "-c",
            "exec /proc/self/exe -c 'test \"${LD_LIBRARY_PATH+x}\" != x && test \"${GBM_BACKENDS_PATH+x}\" = x && test -z \"$GBM_BACKENDS_PATH\" && test \"${LINCE_HOST_LD_LIBRARY_PATH+x}\" != x && printf %s \"$__EGL_VENDOR_LIBRARY_DIRS\"'",
        ]);
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"original:$value with spaces");
    }

    #[test]
    fn unpackaged_launch_inherits_host_graphics_and_rejects_invalid_capture() {
        let mut command = Command::new("native-client");
        restore_graphics_environment(&mut command, |_| None).unwrap();
        assert!(!command.get_envs().any(|(key, _)| key == "LD_LIBRARY_PATH"));
        assert!(restore_graphics_environment(&mut command, |_| Some("invalid".into())).is_err());
    }

    #[test]
    fn launch_uses_park_display_and_removes_inherited_display_routes() {
        let command = Application::new("native-client")
            .unwrap()
            .command(Path::new("/run/user/1000/park-0"))
            .unwrap();
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        for key in [
            "DISPLAY",
            "XAUTHORITY",
            "WAYLAND_SOCKET",
            "NIRI_SOCKET",
            "SWAYSOCK",
            "I3SOCK",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "WAYFIRE_SOCKET",
        ] {
            assert_eq!(environment.get(OsStr::new(key)), Some(&None));
        }
        assert_eq!(
            environment.get(OsStr::new("WAYLAND_DISPLAY")),
            Some(&Some(display_name(
                Path::new("/run/user/1000/park-0"),
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .as_deref()
            )))
        );
        for key in [
            "GDK_BACKEND",
            "QT_QPA_PLATFORM",
            "SDL_VIDEODRIVER",
            "SDL_VIDEO_DRIVER",
        ] {
            assert_eq!(
                environment.get(OsStr::new(key)),
                Some(&Some(OsStr::new("wayland")))
            );
        }
    }

    #[test]
    fn display_names_use_the_runtime_socket_name_and_keep_external_paths_absolute() {
        let socket = Path::new("/run/user/1000/wayland-park-0");
        assert_eq!(
            display_name(socket, Some(Path::new("/run/user/1000"))),
            "wayland-park-0"
        );
        assert_eq!(
            display_name(socket, Some(Path::new("/different/runtime"))),
            socket.as_os_str()
        );
        assert_eq!(display_name(socket, None), socket.as_os_str());
    }

    #[test]
    fn application_arguments_remain_literal_and_working_directory_is_preserved() {
        let argument = "https://example.test/$(touch unexpected); spaces";
        let command = Application::new("native-client")
            .unwrap()
            .arg(argument)
            .current_dir("/tmp")
            .command(Path::new("/run/user/1000/park-0"))
            .unwrap();
        assert_eq!(command.get_program(), OsStr::new("native-client"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new(argument)]
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp")));
    }

    #[test]
    fn missing_and_invalid_display_paths_prevent_launch() {
        let application = Application::new("native-client").unwrap();
        assert!(application.command(Path::new("park-0")).is_err());
        assert!(application.spawn(Path::new("/dev/null")).is_err());
        assert!(application.spawn(Path::new("/dev/null/park-0")).is_err());
        assert!(Application::new("").is_err());
    }
}
