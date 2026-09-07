use std::sync::mpsc::{Receiver, TryRecvError};

use utils::build_info;
use utils::self_update::{Availability, UpdateStatus};

pub enum Phase {
    Idle,
    Checking,
    Checked(Box<UpdateStatus>),
    CheckFailed(String),
    Installing,
    InstallFailed(String),
}

#[cfg_attr(not(feature = "native-runtime"), allow(dead_code))]
enum Msg {
    Checked(Box<UpdateStatus>),
    CheckFailed(String),
    InstallReady,
    InstallFailed(String),
}

pub struct UpdatePanel {
    phase: Phase,
    receiver: Option<Receiver<Msg>>,
}

impl Default for UpdatePanel {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            receiver: None,
        }
    }
}

impl UpdatePanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Installing)
    }

    pub fn updates_line(&self) -> String {
        match &self.phase {
            Phase::Idle => "Not checked. Press Check for updates.".into(),
            Phase::Checking => "Checking the release feed…".into(),
            Phase::CheckFailed(reason) => format!("Check failed: {reason}"),
            Phase::Installing => "Downloading and verifying…".into(),
            Phase::InstallFailed(reason) => format!("Install failed: {reason}"),
            Phase::Checked(status) => match status.availability {
                Availability::Unstamped => {
                    "This build carries no revision stamp, so updates cannot be compared.".into()
                }
                Availability::UpToDate => "Up to date.".into(),
                Availability::Available => {
                    let name = status
                        .revision
                        .get(..12)
                        .unwrap_or(status.revision.as_str());
                    format!("Update available: {} ({name}).", status.version)
                }
            },
        }
    }

    pub fn self_apply_line(&self) -> String {
        match &self.phase {
            Phase::Checked(status) if status.availability == Availability::Available => {
                if status.can_self_apply {
                    "Ready to download and restart.".into()
                } else {
                    status.self_apply_note.clone()
                }
            }
            _ => build_info::self_apply_block().message().to_string(),
        }
    }

    pub fn start_check(&mut self) {
        if self.busy() {
            return;
        }
        #[cfg(not(feature = "native-runtime"))]
        {
            self.phase = Phase::CheckFailed("this build has no updater".into());
        }
        #[cfg(feature = "native-runtime")]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            self.receiver = Some(receiver);
            self.phase = Phase::Checking;
            std::thread::spawn(move || {
                let _ = sender.send(net::run_check());
            });
        }
    }

    pub fn start_install(&mut self) {
        if self.busy() {
            return;
        }
        let Phase::Checked(status) = &self.phase else {
            return;
        };
        if !status.can_self_apply {
            self.phase = Phase::InstallFailed(status.self_apply_note.clone());
            return;
        }
        let (Some(url), Some(sha256)) = (status.asset_url.clone(), status.asset_sha256.clone())
        else {
            self.phase =
                Phase::InstallFailed("the release has no verified asset for this system".into());
            return;
        };
        #[cfg(not(feature = "native-runtime"))]
        {
            let _ = (url, sha256);
            self.phase = Phase::InstallFailed("this build has no updater".into());
        }
        #[cfg(feature = "native-runtime")]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            self.receiver = Some(receiver);
            self.phase = Phase::Installing;
            std::thread::spawn(move || {
                let _ = sender.send(net::run_install(&url, &sha256));
            });
        }
    }

    pub fn pump(&mut self) -> bool {
        let Some(receiver) = &self.receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(Msg::Checked(status)) => {
                self.phase = Phase::Checked(status);
                self.receiver = None;
                true
            }
            Ok(Msg::CheckFailed(reason)) => {
                self.phase = Phase::CheckFailed(reason);
                self.receiver = None;
                true
            }
            Ok(Msg::InstallFailed(reason)) => {
                self.phase = Phase::InstallFailed(reason);
                self.receiver = None;
                true
            }
            Ok(Msg::InstallReady) => {
                self.receiver = None;
                restart_into_new_binary();
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                false
            }
        }
    }
}

fn restart_into_new_binary() {
    let program = build_info::appimage_path()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("lince-desktop"));
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let _ = std::process::Command::new(program).args(arguments).spawn();
    std::process::exit(0);
}

#[cfg(feature = "native-runtime")]
mod net {
    use std::time::Duration;

    use utils::build_info;
    use utils::self_update::{self, UpdateError, UpdateStatus};

    use super::Msg;

    const USER_AGENT: &str = concat!("lince/", env!("CARGO_PKG_VERSION"));

    fn blocking_runtime() -> Result<tokio::runtime::Runtime, String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    }

    pub fn run_check() -> Msg {
        let runtime = match blocking_runtime() {
            Ok(runtime) => runtime,
            Err(error) => return Msg::CheckFailed(error),
        };
        match runtime.block_on(fetch_status()) {
            Ok(status) => Msg::Checked(Box::new(status)),
            Err(error) => Msg::CheckFailed(error),
        }
    }

    async fn fetch_status() -> Result<UpdateStatus, String> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| error.to_string())?;
        let releases = client
            .get(self_update::releases_api())
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .text()
            .await
            .map_err(|error| error.to_string())?;
        let tag = self_update::latest_rolling_tag(&releases)
            .ok_or_else(|| "no rolling release is published yet".to_string())?;
        let manifest_bytes = client
            .get(self_update::manifest_url(&tag))
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        let signature = match client.get(self_update::signature_url(&tag)).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => Some(response.text().await.map_err(|error| error.to_string())?),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let manifest = self_update::parse_verified_manifest(&manifest_bytes, signature.as_deref())
            .map_err(|error: UpdateError| error.to_string())?;
        Ok(self_update::evaluate(
            &manifest,
            build_info::revision(),
            build_info::target_triple(),
            build_info::self_apply_block(),
        ))
    }

    pub fn run_install(url: &str, sha256: &str) -> Msg {
        let runtime = match blocking_runtime() {
            Ok(runtime) => runtime,
            Err(error) => return Msg::InstallFailed(error),
        };
        let bytes = match runtime.block_on(download(url)) {
            Ok(bytes) => bytes,
            Err(error) => return Msg::InstallFailed(error),
        };
        if !self_update::verify_sha256(&bytes, sha256) {
            return Msg::InstallFailed("the downloaded file failed its checksum".into());
        }
        match swap_appimage(&bytes) {
            Ok(()) => Msg::InstallReady,
            Err(error) => Msg::InstallFailed(error),
        }
    }

    async fn download(url: &str) -> Result<Vec<u8>, String> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|error| error.to_string())?;
        let bytes = client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        Ok(bytes.to_vec())
    }

    fn swap_appimage(bytes: &[u8]) -> Result<(), String> {
        let target = build_info::appimage_path()
            .ok_or_else(|| "this build is not running as an AppImage".to_string())?;
        let staged = target.with_extension("lince-update-new");
        std::fs::write(&staged, bytes).map_err(|error| error.to_string())?;
        set_executable(&staged)?;
        std::fs::rename(&staged, &target).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    fn set_executable(path: &std::path::Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())
    }

    #[cfg(not(unix))]
    fn set_executable(_path: &std::path::Path) -> Result<(), String> {
        Ok(())
    }
}
