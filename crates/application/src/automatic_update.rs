use injection::cross_cutting::{AppNotification, InjectedServices};
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use std::{
    env,
    io::{Error, ErrorKind},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(not(windows))]
use {
    std::{fs, path::Path},
    tokio::process::Command,
};

const REPOSITORY: &str = "lince-social/lince";
const ROLLING_TAG: &str = "rolling";
const NOTIFICATION_ID: &str = "automatic-update-available";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub assets: Vec<UpdateAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct AvailableUpdate {
    pub manifest: UpdateManifest,
    pub current_version: String,
    pub current_revision: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check_startup_update(services: InjectedServices, allow_install: bool) {
    match check_for_update(services.clone()).await {
        Ok(Some(update)) => {
            let configuration = match services.repository.configuration.get_active().await {
                Ok(configuration) => configuration,
                Err(error) => {
                    eprintln!("Failed to read automatic update configuration: {error}");
                    return;
                }
            };

            if configuration.automatic_update_install_enabled == 1
                && allow_install
                && current_binary_supports_server_update()
                && !cfg!(debug_assertions)
            {
                if let Err(error) = install_update(&update).await {
                    eprintln!("Automatic update install failed: {error}");
                    if configuration.automatic_update_notify_enabled == 1 {
                        notify_update_available(&services, &update, false);
                    }
                }
                return;
            }

            if configuration.automatic_update_notify_enabled == 1 {
                notify_update_available(
                    &services,
                    &update,
                    allow_install
                        && current_binary_supports_server_update()
                        && !cfg!(debug_assertions),
                );
            }
        }
        Ok(None) => {
            services.notifications.dismiss(NOTIFICATION_ID);
        }
        Err(error) => eprintln!("Automatic update check failed: {error}"),
    }
}

pub async fn check_for_update(
    services: InjectedServices,
) -> Result<Option<AvailableUpdate>, Error> {
    let configuration = services.repository.configuration.get_active().await?;
    let channel = normalize_channel(&configuration.automatic_update_channel);
    let manifest = fetch_manifest(&channel).await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let current_revision = option_env!("LINCE_GIT_REVISION").unwrap_or("").to_string();

    if !is_newer_update(&manifest, &current_version, &current_revision, &channel) {
        return Ok(None);
    }

    Ok(Some(AvailableUpdate {
        manifest,
        current_version,
        current_revision,
    }))
}

pub async fn install_and_restart_later(
    services: InjectedServices,
    delay: Duration,
) -> Result<(), Error> {
    if cfg!(debug_assertions) {
        return Err(Error::other(
            "automatic install is disabled for debug builds",
        ));
    }

    let update = check_for_update(services.clone())
        .await?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "No automatic update is available"))?;
    install_update(&update).await?;
    services.notifications.dismiss(NOTIFICATION_ID);

    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if let Err(error) = restart_current_process() {
            eprintln!("Failed to restart after automatic update: {error}");
            return;
        }
        std::process::exit(0);
    });

    Ok(())
}

fn notify_update_available(
    services: &InjectedServices,
    update: &AvailableUpdate,
    installable: bool,
) {
    let revision = update.manifest.revision.trim();
    let available = if revision.is_empty() {
        update.manifest.version.clone()
    } else {
        format!("{} ({})", update.manifest.version, short_revision(revision))
    };
    let current = if update.current_revision.trim().is_empty() {
        update.current_version.clone()
    } else {
        format!(
            "{} ({})",
            update.current_version,
            short_revision(&update.current_revision)
        )
    };

    services.notifications.upsert(AppNotification {
        id: NOTIFICATION_ID.into(),
        kind: if installable {
            "app_update_installable".into()
        } else {
            "app_update_available".into()
        },
        severity: "info".into(),
        title: "Automatic update available".into(),
        body: format!("Lince {available} is available. Current version: {current}."),
        organ_id: None,
        created_at_unix: current_unix_timestamp_i64(),
    });
}

async fn fetch_manifest(channel: &str) -> Result<UpdateManifest, Error> {
    let client = Client::builder()
        .user_agent("lince-automatic-update")
        .build()
        .map_err(Error::other)?;

    let release = if channel == ROLLING_TAG {
        fetch_release_by_tag(&client, ROLLING_TAG).await?
    } else {
        fetch_latest_tag_release(&client).await?
    };

    let manifest_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "lince-update.json")
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "release has no lince-update.json"))?;

    client
        .get(&manifest_asset.browser_download_url)
        .send()
        .await
        .map_err(Error::other)?
        .error_for_status()
        .map_err(Error::other)?
        .json::<UpdateManifest>()
        .await
        .map_err(Error::other)
}

async fn fetch_release_by_tag(client: &Client, tag: &str) -> Result<GithubRelease, Error> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/tags/{tag}");
    client
        .get(url)
        .send()
        .await
        .map_err(Error::other)?
        .error_for_status()
        .map_err(Error::other)?
        .json::<GithubRelease>()
        .await
        .map_err(Error::other)
}

async fn fetch_latest_tag_release(client: &Client) -> Result<GithubRelease, Error> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases");
    let releases = client
        .get(url)
        .send()
        .await
        .map_err(Error::other)?
        .error_for_status()
        .map_err(Error::other)?
        .json::<Vec<GithubRelease>>()
        .await
        .map_err(Error::other)?;

    releases
        .into_iter()
        .filter(|release| {
            !release.draft && !release.prerelease && release.tag_name.starts_with('v')
        })
        .max_by(|left, right| {
            parse_release_version(&left.tag_name).cmp(&parse_release_version(&right.tag_name))
        })
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "no tagged Lince release found"))
}

fn is_newer_update(
    manifest: &UpdateManifest,
    current_version: &str,
    current_revision: &str,
    channel: &str,
) -> bool {
    if channel == ROLLING_TAG {
        let next_revision = manifest.revision.trim();
        return !next_revision.is_empty() && next_revision != current_revision.trim();
    }

    let Some(next_version) = parse_version(&manifest.version) else {
        return false;
    };
    let Some(current_version) = parse_version(current_version) else {
        return false;
    };
    next_version > current_version
}

async fn install_update(update: &AvailableUpdate) -> Result<(), Error> {
    if !current_binary_supports_server_update() {
        return Err(Error::other(
            "automatic install is only supported by the lince server binary",
        ));
    }

    let asset = select_binary_asset(&update.manifest)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "no matching automatic update asset"))?;

    #[cfg(windows)]
    {
        let _ = asset;
        return Err(Error::other(
            "automatic executable replacement is not supported on Windows yet",
        ));
    }

    #[cfg(not(windows))]
    install_unix_binary_asset(asset).await
}

#[cfg(not(windows))]
async fn install_unix_binary_asset(asset: &UpdateAsset) -> Result<(), Error> {
    let temp_dir = env::temp_dir().join(format!(
        "lince-automatic-update-{}",
        current_unix_timestamp_i64()
    ));
    fs::create_dir_all(&temp_dir)?;
    let archive_path = temp_dir.join(&asset.name);
    download_to_path(&asset.url, &archive_path).await?;
    verify_checksum_if_available(asset, &archive_path).await?;

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&temp_dir)
        .status()
        .await
        .map_err(Error::other)?;
    if !status.success() {
        return Err(Error::other("failed to unpack automatic update archive"));
    }

    let binary_path = temp_dir.join(format!("lince-{}", target_triple()));
    if !binary_path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            "automatic update archive did not contain the expected binary",
        ));
    }

    let current_exe = env::current_exe()?;
    let staged_exe = current_exe.with_extension("lince-update-new");
    fs::copy(&binary_path, &staged_exe)?;
    set_executable(&staged_exe)?;
    fs::rename(&staged_exe, &current_exe)?;
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}

#[cfg(not(windows))]
async fn download_to_path(url: &str, destination: &Path) -> Result<(), Error> {
    let bytes = Client::builder()
        .user_agent("lince-automatic-update")
        .build()
        .map_err(Error::other)?
        .get(url)
        .send()
        .await
        .map_err(Error::other)?
        .error_for_status()
        .map_err(Error::other)?
        .bytes()
        .await
        .map_err(Error::other)?;
    tokio::fs::write(destination, bytes).await
}

#[cfg(not(windows))]
async fn verify_checksum_if_available(
    asset: &UpdateAsset,
    archive_path: &Path,
) -> Result<(), Error> {
    let expected = asset.sha256.trim();
    if expected.is_empty() {
        return Ok(());
    }

    let output = Command::new("sha256sum")
        .arg(archive_path)
        .output()
        .await
        .map_err(Error::other)?;
    if !output.status.success() {
        return Err(Error::other("sha256sum failed for automatic update asset"));
    }
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    if actual != expected {
        return Err(Error::other(
            "automatic update checksum verification failed",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

fn restart_current_process() -> Result<(), Error> {
    let current_exe = env::current_exe()?;
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    std::process::Command::new(current_exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(Error::other)
}

fn select_binary_asset(manifest: &UpdateManifest) -> Option<&UpdateAsset> {
    let triple = target_triple();
    manifest.assets.iter().find(|asset| {
        let target_matches = asset.target == triple || asset.name.contains(triple);
        let kind_matches = asset.kind.is_empty() || asset.kind == "server";
        target_matches && kind_matches && asset.name.ends_with(".tar.gz")
    })
}

fn current_binary_supports_server_update() -> bool {
    env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|value| value.to_string_lossy().into_owned())
        })
        .is_some_and(|name| name == "lince")
}

fn normalize_channel(channel: &str) -> String {
    let channel = channel.trim();
    if channel.is_empty() {
        ROLLING_TAG.into()
    } else {
        channel.to_ascii_lowercase()
    }
}

fn parse_release_version(tag: &str) -> Option<Version> {
    parse_version(tag.trim_start_matches('v'))
}

fn parse_version(version: &str) -> Option<Version> {
    Version::parse(version.trim().trim_start_matches('v')).ok()
}

fn target_triple() -> &'static str {
    option_env!("LINCE_TARGET").unwrap_or(default_target_triple())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn default_target_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn default_target_triple() -> &'static str {
    "aarch64-unknown-linux-gnu"
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn default_target_triple() -> &'static str {
    "aarch64-apple-darwin"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn default_target_triple() -> &'static str {
    "x86_64-apple-darwin"
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn default_target_triple() -> &'static str {
    "x86_64-pc-windows-msvc"
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
fn default_target_triple() -> &'static str {
    "unknown"
}

fn short_revision(revision: &str) -> String {
    revision.chars().take(12).collect()
}

fn current_unix_timestamp_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
