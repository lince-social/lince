use super::*;
use fiote::{acp, config::WorkingDirectory};
use std::collections::BTreeSet;

impl Host {
    pub(super) async fn effective_working_directory(
        &self,
        record: &str,
    ) -> Result<Option<(PathBuf, String)>, String> {
        let mut visited = BTreeSet::new();
        let mut next = Some(record.to_owned());
        let mut selected = None;
        while let Some(uid) = next {
            if !visited.insert(uid.clone()) || visited.len() > 32 {
                return Err(
                    "Working directory ancestry contains a cycle or exceeds 32 Fiotes.".into(),
                );
            }
            self.record(&uid).await?;
            if selected.is_none()
                && let Some(config) = self.load(&uid)?
                && !config.settings.directory.as_os_str().is_empty()
            {
                selected = Some((config.settings.directory, uid.clone()));
            }
            next = self
                .engine
                .fiote_parent(&uid)
                .await
                .map_err(|e| e.to_string())?;
        }
        if let Some((directory, source)) = selected {
            let mut settings = Settings {
                directory,
                ..Default::default()
            };
            settings
                .validate()
                .map_err(|error| format!("Working directory from Fiote {source}: {error}"))?;
            Ok(Some((settings.directory, source)))
        } else {
            Ok(None)
        }
    }

    pub(super) async fn working_directory_status(&self, record: &str) -> WorkingDirectory {
        let configured = match self.load(record) {
            Ok(config) => config
                .map(|config| config.settings.directory)
                .unwrap_or_default(),
            Err(error) => {
                return WorkingDirectory {
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        match self.effective_working_directory(record).await {
            Ok(Some((effective, source))) => WorkingDirectory {
                configured,
                effective: Some(effective),
                source: Some(source),
                error: None,
            },
            Ok(None) => WorkingDirectory {
                configured,
                ..Default::default()
            },
            Err(error) => WorkingDirectory {
                configured,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    pub(super) async fn agent_working_directory(
        &self,
        record: &str,
        mut config: acp::Config,
    ) -> Result<acp::Config, String> {
        config.directory = self
            .effective_working_directory(record)
            .await?
            .ok_or("Choose a working directory on this Fiote or one of its ancestors before starting the external agent.")?
            .0;
        Ok(config)
    }

    pub(super) async fn set_working_directory(
        &self,
        record: &str,
        directory: PathBuf,
    ) -> Result<(), String> {
        self.prepare(record).await?;
        let mut settings = Settings {
            directory,
            ..Default::default()
        };
        settings.validate()?;
        let running = self.running.lock().await;
        if running.values().any(|run| run.record == record) {
            return Err("Stop this Fiote before changing its working directory.".into());
        }
        let mut config = self.load(record)?.unwrap_or_else(|| Configuration {
            author: record.to_owned(),
            settings: Settings::default(),
            agent: None,
        });
        config.settings.directory = settings.directory;
        save(&self.path(record)?, &config)?;
        drop(running);
        if config.agent.is_none() && config.settings.enabled {
            self.remember_connection_configuration(record).await?;
        }
        Ok(())
    }
}
