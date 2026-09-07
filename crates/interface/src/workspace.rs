use crate::LaboratoryReport;
use chrono::Utc;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeWorkspaceKind {
    Empty,
    Laboratory { run_directory: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeWorkspace {
    pub id: String,
    pub name: String,
    pub kind: NativeWorkspaceKind,
}

#[derive(Clone, Debug)]
pub struct NativeWorkspaceManager {
    data_directory: PathBuf,
    port: u16,
    workspaces: Vec<NativeWorkspace>,
    active: usize,
    next_id: u64,
}

#[derive(Serialize)]
struct LaboratoryRunManifest<'a> {
    schema_version: u16,
    workspace_id: &'a str,
    workspace_name: &'a str,
    lince_directory: String,
    run_directory: String,
    isolated_workspace_directory: String,
    port: u16,
    started_at: String,
    started_unix_millis: u128,
    profile: &'static str,
}

impl NativeWorkspaceManager {
    pub fn from_process() -> Self {
        let args = std::env::args().collect::<Vec<_>>();
        let port = argument_value(&args, "--port")
            .and_then(|value| value.parse::<u16>().ok())
            .or_else(|| {
                argument_value(&args, "--listen-addr")
                    .and_then(|value| value.rsplit_once(':').map(|(_, port)| port.to_owned()))
                    .and_then(|value| value.parse::<u16>().ok())
            })
            .or_else(|| {
                std::env::var("LINCE_PORT")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
            })
            .unwrap_or(6174);
        let data_directory = argument_value(&args, "--directory")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LINCE_DIRECTORY").map(PathBuf::from))
            .or_else(utils::config::lince_data_dir)
            .or_else(|| dirs::config_dir().map(|directory| directory.join("lince")))
            .unwrap_or_else(|| PathBuf::from(".lince"));
        Self::new(data_directory, port)
    }

    pub fn new(data_directory: PathBuf, port: u16) -> Self {
        Self {
            data_directory,
            port,
            workspaces: vec![NativeWorkspace {
                id: "workspace-1".into(),
                name: "Workspace 1".into(),
                kind: NativeWorkspaceKind::Empty,
            }],
            active: 0,
            next_id: 2,
        }
    }

    pub fn active(&self) -> &NativeWorkspace {
        &self.workspaces[self.active]
    }

    pub fn workspaces(&self) -> &[NativeWorkspace] {
        &self.workspaces
    }

    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    pub fn tests_directory(&self) -> PathBuf {
        self.data_directory.join("tests")
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn rename_active(&mut self, name: &str) -> Result<(), String> {
        let name = clean_workspace_name(name)?;
        self.workspaces[self.active].name = name;
        Ok(())
    }

    pub fn start_laboratory(&mut self) -> Result<&NativeWorkspace, String> {
        let now = Utc::now();
        let run_id = now.format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string();
        let run_directory = self.tests_directory().join(&run_id);
        let isolated_workspace = run_directory.join("workspace");
        fs::create_dir_all(&isolated_workspace).map_err(|error| error.to_string())?;
        let workspace = NativeWorkspace {
            id: format!("workspace-{}", self.next_id),
            name: format!("Laboratory {run_id}"),
            kind: NativeWorkspaceKind::Laboratory {
                run_directory: run_directory.clone(),
            },
        };
        self.next_id = self.next_id.saturating_add(1);
        let started_unix_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis();
        write_json(
            &run_directory.join("run.json"),
            &LaboratoryRunManifest {
                schema_version: 1,
                workspace_id: &workspace.id,
                workspace_name: &workspace.name,
                lince_directory: self.data_directory.display().to_string(),
                run_directory: run_directory.display().to_string(),
                isolated_workspace_directory: isolated_workspace.display().to_string(),
                port: self.port,
                started_at: now.to_rfc3339(),
                started_unix_millis,
                profile: "native-box-laboratory",
            },
        )?;
        self.workspaces.push(workspace);
        self.active = self.workspaces.len() - 1;
        Ok(self.active())
    }

    pub fn write_active_laboratory_report(
        &self,
        report: &LaboratoryReport,
    ) -> Result<Option<PathBuf>, String> {
        let NativeWorkspaceKind::Laboratory { run_directory } = &self.active().kind else {
            return Ok(None);
        };
        let path = run_directory.join("report.json");
        report.write_pretty(&path)?;
        Ok(Some(path))
    }

    pub fn delete_active(&mut self) -> NativeWorkspace {
        let removed = self.workspaces.remove(self.active);
        if self.workspaces.is_empty() {
            self.workspaces.push(NativeWorkspace {
                id: format!("workspace-{}", self.next_id),
                name: "Workspace 1".into(),
                kind: NativeWorkspaceKind::Empty,
            });
            self.next_id = self.next_id.saturating_add(1);
            self.active = 0;
        } else {
            self.active = self.active.min(self.workspaces.len() - 1);
        }
        removed
    }
}

fn clean_workspace_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("workspace name cannot be empty".into());
    }
    if name.chars().count() > 80 {
        return Err("workspace name cannot exceed 80 characters".into());
    }
    if name.chars().any(char::is_control) {
        return Err("workspace name cannot contain control characters".into());
    }
    Ok(name.into())
}

fn argument_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .or_else(|| {
            args.iter().find_map(|argument| {
                argument
                    .strip_prefix(&format!("{name}="))
                    .map(str::to_owned)
            })
        })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lince-native-workspace-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn deleting_the_last_workspace_creates_a_new_empty_one() {
        let mut manager = NativeWorkspaceManager::new(test_directory("last"), 6174);
        let first_id = manager.active().id.clone();
        manager.delete_active();
        assert_eq!(manager.workspaces().len(), 1);
        assert_ne!(manager.active().id, first_id);
        assert_eq!(manager.active().kind, NativeWorkspaceKind::Empty);
    }

    #[test]
    fn laboratory_gets_a_visible_workspace_and_isolated_evidence_directory() {
        let directory = test_directory("laboratory");
        let mut manager = NativeWorkspaceManager::new(directory.clone(), 6176);
        manager.start_laboratory().unwrap();
        let NativeWorkspaceKind::Laboratory { run_directory } = &manager.active().kind else {
            panic!("expected laboratory workspace");
        };
        assert!(run_directory.starts_with(directory.join("tests")));
        assert!(run_directory.join("workspace").is_dir());
        assert!(run_directory.join("run.json").is_file());
        let report = LaboratoryReport::ownership_preflight(HostFacts::unavailable(
            1.0,
            800,
            600,
            "test adapter".into(),
        ));
        let report_path = manager
            .write_active_laboratory_report(&report)
            .unwrap()
            .expect("active laboratory report path");
        assert_eq!(report_path, run_directory.join("report.json"));
        assert!(report_path.is_file());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn workspace_names_are_bounded_plain_text() {
        let mut manager = NativeWorkspaceManager::new(test_directory("names"), 6174);
        assert!(manager.rename_active("  Team tasks  ").is_ok());
        assert_eq!(manager.active().name, "Team tasks");
        assert!(manager.rename_active("\n").is_err());
        assert!(manager.rename_active(&"x".repeat(81)).is_err());
    }
}
