// non-lean-yet
// Scope: const REGISTRY_URL, const MAX_REGISTRY, struct Registry, struct Agent.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 166, 167, 175, 177, 336, 345.
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   66, 155.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 213, 246, 249,
//   282, 283, 284.
// - [Clippy: clippy::collapsible_if] this `if` statement can be collapsed. Sites 336.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 32.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   67, 157.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   75, 106, 111, 115, 120.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 70, 101.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 11, 16,
//   17, 18, 22, 23, 24, 25, 27, 32, 33, 34, 35, 36, 37, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 54,
//   55, 56, 60, 61, 62, 66, 70, 101, 132, 155.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites parse: String
//   (70), refresh: String (101).

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::acp;

pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
const MAX_REGISTRY: usize = 2_097_152;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: String,
    pub agents: Vec<Agent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub distribution: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Installed,
    BridgeMissing,
    RuntimeMissing,
    RuntimeUnsupported,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub registry_version: String,
    pub availability: Availability,
    pub executable: Option<PathBuf>,
    pub native_executable: Option<PathBuf>,
    pub detail: String,
    pub installation: Option<String>,
    pub config: Option<acp::Config>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Discovery {
    pub candidates: Vec<Candidate>,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct Search {
    pub paths: Vec<PathBuf>,
    pub directory: PathBuf,
}

impl Registry {
    pub fn bundled() -> Self {
        Self::parse(include_bytes!("registry.json")).expect("validated bundled ACP registry")
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_REGISTRY {
            return Err("The ACP registry exceeds its size limit.".into());
        }
        let registry: Self =
            serde_json::from_slice(bytes).map_err(|_| "Invalid ACP registry.".to_string())?;
        let mut ids = BTreeSet::new();
        if registry.version != "1.0.0"
            || registry.agents.len() > 256
            || registry.agents.iter().any(|agent| {
                agent.id.is_empty()
                    || agent.id.len() > 80
                    || !agent
                        .id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                    || !ids.insert(&agent.id)
                    || agent.name.len() > 256
                    || agent.name.chars().any(char::is_control)
                    || agent.version.len() > 80
                    || agent.version.chars().any(char::is_control)
                    || nucleus::json::encoded_len(&agent.distribution)
                        .map_or(true, |bytes| bytes > 32_768)
                    || !valid_distribution(&agent.distribution)
            })
        {
            return Err("The ACP registry has invalid or oversized agent metadata.".into());
        }
        Ok(registry)
    }

    pub async fn refresh() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| "Cannot initialize registry discovery.".to_string())?;
        let mut response = client
            .get(REGISTRY_URL)
            .send()
            .await
            .map_err(|_| {
                "Cannot reach the ACP registry; the bundled registry remains available.".to_string()
            })?
            .error_for_status()
            .map_err(|_| "The ACP registry rejected discovery.".to_string())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "The registry response was interrupted.".to_string())?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_REGISTRY {
                return Err("The ACP registry exceeds its size limit.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Self::parse(&bytes)
    }
}

impl Search {
    pub fn current(directory: PathBuf) -> Self {
        let mut paths: Vec<_> = std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(&home).join(".local/bin"));
        }
        paths.retain(|path| path.is_absolute());
        paths.truncate(96);
        Self { paths, directory }
    }

    fn find(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return None;
        }
        self.paths
            .iter()
            .map(|directory| directory.join(name))
            .find(|path| executable(path))
            .and_then(|path| path.canonicalize().ok())
    }

    pub fn scan(&self, registry: &Registry) -> Discovery {
        let aliases: BTreeMap<String, Value> = serde_json::from_str(include_str!("launchers.json"))
            .expect("validated launcher metadata");
        let os = if std::env::consts::OS == "macos" {
            "darwin"
        } else {
            std::env::consts::OS
        };
        let platform = format!("{os}-{}", std::env::consts::ARCH);
        let mut candidates = Vec::new();
        for agent in &registry.agents {
            let binary = &agent.distribution["binary"][&platform];
            let package_route = if agent.distribution["npx"].is_object() {
                "npx"
            } else {
                "uvx"
            };
            let distribution = if binary.is_object() {
                binary
            } else {
                &agent.distribution[package_route]
            };
            let package = agent.distribution[package_route]["package"].as_str();
            let program = binary["cmd"]
                .as_str()
                .and_then(|cmd| cmd.rsplit(['/', '\\']).next())
                .or_else(|| {
                    package
                        .and_then(|package| package.rsplit('/').next())
                        .and_then(|name| name.split(['@', '=']).next())
                })
                .unwrap_or(&agent.id);
            let executable = self.find(program).or_else(|| self.find(&agent.id));
            let native = aliases
                .get(&agent.id)
                .and_then(|alias| alias["native"].as_str())
                .and_then(|name| self.find(name));
            let installation = package.and_then(|package| {
                if package_route == "npx" {
                    None
                } else {
                    Some(format!("uv tool install {package}"))
                }
            });
            let args = distribution["args"].as_array();
            let args: Vec<String> = args
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let mut environment: BTreeMap<String, String> = distribution["env"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| (name.clone(), value.to_string()))
                })
                .collect();
            if let Ok(path) = std::env::join_paths(&self.paths) {
                environment.insert("PATH".into(), path.to_string_lossy().into_owned());
            }
            if let (Some(native), Some(variable)) = (
                &native,
                aliases
                    .get(&agent.id)
                    .and_then(|alias| alias["native_variable"].as_str()),
            ) {
                environment.insert(variable.into(), native.to_string_lossy().into_owned());
            }
            let runtime = executable
                .as_ref()
                .and_then(|path| {
                    std::fs::File::open(path).ok().and_then(|file| {
                        let mut bytes = Vec::new();
                        file.take(256).read_to_end(&mut bytes).ok().map(|_| bytes)
                    })
                })
                .and_then(|bytes| {
                    let first =
                        std::str::from_utf8(bytes.split(|byte| *byte == b'\n').next()?).ok()?;
                    first
                        .strip_prefix("#!/usr/bin/env ")
                        .map(str::trim)
                        .map(str::to_owned)
                });
            let runtime_missing = runtime
                .as_deref()
                .is_some_and(|runtime| self.find(runtime).is_none());
            let candidate_config = executable.clone().map(|command| acp::Config {
                command,
                args,
                directory: self.directory.clone(),
                additional_directories: Vec::new(),
                environment,
                session_meta: Default::default(),
                options: BTreeMap::new(),
            });
            let node = candidate_config
                .as_ref()
                .is_some_and(super::acp::Config::requires_node);
            let availability = if node {
                Availability::RuntimeUnsupported
            } else if runtime_missing {
                Availability::RuntimeMissing
            } else if executable.is_some() {
                Availability::Installed
            } else if native.is_some() {
                Availability::BridgeMissing
            } else {
                Availability::Unavailable
            };
            let detail = match availability {
                Availability::Installed => "Installed executable found; authentication, capabilities and inference need Check and a real turn.".into(),
                Availability::BridgeMissing => "Your AI is installed, but its ACP bridge is missing. Choose a native Fiote connection or supply a compatible executable that does not require Node.js.".into(),
                Availability::RuntimeMissing => format!("The ACP executable requires the missing runtime {}. Put that runtime on the connection's PATH.", runtime.unwrap_or_default()),
                Availability::Unavailable => "No installed executable found. You can supply another command or install this ACP agent.".into(),
                Availability::RuntimeUnsupported => "Node.js agent launchers are not supported. Use a native Fiote connection or an ACP executable that does not require Node.js.".into(),
            };
            let config = if availability == Availability::Installed {
                candidate_config.filter(|config| config.args.len() <= 32)
            } else {
                None
            };
            candidates.push(Candidate {
                id: agent.id.clone(),
                name: agent.name.clone(),
                registry_version: agent.version.clone(),
                availability,
                executable,
                native_executable: native,
                detail,
                installation,
                config,
            });
        }
        candidates.sort_by_key(|entry| {
            (
                entry.availability != Availability::Installed,
                entry.native_executable.is_none(),
                entry.name.to_lowercase(),
            )
        });
        Discovery { candidates, detail: "Local discovery reads executable metadata without starting an agent, opening credentials or sending a prompt.".into() }
    }
}

fn valid_distribution(distribution: &Value) -> bool {
    let args_valid = |value: &Value| {
        value.is_null()
            || value.as_array().is_some_and(|args| {
                args.len() <= 32
                    && args.iter().all(|arg| {
                        arg.as_str()
                            .is_some_and(|arg| arg.len() <= 4096 && !arg.contains('\0'))
                    })
            })
    };
    let env_valid = |value: &Value| {
        value.is_null()
            || value.as_object().is_some_and(|env| {
                env.len() <= 30
                    && env.iter().all(|(key, value)| {
                        !key.is_empty()
                            && key.len() <= 80
                            && key
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                            && !matches!(
                                key.as_str(),
                                "HOME" | "CODEX_HOME" | "XDG_CONFIG_HOME" | "XDG_DATA_HOME"
                            )
                            && value
                                .as_str()
                                .is_some_and(|value| value.len() <= 4096 && !value.contains('\0'))
                    })
            })
    };
    for route in ["npx", "uvx"] {
        if let Some(package) = distribution[route]["package"].as_str()
            && (package.len() > 256
                || !package
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"@/._-+=".contains(&byte)))
        {
            return false;
        }
        if !args_valid(&distribution[route]["args"]) || !env_valid(&distribution[route]["env"]) {
            return false;
        }
    }
    distribution["binary"].as_object().is_none_or(|platforms| {
        platforms
            .values()
            .all(|binary| args_valid(&binary["args"]) && env_valid(&binary["env"]))
    })
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests;
