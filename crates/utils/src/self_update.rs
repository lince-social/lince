use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::build_info::SelfApplyBlock;

const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/lince-social/lince/releases?per_page=100";
const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/lince-social/lince/releases/download";

pub const MANIFEST_NAME: &str = "lince-update.json";
pub const SIGNATURE_NAME: &str = "lince-update.json.sig";

pub const UPDATE_SIGNING_PUBLIC_KEY_HEX: &str =
    "4810bed0038acf62810ecff3ed28fcc8b2b924563cd6471ee7ea77cbe0006d3f";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Desktop,
    Server,
}

#[derive(Debug, Clone, Copy)]
pub enum UpdateCommand {
    Check,
    DownloadAndRestart,
    Automatic(bool),
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateManifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub assets: Vec<ManifestAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAsset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Unstamped,
    UpToDate,
    Available,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub availability: Availability,
    pub version: String,
    pub revision: String,
    pub channel: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_sha256: Option<String>,
    pub can_self_apply: bool,
    pub self_apply_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Unsigned,
    BadSignature,
    MalformedManifest,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            UpdateError::Unsigned => "the update manifest has no signature",
            UpdateError::BadSignature => "the update manifest signature does not verify",
            UpdateError::MalformedManifest => "the update manifest could not be parsed",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for UpdateError {}

impl UpdateManifest {
    pub fn asset_for(&self, target: &str) -> Option<&ManifestAsset> {
        self.asset_for_kind(target, AssetKind::Desktop)
    }

    pub fn asset_for_kind(&self, target: &str, kind: AssetKind) -> Option<&ManifestAsset> {
        self.assets
            .iter()
            .filter(|asset| !target.is_empty() && asset.target == target)
            .filter(|asset| {
                asset.kind
                    == match kind {
                        AssetKind::Desktop => "desktop",
                        AssetKind::Server => "server",
                    }
            })
            .min_by_key(|asset| !asset.name.ends_with(".AppImage"))
    }
}

pub fn releases_api() -> String {
    std::env::var("LINCE_UPDATE_RELEASES_URL").unwrap_or_else(|_| DEFAULT_RELEASES_API.to_string())
}

pub fn download_base() -> String {
    std::env::var("LINCE_UPDATE_DOWNLOAD_BASE")
        .unwrap_or_else(|_| DEFAULT_DOWNLOAD_BASE.to_string())
}

pub fn manifest_url(tag: &str) -> String {
    format!(
        "{}/{tag}/{MANIFEST_NAME}",
        download_base().trim_end_matches('/')
    )
}

pub fn signature_url(tag: &str) -> String {
    format!(
        "{}/{tag}/{SIGNATURE_NAME}",
        download_base().trim_end_matches('/')
    )
}

pub fn checks_disabled() -> bool {
    matches!(
        std::env::var("LINCE_UPDATE_CHECK").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    )
}

pub fn latest_rolling_tag(releases_json: &str) -> Option<String> {
    let releases: Vec<serde_json::Value> = serde_json::from_str(releases_json).ok()?;
    releases
        .iter()
        .filter_map(|release| release.get("tag_name").and_then(serde_json::Value::as_str))
        .find(|tag| tag.starts_with("rolling-"))
        .map(str::to_string)
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.is_ascii() || text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    let expected = expected_hex.trim().to_ascii_lowercase();
    !expected.is_empty() && sha256_hex(bytes) == expected
}

pub fn signing_key_configured() -> bool {
    UPDATE_SIGNING_PUBLIC_KEY_HEX
        .bytes()
        .any(|byte| byte != b'0')
}

pub fn verify_manifest_signature(manifest_bytes: &[u8], signature_hex: &str) -> bool {
    let Some(key_bytes) = decode_hex(UPDATE_SIGNING_PUBLIC_KEY_HEX) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Some(signature_bytes) = decode_hex(signature_hex) else {
        return false;
    };
    let Ok(signature_array): Result<[u8; 64], _> = signature_bytes.try_into() else {
        return false;
    };
    let signature = Signature::from_bytes(&signature_array);
    verifying_key
        .verify_strict(manifest_bytes, &signature)
        .is_ok()
}

pub fn parse_verified_manifest(
    manifest_bytes: &[u8],
    signature_hex: Option<&str>,
) -> Result<UpdateManifest, UpdateError> {
    let signature_hex = signature_hex
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(UpdateError::Unsigned)?;
    if !verify_manifest_signature(manifest_bytes, signature_hex) {
        return Err(UpdateError::BadSignature);
    }
    let manifest: UpdateManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| UpdateError::MalformedManifest)?;
    if manifest.version.trim().is_empty() || manifest.revision.trim().is_empty() {
        return Err(UpdateError::MalformedManifest);
    }
    Ok(manifest)
}

pub fn evaluate(
    manifest: &UpdateManifest,
    current_revision: &str,
    target: Option<&str>,
    block: SelfApplyBlock,
) -> UpdateStatus {
    evaluate_kind(
        manifest,
        current_revision,
        target,
        block,
        AssetKind::Desktop,
    )
}

pub fn evaluate_kind(
    manifest: &UpdateManifest,
    current_revision: &str,
    target: Option<&str>,
    block: SelfApplyBlock,
    kind: AssetKind,
) -> UpdateStatus {
    let asset = target.and_then(|value| manifest.asset_for_kind(value, kind));
    let availability = if current_revision.is_empty() || current_revision == "unknown" {
        Availability::Unstamped
    } else if !manifest.revision.is_empty() && manifest.revision != current_revision {
        Availability::Available
    } else {
        Availability::UpToDate
    };
    let note = if block != SelfApplyBlock::None {
        block.message().to_string()
    } else if asset.is_none() {
        "No published build matches this system and mode. Visit https://github.com/lince-social/lince/releases".into()
    } else if !asset.is_some_and(replaceable_asset) {
        "This download needs an installer or unpacking. Visit https://github.com/lince-social/lince/releases".into()
    } else {
        block.message().to_string()
    };
    let can_self_apply = availability == Availability::Available
        && block == SelfApplyBlock::None
        && asset.is_some_and(replaceable_asset);
    UpdateStatus {
        availability,
        version: manifest.version.clone(),
        revision: manifest.revision.clone(),
        channel: manifest.channel.clone(),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_url: asset.map(|asset| asset.url.clone()),
        asset_sha256: asset
            .filter(|asset| !asset.sha256.is_empty())
            .map(|asset| asset.sha256.clone()),
        can_self_apply,
        self_apply_note: note,
    }
}

fn replaceable_asset(asset: &ManifestAsset) -> bool {
    let raw_server = asset.kind == "server"
        && ![".zip", ".gz", ".xz", ".dmg", ".deb", ".msi", ".exe"]
            .iter()
            .any(|suffix| asset.name.ends_with(suffix));
    (asset.name.ends_with(".AppImage") || raw_server)
        && asset.sha256.len() == 64
        && asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        && asset.url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const MANIFEST: &[u8] = br#"{"version":"0.7.0","revision":"abcdef1234567890","channel":"rolling","assets":[{"name":"Lince_0.7.0_amd64.AppImage","url":"https://example/app.AppImage","sha256":"aa","target":"x86_64-unknown-linux-gnu","kind":"desktop"},{"name":"Lince_0.7.0_x64-setup.exe","url":"https://example/setup.exe","sha256":"bb","target":"x86_64-pc-windows-msvc","kind":"desktop"}]}"#;

    fn manifest() -> UpdateManifest {
        let mut manifest: UpdateManifest = serde_json::from_slice(MANIFEST).unwrap();
        for asset in &mut manifest.assets {
            asset.sha256 = "ab".repeat(32);
        }
        manifest
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn picks_the_asset_for_the_running_target() {
        let manifest = manifest();
        assert_eq!(
            manifest.asset_for("x86_64-unknown-linux-gnu").unwrap().name,
            "Lince_0.7.0_amd64.AppImage"
        );
        assert!(manifest.asset_for("").is_none());
        assert!(manifest.asset_for("sparc-unknown-none").is_none());
    }

    #[test]
    fn replacement_rejects_installers_missing_checksums_and_wrong_modes() {
        let mut manifest = manifest();
        let target = "x86_64-unknown-linux-gnu";
        let mut package = manifest.assets[0].clone();
        package.name = "lince.deb".into();
        manifest.assets.insert(0, package);
        assert!(
            manifest
                .asset_for(target)
                .unwrap()
                .name
                .ends_with(".AppImage")
        );
        assert!(manifest.asset_for_kind(target, AssetKind::Server).is_none());
        manifest
            .assets
            .retain(|asset| !asset.name.ends_with(".AppImage"));
        assert!(!evaluate(&manifest, "old", Some(target), SelfApplyBlock::None).can_self_apply);
        manifest.assets[0].kind = "server".into();
        manifest.assets[0].name = "lince-server".into();
        assert!(
            evaluate_kind(
                &manifest,
                "old",
                Some(target),
                SelfApplyBlock::None,
                AssetKind::Server
            )
            .can_self_apply
        );
        manifest.assets[0].sha256.clear();
        assert!(
            !evaluate_kind(
                &manifest,
                "old",
                Some(target),
                SelfApplyBlock::None,
                AssetKind::Server
            )
            .can_self_apply
        );
        assert!(!verify_manifest_signature(MANIFEST, "aéa"));
    }

    #[test]
    fn availability_has_three_states() {
        assert_eq!(
            evaluate(
                &manifest(),
                "unknown",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::Unstamped
        );
        assert_eq!(
            evaluate(
                &manifest(),
                "abcdef1234567890",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::UpToDate
        );
        assert_eq!(
            evaluate(
                &manifest(),
                "0000000000000000",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::Available
        );
    }

    #[test]
    fn can_self_apply_needs_available_asset_and_no_block() {
        let ok = evaluate(
            &manifest(),
            "0000000000000000",
            Some("x86_64-unknown-linux-gnu"),
            SelfApplyBlock::None,
        );
        assert!(ok.can_self_apply);
        let nix = evaluate(
            &manifest(),
            "0000000000000000",
            Some("x86_64-unknown-linux-gnu"),
            SelfApplyBlock::NixStore,
        );
        assert!(!nix.can_self_apply);
        assert!(nix.self_apply_note.contains("nixos-rebuild"));
    }

    #[test]
    fn latest_rolling_tag_takes_the_first_rolling_entry() {
        let json = r#"[{"tag_name":"v0.7.0"},{"tag_name":"rolling-41"},{"tag_name":"rolling-40"}]"#;
        assert_eq!(latest_rolling_tag(json).as_deref(), Some("rolling-41"));
    }

    #[test]
    fn sha256_round_trips() {
        let digest = sha256_hex(b"lince");
        assert!(verify_sha256(b"lince", &digest));
        assert!(verify_sha256(b"lince", &digest.to_uppercase()));
        assert!(!verify_sha256(b"lynx", &digest));
        assert!(!verify_sha256(b"lince", ""));
    }

    #[test]
    fn signature_verifies_only_for_the_matching_key_and_bytes() {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let good_signature = hex(&signing_key.sign(MANIFEST).to_bytes());
        let public_key_hex = hex(signing_key.verifying_key().as_bytes());

        assert!(super::verify_with_key(
            &public_key_hex,
            MANIFEST,
            &good_signature
        ));
        assert!(!super::verify_with_key(
            &public_key_hex,
            b"tampered",
            &good_signature
        ));
        let other_key = SigningKey::from_bytes(&[9u8; 32]);
        let wrong_signature = hex(&other_key.sign(MANIFEST).to_bytes());
        assert!(!super::verify_with_key(
            &public_key_hex,
            MANIFEST,
            &wrong_signature
        ));
    }

    #[test]
    fn parse_verified_manifest_rejects_missing_and_bad_signatures() {
        assert_eq!(
            parse_verified_manifest(MANIFEST, None).unwrap_err(),
            UpdateError::Unsigned
        );
        assert_eq!(
            parse_verified_manifest(MANIFEST, Some("   ")).unwrap_err(),
            UpdateError::Unsigned
        );
        assert_eq!(
            parse_verified_manifest(MANIFEST, Some(&"ab".repeat(64))).unwrap_err(),
            UpdateError::BadSignature
        );
    }
}

#[cfg(test)]
fn verify_with_key(public_key_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    let Some(key_bytes) = decode_hex(public_key_hex) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Some(signature_bytes) = decode_hex(signature_hex) else {
        return false;
    };
    let Ok(signature_array): Result<[u8; 64], _> = signature_bytes.try_into() else {
        return false;
    };
    verifying_key
        .verify_strict(message, &Signature::from_bytes(&signature_array))
        .is_ok()
}

#[cfg(feature = "update-net")]
pub mod net {
    use std::time::Duration;

    use super::{
        AssetKind, Availability, UpdateCommand, UpdateError, UpdateStatus, evaluate_kind,
        latest_rolling_tag, manifest_url, parse_verified_manifest, signature_url, verify_sha256,
    };
    use crate::build_info;

    const USER_AGENT: &str = concat!("lince/", env!("CARGO_PKG_VERSION"));

    fn client(timeout: Duration) -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .map_err(|error| error.to_string())
    }

    pub async fn check(kind: AssetKind) -> Result<UpdateStatus, String> {
        let client = client(Duration::from_secs(20))?;
        let releases = client
            .get(super::releases_api())
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .text()
            .await
            .map_err(|error| error.to_string())?;
        let tag = latest_rolling_tag(&releases)
            .ok_or_else(|| "no rolling release is published yet".to_string())?;
        let manifest_bytes = client
            .get(manifest_url(&tag))
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;
        let signature = match client.get(signature_url(&tag)).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => Some(response.text().await.map_err(|error| error.to_string())?),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let manifest = parse_verified_manifest(&manifest_bytes, signature.as_deref())
            .map_err(|error: UpdateError| error.to_string())?;
        Ok(evaluate_kind(
            &manifest,
            build_info::revision(),
            build_info::target_triple(),
            build_info::self_apply_block_for(kind == AssetKind::Server),
            kind,
        ))
    }

    async fn download(url: &str) -> Result<Vec<u8>, String> {
        let bytes = client(Duration::from_secs(600))?
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

    pub async fn download_and_apply(status: &UpdateStatus) -> Result<(), String> {
        if !status.can_self_apply || status.availability != Availability::Available {
            return Err(status.self_apply_note.clone());
        }
        let url = status
            .asset_url
            .as_deref()
            .ok_or_else(|| "no release asset for this system".to_string())?;
        let sha256 = status
            .asset_sha256
            .as_deref()
            .ok_or_else(|| "the release asset has no checksum".to_string())?;
        let bytes = download(url).await?;
        if !verify_sha256(&bytes, sha256) {
            return Err("the downloaded file failed its checksum".into());
        }
        apply_bytes(&bytes)
    }

    fn apply_bytes(bytes: &[u8]) -> Result<(), String> {
        let target = build_info::appimage_path()
            .or_else(|| std::env::current_exe().ok())
            .ok_or_else(|| "cannot find the running program to replace".to_string())?;
        if target.starts_with("/nix/store") {
            return Err("running from the Nix store; update through the flake instead".into());
        }
        let staged = target.with_extension("lince-update-new");
        stage_and_replace(&target, &staged, bytes)
    }

    fn stage_and_replace(
        target: &std::path::Path,
        staged: &std::path::Path,
        bytes: &[u8],
    ) -> Result<(), String> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged)
            .map_err(|error| error.to_string())?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            set_executable(staged).map_err(std::io::Error::other)?;
            std::fs::rename(staged, target)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(staged);
        }
        result.map_err(|error| error.to_string())
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

    pub fn restart() -> std::io::Result<()> {
        let program = build_info::appimage_path()
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("lince"));
        restart_program(&program)
    }

    pub fn restart_program(program: &std::path::Path) -> std::io::Result<()> {
        let mut command = std::process::Command::new(program);
        command.args(std::env::args_os().skip(1));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            Err(command.exec())
        }
        #[cfg(not(unix))]
        {
            command.spawn()?;
            std::process::exit(0);
        }
    }

    pub trait UpdateWatcher: Send {
        fn deadline(&self) -> Option<tokio::time::Instant>;
        fn handle(
            &mut self,
            command: UpdateCommand,
        ) -> impl std::future::Future<Output = ()> + Send;
    }

    pub async fn run_watch_loop(
        mut watcher: impl UpdateWatcher,
        mut commands: tokio::sync::mpsc::Receiver<UpdateCommand>,
    ) {
        loop {
            let deadline = watcher.deadline();
            let command = tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => command,
                    None => return,
                },
                _ = async {
                    match deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                } => UpdateCommand::Check,
            };
            watcher.handle(command).await;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn replacement_is_atomic_and_leaves_the_original_when_staging_fails() {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("lince");
            let staged = directory.path().join("lince-update-new");
            std::fs::write(&target, b"original").unwrap();
            std::fs::write(&staged, b"occupied").unwrap();
            assert!(stage_and_replace(&target, &staged, b"new").is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"original");
            assert_eq!(std::fs::read(&staged).unwrap(), b"occupied");
            std::fs::remove_file(&staged).unwrap();
            stage_and_replace(&target, &staged, b"new").unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"new");
            assert!(!staged.exists());
        }

        #[cfg(unix)]
        #[test]
        fn staging_refuses_symlinks_without_touching_the_link_target() {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("lince");
            let staged = directory.path().join("lince-update-new");
            let other = directory.path().join("private");
            std::fs::write(&target, b"original").unwrap();
            std::fs::write(&other, b"private").unwrap();
            std::os::unix::fs::symlink(&other, &staged).unwrap();
            assert!(stage_and_replace(&target, &staged, b"new").is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"original");
            assert_eq!(std::fs::read(&other).unwrap(), b"private");
        }
    }
}
