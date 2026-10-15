use std::{
    ffi::OsString,
    io,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[cfg_attr(
    feature = "native-presentation",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Clone, Debug, PartialEq)]
pub struct Description {
    pub program: String,
    pub arguments: Vec<String>,
    pub directory: Option<PathBuf>,
}

impl Description {
    pub fn valid(&self) -> bool {
        !self.program.is_empty()
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
        command.args(&self.arguments);
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        command
            .env("WAYLAND_DISPLAY", socket)
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("SDL_VIDEODRIVER", "wayland")
            .env("SDL_VIDEO_DRIVER", "wayland")
            .env("XDG_SESSION_TYPE", "wayland")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, ffi::OsStr};

    #[test]
    fn launch_uses_park_display_and_removes_inherited_display_routes() {
        let command = Application::new("native-client")
            .unwrap()
            .command(Path::new("/run/user/1000/park-0"))
            .unwrap();
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        for key in ["DISPLAY", "XAUTHORITY", "WAYLAND_SOCKET"] {
            assert_eq!(environment.get(OsStr::new(key)), Some(&None));
        }
        assert_eq!(
            environment.get(OsStr::new("WAYLAND_DISPLAY")),
            Some(&Some(OsStr::new("/run/user/1000/park-0")))
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
