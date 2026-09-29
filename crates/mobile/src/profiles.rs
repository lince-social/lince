use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Profiles {
    pub selected: Option<String>,
    pub names: BTreeMap<String, String>,
}

impl Profiles {
    pub fn read(root: &Path) -> Result<Self, String> {
        let path = root.join("mobile-profiles.json");
        let profiles: Self = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 64 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
            }
            Ok(_) => return Err("The saved profile list is too large".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error.to_string()),
        };
        profiles.validate()?;
        Ok(profiles)
    }

    fn validate(&self) -> Result<(), String> {
        if self.names.len() > 32
            || self.names.iter().any(|(id, name)| {
                id.is_empty()
                    || id.len() > 80
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
                    || name.is_empty()
                    || name.chars().count() > 80
            })
            || self
                .selected
                .as_ref()
                .is_some_and(|id| !self.names.contains_key(id))
        {
            return Err("The saved profile list is invalid. Existing data has been kept.".into());
        }
        Ok(())
    }

    pub fn directory(&self, root: &Path) -> PathBuf {
        self.selected
            .as_ref()
            .map_or_else(|| root.to_path_buf(), |id| root.join("profiles").join(id))
    }

    pub fn create(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 || self.names.len() >= 32 {
            return Err(
                "Use a profile name of 1–80 characters. Up to 32 extra profiles are supported."
                    .into(),
            );
        }
        let id = nucleus::new_uid("profile");
        self.names.insert(id.clone(), name.into());
        self.selected = Some(id);
        Ok(())
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        self.validate()?;
        std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        let temporary = root.join(format!(".profiles-{}", nucleus::new_uid("save")));
        let result = (|| -> Result<(), std::io::Error> {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, root.join("mobile-profiles.json"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result.map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_profiles_preserve_original_data_and_reject_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("identity"), b"original").unwrap();
        let mut profiles = Profiles::read(root.path()).unwrap();
        assert_eq!(profiles.directory(root.path()), root.path());
        profiles.create("Phone for another Organ").unwrap();
        profiles.save(root.path()).unwrap();
        let restored = Profiles::read(root.path()).unwrap();
        assert_eq!(
            profiles.directory(root.path()),
            restored.directory(root.path())
        );
        assert!(
            restored
                .directory(root.path())
                .starts_with(root.path().join("profiles"))
        );
        assert_eq!(
            std::fs::read(root.path().join("identity")).unwrap(),
            b"original"
        );
        profiles.names.insert("../escape".into(), "Unsafe".into());
        profiles.selected = Some("../escape".into());
        assert!(profiles.save(root.path()).is_err());
        profiles.selected = None;
        assert_eq!(profiles.directory(root.path()), root.path());
    }
}
