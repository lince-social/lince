use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Default, Serialize, Deserialize)]
pub struct Saved {
    pub organ: String,
    pub navigation: crate::navigation::Navigation,
    pub drafts: BTreeMap<String, String>,
    pub documents: BTreeMap<String, String>,
    pub karma: Option<lince_interface::karma::Draft>,
    pub frequency: Option<lince_interface::frequency::Draft>,
    pub search: String,
    pub sort: usize,
    #[serde(default)]
    pub negative_only: bool,
    #[serde(default)]
    pub view: Option<lince_interface::queries::ProteinDraft>,
    #[serde(default)]
    pub attachments: BTreeMap<String, Vec<nucleus::message::MessagePart>>,
    pub outbox: BTreeMap<String, crate::record::Prepared>,
}

pub fn read(directory: &Path, organ: &str) -> Result<Option<Saved>, std::io::Error> {
    let file = match std::fs::File::open(directory.join("mobile-drafts.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(std::io::Error::other("Saved mobile drafts are too large"));
    }
    let saved: Saved = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    Ok((saved.organ == organ).then_some(saved))
}

pub fn write(directory: &Path, state: &Saved) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec(state).map_err(std::io::Error::other)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(std::io::Error::other(
            "Mobile drafts exceed 16 MB; finish or discard some edits",
        ));
    }
    std::fs::create_dir_all(directory)?;
    let path = directory.join("mobile-drafts.json");
    let temporary = directory.join(format!(".mobile-drafts-{}.json", nucleus::new_uid("write")));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_drafts_preserve_the_last_successful_save() {
        let directory = tempfile::tempdir().unwrap();
        let mut saved = Saved {
            organ: "test-organ".into(),
            ..Default::default()
        };
        saved
            .drafts
            .insert("r/body".into(), "Saved before storage failed".into());
        write(directory.path(), &saved).unwrap();
        saved
            .drafts
            .insert("r/body".into(), "x".repeat(MAX_BYTES as usize));
        assert!(write(directory.path(), &saved).is_err());
        let recovered = read(directory.path(), "test-organ").unwrap().unwrap();
        assert_eq!(recovered.drafts["r/body"], "Saved before storage failed");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
