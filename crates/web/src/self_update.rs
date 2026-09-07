use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/lince-social/lince/releases?per_page=100";
const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/lince-social/lince/releases/download";
const POLL_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(60 * 60);
const USER_AGENT: &str = concat!("lince/", env!("CARGO_PKG_VERSION"));

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub available: bool,
    pub version: String,
    pub revision: String,
    pub channel: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_sha256: Option<String>,
    pub can_self_apply: bool,
}

pub type SharedUpdateStatus = Arc<RwLock<Option<UpdateStatus>>>;

impl UpdateManifest {
    pub fn asset_for(&self, target: &str) -> Option<&ManifestAsset> {
        if target.is_empty() {
            return None;
        }
        let matches = |asset: &&ManifestAsset| {
            asset.target == target
                || (!asset.target.is_empty() && asset.target.contains(target))
                || asset.name.contains(target)
        };
        self.assets
            .iter()
            .find(|asset| matches(asset) && asset.kind == "desktop")
            .or_else(|| self.assets.iter().find(matches))
    }
}

pub fn latest_rolling_tag(releases_json: &str) -> Option<String> {
    let releases: Vec<serde_json::Value> = serde_json::from_str(releases_json).ok()?;
    releases
        .iter()
        .filter_map(|release| release.get("tag_name").and_then(serde_json::Value::as_str))
        .find(|tag| tag.starts_with("rolling-"))
        .map(str::to_string)
}

pub fn evaluate(
    manifest: &UpdateManifest,
    current_revision: &str,
    target: Option<&str>,
) -> UpdateStatus {
    let asset = target.and_then(|value| manifest.asset_for(value));
    let available = !manifest.revision.is_empty()
        && current_revision != "unknown"
        && manifest.revision != current_revision;
    UpdateStatus {
        available,
        version: manifest.version.clone(),
        revision: manifest.revision.clone(),
        channel: manifest.channel.clone(),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_url: asset.map(|asset| asset.url.clone()),
        asset_sha256: asset
            .filter(|asset| !asset.sha256.is_empty())
            .map(|asset| asset.sha256.clone()),
        can_self_apply: available
            && asset.is_some()
            && utils::build_info::current_exe_replaceable(),
    }
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

fn checks_disabled() -> bool {
    matches!(
        std::env::var("LINCE_UPDATE_CHECK").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    )
}

fn releases_api() -> String {
    std::env::var("LINCE_UPDATE_RELEASES_URL").unwrap_or_else(|_| DEFAULT_RELEASES_API.to_string())
}

fn download_base() -> String {
    std::env::var("LINCE_UPDATE_DOWNLOAD_BASE").unwrap_or_else(|_| DEFAULT_DOWNLOAD_BASE.to_string())
}

pub fn spawn_poller(shared: SharedUpdateStatus) {
    if checks_disabled() {
        return;
    }
    tokio::spawn(async move {
        loop {
            let wait = match check_once().await {
                Some(status) => {
                    *shared.write().await = Some(status);
                    POLL_INTERVAL
                }
                None => RETRY_INTERVAL,
            };
            tokio::time::sleep(wait).await;
        }
    });
}

async fn check_once() -> Option<UpdateStatus> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .ok()?;
    let releases_json = client
        .get(releases_api())
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    let tag = latest_rolling_tag(&releases_json)?;
    let manifest_url = format!("{}/{tag}/lince-update.json", download_base().trim_end_matches('/'));
    let manifest: UpdateManifest = client
        .get(manifest_url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    Some(evaluate(
        &manifest,
        utils::build_info::revision(),
        utils::build_info::target_triple(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "version": "0.7.0",
        "revision": "abcdef1234567890",
        "channel": "rolling",
        "assets": [
            {"name":"Lince_0.7.0_amd64.AppImage","url":"https://example/app.AppImage","sha256":"aa","target":"x86_64-unknown-linux-gnu","kind":"desktop"},
            {"name":"Lince_0.7.0_x64-setup.exe","url":"https://example/setup.exe","sha256":"bb","target":"x86_64-pc-windows-msvc","kind":"desktop"}
        ]
    }"#;

    fn manifest() -> UpdateManifest {
        serde_json::from_str(MANIFEST).unwrap()
    }

    #[test]
    fn picks_the_asset_for_the_running_target() {
        let manifest = manifest();
        let asset = manifest.asset_for("x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(asset.name, "Lince_0.7.0_amd64.AppImage");
    }

    #[test]
    fn unknown_target_yields_no_asset() {
        let manifest = manifest();
        assert!(manifest.asset_for("").is_none());
        assert!(manifest.asset_for("sparc-unknown-none").is_none());
    }

    #[test]
    fn available_when_revision_differs_and_current_is_known() {
        let status = evaluate(&manifest(), "0000000000000000", Some("x86_64-unknown-linux-gnu"));
        assert!(status.available);
        assert_eq!(status.asset_url.as_deref(), Some("https://example/app.AppImage"));
    }

    #[test]
    fn not_available_for_the_same_revision() {
        let status = evaluate(&manifest(), "abcdef1234567890", Some("x86_64-unknown-linux-gnu"));
        assert!(!status.available);
    }

    #[test]
    fn not_available_when_current_revision_is_unknown() {
        let status = evaluate(&manifest(), "unknown", Some("x86_64-unknown-linux-gnu"));
        assert!(!status.available);
    }

    #[test]
    fn latest_rolling_tag_takes_the_first_rolling_entry() {
        let json = r#"[
            {"tag_name":"v0.7.0"},
            {"tag_name":"rolling-41"},
            {"tag_name":"rolling-40"},
            {"tag_name":"rolling-20260614"}
        ]"#;
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
}
