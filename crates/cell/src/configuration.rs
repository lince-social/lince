use crate::CellRuntime;
use serde_json::Value;
use std::io;

#[derive(Clone, Debug)]
pub struct Contact {
    pub uid: String,
    pub name: String,
    pub trust: String,
    pub proximity: u32,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    pub organ_uid: String,
    pub name: String,
    pub address: String,
    pub discovery: Value,
    pub contacts: Vec<Contact>,
    pub storage: Storage,
}

#[derive(Clone, Debug)]
pub struct Storage {
    pub budget_bytes: i64,
    pub database_bytes: i64,
    pub quarantine_bytes: i64,
    pub on_disk_bytes: Option<u64>,
}

impl CellRuntime {
    pub async fn configuration(&self) -> io::Result<Configuration> {
        let organ = store::organs::local(&self.store.pool)
            .await
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("The local Organ is missing"))?;
        let discovery = crate::discovery::config(&self.store, &organ.uid)
            .await?
            .unwrap_or_else(|| serde_json::json!({}));
        let contacts = store::organs::contacts(&self.store.pool)
            .await
            .map_err(io::Error::other)?
            .into_iter()
            .map(|contact| Contact {
                uid: contact.record_uid,
                name: contact.head,
                trust: contact.trust,
                proximity: contact.proximity,
            })
            .collect();
        Ok(Configuration {
            organ_uid: organ.uid,
            name: organ.head,
            address: organ.body,
            discovery,
            contacts,
            storage: self.storage_usage().await?,
        })
    }

    pub async fn storage_usage(&self) -> io::Result<Storage> {
        let budget_bytes = store::budget::total(&self.store.pool)
            .await
            .map_err(io::Error::other)?;
        let database_bytes = store::budget::database_bytes(&self.store.pool)
            .await
            .map_err(io::Error::other)?;
        let quarantine_bytes = store::budget::quarantine_bytes(&self.store.pool)
            .await
            .map_err(io::Error::other)?;
        let directory = self
            .information
            .as_ref()
            .map(|channel| channel.state.borrow().directory.clone());
        let on_disk_bytes = match directory {
            Some(directory) => Some(
                tokio::task::spawn_blocking(move || directory_bytes(&directory))
                    .await
                    .map_err(io::Error::other)??,
            ),
            None => None,
        };
        Ok(Storage {
            budget_bytes,
            database_bytes,
            quarantine_bytes,
            on_disk_bytes,
        })
    }

    pub async fn set_storage_budget(&self, bytes: i64) -> io::Result<Storage> {
        store::budget::set_total(&self.store.pool, bytes)
            .await
            .map_err(io::Error::other)?;
        self.engine.notify_config_changed();
        self.storage_usage().await
    }
}

fn directory_bytes(root: &std::path::Path) -> io::Result<u64> {
    let mut directories = vec![root.to_path_buf()];
    let mut bytes = 0u64;
    let mut entries = 0usize;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            entries += 1;
            if entries > 1_000_000 {
                return Err(io::Error::other(
                    "Storage directory has too many entries to measure",
                ));
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() {
                bytes = bytes.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_measurement_counts_files_without_following_links() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        std::fs::write(directory.path().join("a"), b"abc").unwrap();
        std::fs::write(directory.path().join("nested/b"), b"abcd").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(directory.path(), directory.path().join("nested/loop")).unwrap();
        assert_eq!(directory_bytes(directory.path()).unwrap(), 7);
    }
}
