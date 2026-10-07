use super::*;
use fiote::connection::{self, Connection, Profiles};

impl Host {
    pub(super) async fn save_native_profile(
        &self,
        record: &str,
        settings: &Settings,
    ) -> Result<(), String> {
        let _lock = self.profile_lock.lock().await;
        let mut profiles = self.profile_status(record)?.profiles;
        let id = if settings.account.is_empty() {
            "native".to_string()
        } else {
            format!(
                "native-{}",
                settings.account.rsplit(':').next().unwrap_or("account")
            )
        };
        let name = self
            .vault
            .key(&vault::slot(settings))
            .await
            .ok()
            .flatten()
            .as_ref()
            .and_then(|key| fiote::communication::native::account_label(settings, key))
            .unwrap_or_else(|| {
                self.catalog
                    .descriptor(&settings.provider)
                    .map(|d| d.label.clone())
                    .unwrap_or_else(|_| "Native Fiote".into())
            });
        let profile = connection::Profile {
            id: id.clone(),
            name: name.chars().take(80).collect(),
            connection: Connection::Model {
                settings: settings.clone(),
            },
        };
        if let Some(existing) = profiles.entries.iter_mut().find(|p| p.id == id) {
            *existing = profile;
        } else {
            profiles.entries.push(profile);
        }
        profiles.selected = Some(id);
        profiles.validate()?;
        save(&self.profile_path(record)?, &profiles)
    }

    fn profile_path(&self, record: &str) -> Result<PathBuf, String> {
        Ok(self.path(record)?.with_extension("connections.json"))
    }

    pub(super) fn profile_status(&self, record: &str) -> Result<connection::Status, String> {
        let profiles = match std::fs::read(self.profile_path(record)?) {
            Ok(bytes) => {
                if bytes.len() > 524_288 {
                    return Err("Saved connections exceed their size limit.".into());
                }
                let mut profiles: Profiles =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                profiles.validate()?;
                profiles
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Profiles::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(connection::Status {
            profiles,
            check: std::fs::read(self.path(record)?.with_extension("check.json"))
                .ok()
                .filter(|bytes| bytes.len() <= 1024 * 1024)
                .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
            discovery: std::fs::read(self.directory.join("discovery.json"))
                .ok()
                .filter(|bytes| bytes.len() <= 2_097_152)
                .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
        })
    }

    pub(super) async fn remember_connection_configuration(
        &self,
        record: &str,
    ) -> Result<(), String> {
        let _profile_lock = self.profile_lock.lock().await;
        let mut profiles = self.profile_status(record)?.profiles;
        let Some(selected) = profiles.selected.clone() else {
            return Ok(());
        };
        let saved = self
            .load(record)?
            .ok_or("Fiote configuration is unavailable.")?;
        let profile = profiles
            .entries
            .iter_mut()
            .find(|profile| profile.id == selected)
            .ok_or("The selected connection no longer exists.")?;
        match (&mut profile.connection, saved.agent) {
            (Connection::Harness { config }, Some(current)) => *config = current,
            (Connection::Model { settings }, None) if saved.settings.enabled => {
                *settings = saved.settings
            }
            _ => profiles.selected = None,
        }
        profiles.validate()?;
        save(&self.profile_path(record)?, &profiles)
    }

    pub(super) async fn connection_request(
        &self,
        record: &str,
        request: connection::Request,
    ) -> Result<Status, String> {
        let _profile_lock = self.profile_lock.lock().await;
        self.record(record).await?;
        let mut status = self.profile_status(record)?;
        match request {
            connection::Request::Inspect => {}
            connection::Request::Discover { refresh_registry } => {
                let path = self.directory.join("agent-registry.json");
                let cached = std::fs::read(&path).ok().and_then(|bytes| {
                    fiote::communication::discovery::Registry::parse(&bytes).ok()
                });
                let mut registry =
                    cached.unwrap_or_else(fiote::communication::discovery::Registry::bundled);
                let mut error = None;
                if refresh_registry {
                    match fiote::communication::discovery::Registry::refresh().await {
                        Ok(current) => {
                            save(&path, &current)?;
                            registry = current;
                        }
                        Err(detail) => error = Some(detail),
                    }
                }
                let directory = std::env::current_dir().map_err(|e| e.to_string())?;
                let mut discovered =
                    fiote::communication::discovery::Search::current(directory).scan(&registry);
                if let Some(error) = error {
                    discovered.detail = error;
                }
                save(&self.directory.join("discovery.json"), &discovered)?;
                status.discovery = Some(discovered);
            }
            connection::Request::Deselect => {
                let running = self.running.lock().await;
                if running.values().any(|run| run.record == record) {
                    return Err("Stop running turns before disconnecting this connection.".into());
                }
                self.agents.close_record(record).await;
                let removed = {
                    let mut connections = self.connections.lock().await;
                    let threads: Vec<_> = connections
                        .iter()
                        .filter(|(_, open)| open.record == record)
                        .map(|(thread, _)| thread.clone())
                        .collect();
                    threads
                        .into_iter()
                        .filter_map(|thread| connections.remove(&thread))
                        .collect::<Vec<_>>()
                };
                for open in removed {
                    open.connection.close().await;
                }
                save(
                    &self.path(record)?,
                    &Configuration {
                        settings: Settings::default(),
                        agent: None,
                        author: record.into(),
                    },
                )?;
                status.profiles.selected = None;
                save(&self.profile_path(record)?, &status.profiles)?;
            }
            connection::Request::Save { mut profile } => {
                profile.validate()?;
                if let Connection::Model { settings } = &mut profile.connection {
                    self.catalog.validate(settings)?;
                }
                if status.profiles.selected.as_deref() == Some(&profile.id) {
                    return Err(
                        "Disconnect this connection before editing the active profile.".into(),
                    );
                }
                if let Some(previous) = status
                    .profiles
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id == profile.id)
                {
                    *previous = profile;
                } else {
                    status.profiles.entries.push(profile);
                }
                status.profiles.validate()?;
                save(&self.profile_path(record)?, &status.profiles)?;
            }
            connection::Request::Remove { id } => {
                if status.profiles.selected.as_deref() == Some(&id) {
                    return Err(
                        "Disconnect this connection before removing the active profile.".into(),
                    );
                }
                if !status.profiles.entries.iter().any(|entry| entry.id == id) {
                    return Err("This connection no longer exists.".into());
                }
                status.profiles.entries.retain(|entry| entry.id != id);
                save(&self.profile_path(record)?, &status.profiles)?;
            }
            connection::Request::Select {
                id,
                api_key,
                password,
            } => {
                let profile = status
                    .profiles
                    .entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or("This connection no longer exists.")?;
                match &profile.connection {
                    Connection::Model { settings } => {
                        let mut settings = settings.clone();
                        settings.enabled = true;
                        self.configure(record, settings, api_key, password).await?;
                    }
                    Connection::Harness { config } => {
                        if api_key.is_some() || password.is_some() {
                            return Err("The harness manages its own authentication.".into());
                        }
                        self.configure_agent(record, config.clone()).await?;
                    }
                    Connection::External => {
                        if api_key.is_some() || password.is_some() {
                            return Err("External tool connections use their scoped connection credentials.".into());
                        }
                        if self
                            .running
                            .lock()
                            .await
                            .values()
                            .any(|run| run.record == record)
                        {
                            return Err("Stop running turns before switching connections.".into());
                        }
                        self.agents.close_record(record).await;
                        save(
                            &self.path(record)?,
                            &Configuration {
                                settings: Settings::default(),
                                agent: None,
                                author: record.into(),
                            },
                        )?;
                    }
                }
                status.profiles.selected = Some(id);
                save(&self.profile_path(record)?, &status.profiles)?;
            }
            connection::Request::Logout { id } => {
                if self
                    .running
                    .lock()
                    .await
                    .values()
                    .any(|run| run.record == record)
                {
                    return Err("Stop active turns before signing out.".into());
                }
                let profile = status
                    .profiles
                    .entries
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or("This connection no longer exists.")?;
                let Connection::Model { settings } = &profile.connection else {
                    return Err("This connection manages authentication externally.".into());
                };
                if !fiote::communication::native::has_native_login(settings) {
                    return Err("Use the connection's credential editor for API keys.".into());
                }
                let slot = vault::slot(settings);
                let lock = self
                    .refresh_locks
                    .lock()
                    .await
                    .entry(slot.clone())
                    .or_default()
                    .clone();
                let _guard = lock.lock().await;
                let credential = vault::Credential {
                    vault: self.vault.clone(),
                    directory: self.directory.clone(),
                    slot: slot.clone(),
                };
                let _process_guard =
                    fiote::communication::auth::CredentialStore::refresh_guard(&credential).await?;
                let key = fiote::communication::auth::CredentialStore::read(&credential).await?;
                let (key, detail) = fiote::communication::native::logout(settings, &key).await?;
                self.vault.put(slot, key).await?;
                status.check = Some(connection::Check {
                    profile: id,
                    ready: false,
                    detail,
                    capabilities: Default::default(),
                    settings: serde_json::Value::Null,
                    stage: fiote::communication::check::Stage::Authentication,
                });
            }
            connection::Request::Test { id } => {
                let profile = status
                    .profiles
                    .entries
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or("This connection no longer exists.")?;
                let Connection::Model { settings } = &profile.connection else {
                    return Err(
                        "Send a conversation message to test this external connection.".into(),
                    );
                };
                let credential =
                    if self.catalog.method(settings)?.kind == fiote::adapters::AuthKind::None {
                        Secret::default()
                    } else {
                        self.vault
                            .key(&vault::slot(settings))
                            .await?
                            .ok_or("Unlock this connection's credential vault.")?
                    };
                let lock = self
                    .refresh_locks
                    .lock()
                    .await
                    .entry(vault::slot(settings))
                    .or_default()
                    .clone();
                let provider: Arc<dyn Provider> =
                    if let Some(driver) = self.catalog.driver(&settings.provider).cloned() {
                        Arc::new(fiote::driver::DriverProvider::new(
                            driver,
                            settings.clone(),
                            credential,
                        ))
                    } else {
                        fiote::communication::native::provider(
                            settings,
                            &credential,
                            Some(Arc::new(vault::Credential {
                                vault: self.vault.clone(),
                                directory: self.directory.clone(),
                                slot: vault::slot(settings),
                            })),
                            Some(lock),
                        )?
                    };
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(120),
                    provider.complete(
                        "This is a connection acceptance test. Do not call tools.",
                        &[Message::User("Say hello in one short sentence.".into())],
                        &[],
                    ),
                )
                .await
                .map_err(|_| "The inference test timed out.".to_string())
                .and_then(|r| r);
                let (ready, detail) = match result {
                    Ok(reply) if reply.calls.is_empty() && !reply.text.trim().is_empty() => {
                        if let Some(report) = reply.usage {
                            usage::record(
                                &self
                                    .directory
                                    .join(format!("connection-usage-{record}.json")),
                                report,
                            )?;
                        }
                        (
                            true,
                            format!(
                                "Completed model response: {}",
                                reply.text.chars().take(256).collect::<String>()
                            ),
                        )
                    }
                    Ok(_) => (
                        false,
                        "The model returned no text or unexpected tools.".into(),
                    ),
                    Err(error) => (false, error),
                };
                status.check = Some(connection::Check {
                    profile: id.clone(),
                    ready,
                    detail,
                    capabilities: connection::Capabilities::model(),
                    settings: status
                        .check
                        .as_ref()
                        .filter(|c| c.profile == id)
                        .map(|c| c.settings.clone())
                        .unwrap_or_default(),
                    stage: fiote::communication::check::Stage::Inference,
                });
            }
            connection::Request::Check { id } => {
                let profile = status
                    .profiles
                    .entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .ok_or("This connection no longer exists.")?;
                let credential = if let Connection::Model { settings } = &profile.connection {
                    if self.catalog.method(settings)?.kind != fiote::adapters::AuthKind::None
                        && !self.vault.status().await?.1
                    {
                        self.vault.key(&vault::slot(settings)).await?
                    } else {
                        None
                    }
                } else {
                    None
                };
                let session = if let Connection::Model { settings } = &profile.connection {
                    let slot = vault::slot(settings);
                    let lock = self
                        .refresh_locks
                        .lock()
                        .await
                        .entry(slot.clone())
                        .or_default()
                        .clone();
                    Some(fiote::communication::native::session(
                        Arc::new(vault::Credential {
                            vault: self.vault.clone(),
                            directory: self.directory.clone(),
                            slot,
                        }),
                        lock,
                    )?)
                } else {
                    None
                };
                let checked = fiote::communication::check::probe_with_session(
                    profile,
                    &self.catalog,
                    credential.as_ref(),
                    session,
                )
                .await;
                status.check = Some(checked);
            }
        }
        if let Some(check) = &status.check {
            save(&self.path(record)?.with_extension("check.json"), check)?;
        }
        let mut response = self.status(record).await?;
        response.connections = status;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiote::acp;
    use fiote::connection::{Profile, Request};
    use std::collections::BTreeMap;
    use transport::fiote::Service;

    #[tokio::test]
    async fn local_model_profile_keeps_normal_configuration_changes_without_credentials() {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let root = tempfile::tempdir().unwrap();
        let host = Host::open(engine.clone(), root.path().join("fiote"))
            .await
            .unwrap();
        let record = engine
            .act(
                Action::CreateAgent {
                    head: "Local assistant".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let descriptor = host
            .catalog
            .descriptors
            .iter()
            .find(|descriptor| {
                descriptor
                    .auth_methods
                    .iter()
                    .any(|method| method.kind == fiote::adapters::AuthKind::None)
            })
            .unwrap();
        let method = descriptor
            .auth_methods
            .iter()
            .find(|method| method.kind == fiote::adapters::AuthKind::None)
            .unwrap();
        let mut settings = Settings {
            enabled: true,
            provider: descriptor.id.clone(),
            auth_method: method.id.clone(),
            model: "local-model".into(),
            endpoint: "http://127.0.0.1:11434/".into(),
            directory: root.path().into(),
            ..Default::default()
        };
        for request in [
            Request::Save {
                profile: Profile {
                    id: "local".into(),
                    name: "Local model".into(),
                    connection: Connection::Model {
                        settings: settings.clone(),
                    },
                },
            },
            Request::Select {
                id: "local".into(),
                api_key: None,
                password: None,
            },
        ] {
            host.handle(fiote::config::Request::Connections {
                record: record.clone(),
                request,
            })
            .await
            .unwrap();
        }
        settings.model = "different-local-model".into();
        host.handle(fiote::config::Request::Configure {
            record: record.clone(),
            settings,
            api_key: None,
            password: None,
        })
        .await
        .unwrap();
        for request in [
            Request::Deselect,
            Request::Select {
                id: "local".into(),
                api_key: None,
                password: None,
            },
        ] {
            host.handle(fiote::config::Request::Connections {
                record: record.clone(),
                request,
            })
            .await
            .unwrap();
        }
        let saved = host.load(&record).unwrap().unwrap();
        assert!(saved.agent.is_none());
        assert_eq!(saved.settings.model, "different-local-model");
        assert!(!host.status(&record).await.unwrap().requires_credential);
    }

    #[tokio::test]
    async fn configured_harness_choices_survive_disconnect_and_reselect() {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let root = tempfile::tempdir().unwrap();
        let host = Host::open(engine.clone(), root.path().join("fiote"))
            .await
            .unwrap();
        let record = engine
            .act(
                Action::CreateAgent {
                    head: "Assistant".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let config = acp::Config {
            command: "own-compatible-harness".into(),
            args: Vec::new(),
            directory: root.path().into(),
            require_vault: false,
            additional_directories: Vec::new(),
            environment: BTreeMap::new(),
            session_meta: serde_json::Map::new(),
            options: BTreeMap::new(),
        };
        for request in [
            Request::Save {
                profile: Profile {
                    id: "harness".into(),
                    name: "Own harness".into(),
                    connection: Connection::Harness {
                        config: config.clone(),
                    },
                },
            },
            Request::Select {
                id: "harness".into(),
                api_key: None,
                password: None,
            },
        ] {
            host.handle(fiote::config::Request::Connections {
                record: record.clone(),
                request,
            })
            .await
            .unwrap();
        }
        let mut configured = config;
        configured
            .options
            .insert("effort".into(), serde_json::json!("low"));
        configured
            .options
            .insert("fast".into(), serde_json::json!(true));
        host.handle(fiote::config::Request::AgentConfigure {
            record: record.clone(),
            config: configured.clone(),
        })
        .await
        .unwrap();
        for request in [
            Request::Deselect,
            Request::Select {
                id: "harness".into(),
                api_key: None,
                password: None,
            },
        ] {
            host.handle(fiote::config::Request::Connections {
                record: record.clone(),
                request,
            })
            .await
            .unwrap();
        }
        assert_eq!(
            host.load(&record).unwrap().unwrap().agent.unwrap().options,
            configured.options
        );
        let restarted = Host::open(engine, root.path().join("fiote")).await.unwrap();
        let profiles = restarted.profile_status(&record).unwrap().profiles;
        assert_eq!(profiles.selected.as_deref(), Some("harness"));
        let Connection::Harness { config } = &profiles.entries[0].connection else {
            panic!("harness")
        };
        assert_eq!(config.options, configured.options);
    }

    #[tokio::test]
    async fn saved_profiles_select_external_and_survive_restart_without_model_calls() {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let root = tempfile::tempdir().unwrap();
        let host = Host::open(engine.clone(), root.path().join("fiote"))
            .await
            .unwrap();
        let record = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: RecordKind::Person,
                    head: "Assistant".into(),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let request = fiote::config::Request::Connections {
            record: record.clone(),
            request: Request::Save {
                profile: Profile {
                    id: "own-ai".into(),
                    name: "My external AI".into(),
                    connection: Connection::External,
                },
            },
        };
        let status = host.handle(request).await.unwrap();
        assert_eq!(status.connections.profiles.entries.len(), 1);
        let status = host
            .handle(fiote::config::Request::Connections {
                record: record.clone(),
                request: Request::Select {
                    id: "own-ai".into(),
                    api_key: None,
                    password: None,
                },
            })
            .await
            .unwrap();
        assert!(!status.settings.enabled);
        assert_eq!(
            status.connections.profiles.selected.as_deref(),
            Some("own-ai")
        );
        let host = Host::open(engine, root.path().join("fiote")).await.unwrap();
        let status = host
            .handle(fiote::config::Request::Connections {
                record: record.clone(),
                request: Request::Check {
                    id: "own-ai".into(),
                },
            })
            .await
            .unwrap();
        assert_eq!(
            status
                .connections
                .check
                .unwrap()
                .capabilities
                .automatic_activation,
            connection::Support::Unsupported
        );
        let status = host
            .handle(fiote::config::Request::Connections {
                record: record.clone(),
                request: Request::Deselect,
            })
            .await
            .unwrap();
        assert!(status.connections.profiles.selected.is_none());
        assert_eq!(status.connections.profiles.entries.len(), 1);
        host.handle(fiote::config::Request::Connections {
            record,
            request: Request::Remove {
                id: "own-ai".into(),
            },
        })
        .await
        .unwrap();
    }
}
