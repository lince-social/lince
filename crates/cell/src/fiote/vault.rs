use super::*;
use std::collections::BTreeMap;

pub const NAMESPACE: &str = "lince.vault.providers";
const SLUG: &str = "fiote-vault";

struct Opened {
    password: Secret,
    envelope: String,
    entries: BTreeMap<String, Secret>,
}

pub struct Vault {
    engine: Arc<Engine>,
    opened: Mutex<Option<Opened>>,
}

impl Vault {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            opened: Mutex::new(None),
        }
    }

    async fn stored(&self) -> Result<Option<(String, String)>, String> {
        let Some(row) = store::records::resolve(&self.engine.store.pool, SLUG)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let value = store::records::get_extension(&self.engine.store.pool, &row.uid, NAMESPACE)
            .await
            .map_err(|e| e.to_string())?;
        match value {
            None => Ok(Some((row.uid, String::new()))),
            Some(value) => Ok(Some((
                row.uid,
                value["ciphertext"]
                    .as_str()
                    .ok_or("The provider vault is damaged.")?
                    .into(),
            ))),
        }
    }

    pub async fn status(&self) -> Result<(bool, bool), String> {
        let mut opened = self.opened.lock().await;
        let stored = self.stored().await?;
        if opened.as_ref().is_some_and(|open| {
            stored
                .as_ref()
                .map(|(_, envelope)| envelope.as_str())
                .unwrap_or("")
                != open.envelope
        }) {
            *opened = None;
        }
        Ok((
            stored.is_some_and(|(_, envelope)| !envelope.is_empty()),
            opened.is_none(),
        ))
    }

    pub async fn lock(&self) {
        *self.opened.lock().await = None;
    }

    pub async fn unlock(&self, password: Secret) -> Result<(), String> {
        if password.0.is_empty() || password.0.len() > 4096 {
            return Err("Enter a vault password of at most 4096 bytes.".into());
        }
        let mut opened = self.opened.lock().await;
        let stored = self.stored().await?;
        let (envelope, entries) =
            if let Some((uid, envelope)) = stored.filter(|(_, value)| !value.is_empty()) {
                let encrypted = envelope.clone();
                let key = password.clone();
                let plaintext = tokio::task::spawn_blocking(move || {
                    utils::vault::unlock(&uid, &key.0, &encrypted).map(Secret)
                })
                .await
                .map_err(|_| "Vault worker stopped.")?
                .map_err(|_| "The vault did not open. Check your password.")?;
                let entries = serde_json::from_str(&plaintext.0)
                    .map_err(|_| "The provider vault is damaged.")?;
                (envelope, entries)
            } else {
                (String::new(), BTreeMap::new())
            };
        *opened = Some(Opened {
            password,
            envelope,
            entries,
        });
        Ok(())
    }

    pub async fn key(&self, slot: &str) -> Result<Option<Secret>, String> {
        self.status().await?;
        let opened = self.opened.lock().await;
        let open = opened
            .as_ref()
            .ok_or("Unlock the provider vault with /login before sending a message.")?;
        Ok(open.entries.get(slot).cloned())
    }

    pub async fn put(&self, slot: String, secret: Secret) -> Result<(), String> {
        self.write(slot, secret, None).await
    }

    pub async fn refreshed(
        &self,
        slot: String,
        secret: Secret,
        previous: Secret,
    ) -> Result<(), String> {
        if secret.0 == previous.0 {
            return Ok(());
        }
        self.write(slot, secret, Some(previous)).await
    }

    async fn write(
        &self,
        slot: String,
        secret: Secret,
        previous: Option<Secret>,
    ) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                let mut opened = self.opened.lock().await;
                let open = opened.as_mut().ok_or_else(|| {
                    engine::EngineError::Consequence("Unlock the provider vault first.".into())
                })?;
                if previous.as_ref().is_some_and(|previous| {
                    open.entries
                        .get(&slot)
                        .is_none_or(|current| current.0 != previous.0)
                }) {
                    return Ok(());
                }
                let stored = self
                    .stored()
                    .await
                    .map_err(engine::EngineError::Consequence)?;
                if stored
                    .as_ref()
                    .map(|(_, value)| value.as_str())
                    .unwrap_or("")
                    != open.envelope
                {
                    return Err(engine::EngineError::Consequence(
                        "The vault changed. Unlock it again before saving.".into(),
                    ));
                }
                let uid = match stored {
                    Some((uid, _)) => uid,
                    None => self
                        .engine
                        .act(
                            Action::CreateRecord {
                                slug: Some(SLUG.into()),
                                kind: RecordKind::Plain,
                                head: "Fiote provider vault".into(),
                                body: String::new(),
                                quantity: 0.0,
                            },
                            None,
                        )
                        .await?
                        .created
                        .ok_or_else(|| {
                            engine::EngineError::Consequence("Could not create the vault.".into())
                        })?,
                };
                let mut entries = open.entries.clone();
                entries.insert(slot, secret);
                let plaintext = Secret(
                    serde_json::to_string(&entries)
                        .map_err(|e| engine::EngineError::Consequence(e.to_string()))?,
                );
                let key = open.password.clone();
                let target = uid.clone();
                let envelope = tokio::task::spawn_blocking(move || {
                    utils::vault::lock(&target, &key.0, &plaintext.0)
                })
                .await
                .map_err(|_| engine::EngineError::Consequence("Vault worker stopped.".into()))?
                .map_err(|e| engine::EngineError::Consequence(e.to_string()))?;
                self.engine
                    .act(
                        Action::SetExtension {
                            target: uid,
                            namespace: NAMESPACE.into(),
                            fds: serde_json::json!({"ciphertext":envelope}),
                        },
                        None,
                    )
                    .await?;
                open.envelope = envelope;
                open.entries = entries;
                Ok(())
            })
            .await
            .map_err(|e| e.to_string())
    }
}

pub fn slot(settings: &Settings) -> String {
    format!(
        "{}:{}:{}",
        settings.provider.0, settings.auth_method, settings.endpoint
    )
}
