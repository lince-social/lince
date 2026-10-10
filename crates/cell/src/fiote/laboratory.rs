use super::*;
pub use ::fiote::provider as inference;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub const NAMESPACE: &str = "lince.fiote-laboratory";
pub const HELLO: &str =
    "Hello! Please reply with a short greeting. This is a connection test; do not use tools.";

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Options {
    pub record: Option<String>,
    pub separate: bool,
    pub profile: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub version: String,
    pub revision: String,
    pub outcome: String,
    pub stage: String,
    pub detail: String,
    pub record: Option<String>,
    pub thread: Option<String>,
    pub provider: String,
    pub model: String,
    pub reasoning: Option<String>,
    pub fast: bool,
    pub tools_enabled: bool,
    pub elapsed_ms: u64,
    pub reply: String,
    pub persisted: bool,
    pub view: String,
    pub timings: Vec<(String, u64)>,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            schema: 1,
            version: env!("CARGO_PKG_VERSION").into(),
            revision: utils::build_info::revision().into(),
            outcome: "running".into(),
            stage: "connection".into(),
            detail: "Inspecting native connections".into(),
            record: None,
            thread: None,
            provider: String::new(),
            model: String::new(),
            reasoning: None,
            fast: false,
            tools_enabled: false,
            elapsed_ms: 0,
            reply: String::new(),
            persisted: false,
            view: "not-checked".into(),
            timings: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Prepared {
    pub report: Report,
    pub data: Value,
}

pub enum Preparation {
    Setup { record: String, detail: String },
    Ready(Prepared),
}

impl Host {
    pub async fn laboratory_choices(&self) -> Result<Vec<fiote::config::FioteChoice>, String> {
        self.fiote_choices().await
    }

    pub async fn laboratory_unlock(&self, password: Secret) -> Result<(), String> {
        self.vault.unlock(password).await
    }

    pub async fn laboratory_prepare(&self, options: &Options) -> Result<Preparation, String> {
        let record = match &options.record {
            Some(record) => record.clone(),
            None => self
                .fiote_choices()
                .await?
                .first()
                .ok_or("No Fiote exists yet.")?
                .record
                .clone(),
        };
        self.record(&record).await?;
        let selected_settings = if let Some(profile) = &options.profile {
            let profiles = self.profile_status(&record)?.profiles;
            let selected = profiles
                .entries
                .iter()
                .find(|entry| &entry.id == profile)
                .ok_or("The selected connection does not exist for this Fiote.")?;
            let fiote::connection::Connection::Model { settings } = &selected.connection else {
                return Err("Choose a native connection to test Fiote's own harness.".into());
            };
            Some(settings.clone())
        } else {
            None
        };
        let status = self.status(&record).await?;
        if status.login_pending {
            self.handle(Request::BrowserPoll {
                record: record.clone(),
            })
            .await?;
            return Ok(Preparation::Setup {
                record,
                detail: "Finish authorization in your browser.".into(),
            });
        }
        let config = self.load(&record)?;
        let Some(config) = config.filter(|config| {
            selected_settings.is_some() || (config.settings.enabled && config.agent.is_none())
        }) else {
            return Ok(Preparation::Setup {
                record,
                detail: "Choose a native subscription, API-key, or local-model connection.".into(),
            });
        };
        let mut settings = selected_settings.unwrap_or(config.settings);
        let requires_credential =
            self.catalog.method(&settings)?.kind != fiote::adapters::AuthKind::None;
        if requires_credential && status.locked {
            return Ok(Preparation::Setup {
                record,
                detail: "Unlock the provider vault; the saved login is retained.".into(),
            });
        }
        if requires_credential && self.vault.key(&vault::slot(&settings)).await?.is_none() {
            return Ok(Preparation::Setup {
                record,
                detail: "Connect this account before running the test.".into(),
            });
        }
        if let Some(model) = &options.model {
            settings.model = model.clone();
        }
        if let Some(reasoning) = &options.reasoning {
            settings.reasoning = Some(reasoning.clone());
        }
        settings.enabled = true;
        self.catalog.validate(&mut settings)?;
        let target = if options.separate {
            let created = self
                .engine
                .act(
                    Action::CreateAgent {
                        head: "Laboratory Fiote".into(),
                        operated_by: None,
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?
                .created
                .ok_or("Cannot create Laboratory Fiote.")?;
            self.engine
                .act(
                    Action::ConfigureFiote {
                        target: created.clone(),
                        prompt_parent: None,
                        run_assigned: false,
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
            self.configure(&created, settings.clone(), None, None)
                .await?;
            created
        } else {
            record
        };
        let thread = self
            .engine
            .act(
                Action::CreateThread {
                    target: target.clone(),
                    head: format!("Laboratory hello · {}", nucleus::operation::now_ms()),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?
            .created
            .ok_or("Cannot create test thread.")?;
        self.engine
            .act(
                Action::SetExtension {
                    target: thread.clone(),
                    namespace: NAMESPACE.into(),
                    fds: json!({"record":target,"settings":settings,"tools":false}),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        let data = self.laboratory_data(&target, &thread).await?;
        Ok(Preparation::Ready(Prepared {
            data,
            report: Report {
                record: Some(target),
                thread: Some(thread),
                provider: settings.provider.0,
                model: settings.model,
                reasoning: settings.reasoning,
                fast: settings.fast,
                stage: "sending".into(),
                detail: "Sending a greeting through the normal conversation workflow".into(),
                ..Default::default()
            },
        }))
    }

    pub async fn laboratory_authorize(
        &self,
        record: &str,
        profile: Option<&str>,
    ) -> Result<Option<String>, String> {
        let status = self.status(record).await?;
        if status.login_pending {
            return Ok(status.login_url);
        }
        let Some(password) = self.vault.unlocked_password().await else {
            return Ok(None);
        };
        let profile_settings = profile
            .map(|id| {
                self.profile_status(record)?
                    .profiles
                    .entries
                    .into_iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| match entry.connection {
                        fiote::connection::Connection::Model { settings } => Some(settings),
                        _ => None,
                    })
                    .ok_or_else(|| "Choose a native connection to authorize".to_string())
            })
            .transpose()?;
        if profile_settings
            .as_ref()
            .is_some_and(|settings| !fiote::communication::native::has_native_login(settings))
        {
            return Ok(None);
        }
        if profile_settings.is_none()
            && self.load(record)?.is_some_and(|config| {
                config.agent.is_none()
                    && config.settings.enabled
                    && !fiote::communication::native::has_native_login(&config.settings)
            })
        {
            return Ok(None);
        }
        let settings = profile_settings
            .or(self
                .load(record)?
                .filter(|config| {
                    config.agent.is_none()
                        && fiote::communication::native::has_native_login(&config.settings)
                })
                .map(|config| config.settings))
            .unwrap_or_else(|| Settings {
                enabled: true,
                provider: fiote::config::ProviderKind("chatgpt".into()),
                ..Default::default()
            });
        let status = self
            .handle(Request::BrowserStart {
                record: record.into(),
                password,
                settings,
            })
            .await?;
        Ok(status.login_url)
    }

    pub async fn laboratory_data(&self, record: &str, thread: &str) -> Result<Value, String> {
        let query = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":record}],
            "fields":["uid","head","threads"],"include":{"threads":{"messages_limit":50}},"limit":1}))
            .map_err(|error| error.to_string())?;
        let mut rows = protein::execute(&self.engine.store, &query)
            .await
            .map_err(|error| error.to_string())?;
        let mut row = rows.pop().ok_or("Test conversation disappeared.")?;
        if let Some(threads) = row["threads"].as_array_mut() {
            threads.retain(|row| row["uid"] == thread);
            if threads.is_empty() {
                return Err("Test thread disappeared.".into());
            }
        } else {
            return Err("Test thread disappeared.".into());
        }
        Ok(row)
    }

    pub async fn laboratory_verify(&self, report: Report, cancel: watch::Receiver<bool>) -> Report {
        self.laboratory_verify_with_timeout(report, cancel, Duration::from_secs(120))
            .await
    }

    pub(super) async fn laboratory_verify_with_timeout(
        &self,
        mut report: Report,
        mut cancel: watch::Receiver<bool>,
        timeout: Duration,
    ) -> Report {
        let start = Instant::now();
        report.stage = "inference".into();
        let thread = report.thread.clone().unwrap_or_default();
        let record = report.record.clone().unwrap_or_default();
        loop {
            report.elapsed_ms = start.elapsed().as_millis() as u64;
            if *cancel.borrow() || start.elapsed() > timeout {
                let _ = self
                    .handle(Request::Stop {
                        thread: thread.clone(),
                    })
                    .await;
                report.outcome = if *cancel.borrow() {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                report.detail = if *cancel.borrow() {
                    "Stopped by the user"
                } else {
                    "No completed reply within two minutes"
                }
                .into();
                break;
            }
            match self.laboratory_data(&record, &thread).await {
                Err(_) => {
                    report.outcome = "failed".into();
                    report.stage = "persistence".into();
                    report.detail = "Cannot read back the test conversation".into();
                    break;
                }
                Ok(data) => {
                    let messages = data["threads"][0]["messages"].as_array();
                    if let Some(messages) = messages {
                        let user = messages.iter().any(|message| message["body"] == HELLO);
                        for message in messages {
                            let uid = message["uid"].as_str().unwrap_or_default();
                            let metadata = store::records::get_extension(
                                &self.engine.store.pool,
                                uid,
                                "lince.message",
                            )
                            .await
                            .ok()
                            .flatten();
                            if let Some(metadata) = metadata
                                && metadata["author"] == record
                            {
                                let state = metadata["state"].as_str().unwrap_or_default();
                                if state == "interrupted" {
                                    report.outcome = "failed".into();
                                    report.detail="Fiote returned an interrupted or failed turn; inspect the retained conversation".into();
                                    break;
                                }
                                if state == "finished"
                                    && !message["body"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .trim()
                                        .is_empty()
                                    && user
                                {
                                    report.reply = message["body"].as_str().unwrap().into();
                                    report.persisted = true;
                                    report.outcome = "passed".into();
                                    report.stage = "persistence".into();
                                    report.detail="Completed assistant reply and user message read back from storage".into();
                                    break;
                                }
                            }
                        }
                    }
                    if report.outcome != "running" {
                        break;
                    }
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(100)) => {},
                result = cancel.changed() => { if result.is_err() {
                    let _ = self.handle(Request::Stop {thread:thread.clone()}).await;
                    report.outcome="cancelled".into(); report.detail="Test controller disconnected".into(); break;
                } }
            }
        }
        report
            .timings
            .push((report.stage.clone(), report.elapsed_ms));
        report
    }
}

pub fn open_browser(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        })
}

pub fn terminal_password() -> Result<Option<Secret>, String> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        terminal,
    };
    use std::io::Write;
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
        }
    }
    eprint!("Provider vault password (Enter to skip): ");
    std::io::stderr()
        .flush()
        .map_err(|_| "Cannot prompt for vault password")?;
    terminal::enable_raw_mode().map_err(|_| "Cannot hide terminal password input")?;
    let _restore = Restore;
    let mut password = Secret::default();
    loop {
        if let Event::Key(key) = event::read().map_err(|_| "Cannot read vault password")? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Enter => {
                    eprintln!("\r");
                    return Ok((!password.0.is_empty()).then_some(password));
                }
                KeyCode::Esc => return Err("Password entry cancelled".into()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err("Password entry cancelled".into());
                }
                KeyCode::Backspace => {
                    password.0.pop();
                }
                KeyCode::Char(value)
                    if !value.is_control() && password.0.len() + value.len_utf8() <= 4096 =>
                {
                    password.0.push(value)
                }
                _ => {}
            }
        }
    }
}
