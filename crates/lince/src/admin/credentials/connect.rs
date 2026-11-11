use cell::{FioteSettings, fiote_communication::native, fiote_credentials};
use serde_json::Value;
use std::{io, path::Path};

pub(super) async fn chatgpt(directory: &Path, record: &str) -> io::Result<()> {
    if !nucleus::valid_uid(record, "r") {
        return Err(io::Error::other("Use the configured Fiote's Record UID"));
    }
    let fiote = directory.join("fiote");
    let path = fiote.join(format!("{record}.json"));
    let original = std::fs::read(&path)?;
    let mut saved: Value = serde_json::from_slice(&original)?;
    let mut settings: FioteSettings = serde_json::from_value(saved["settings"].clone())?;
    settings.validate().map_err(io::Error::other)?;
    if !native::has_native_login(&settings) || !saved["agent"].is_null() {
        return Err(io::Error::other(
            "This Fiote must use native ChatGPT browser sign-in",
        ));
    }
    let credentials = fiote_credentials::Store::new(fiote.join("credentials"));
    let previous = credentials
        .key(&fiote_credentials::slot(&settings))
        .await
        .map_err(io::Error::other)?;
    let login = native::login(&mut settings, &fiote, previous.as_ref())
        .await
        .map_err(io::Error::other)?;
    cell::fiote::laboratory::open_browser(&login.url)?;
    println!("Complete Lince's ChatGPT sign-in in the browser. No Lince password is needed.");
    let key = login.finish().await.map_err(io::Error::other)?;
    if !native::account_ready(&settings, &key).map_err(io::Error::other)? {
        return Err(io::Error::other(
            "ChatGPT plan usage was not granted. Enable it and reconnect.",
        ));
    }
    if std::fs::read(&path)? != original {
        return Err(io::Error::other(
            "Fiote settings changed during sign-in; retry using the current settings",
        ));
    }
    credentials
        .put(fiote_credentials::slot(&settings), key)
        .await
        .map_err(io::Error::other)?;
    saved["settings"] = serde_json::to_value(&settings)?;
    save(&path, &serde_json::to_vec_pretty(&saved)?)?;
    println!(
        "ChatGPT connected. Credentials are private local files and remain available after reboot."
    );
    Ok(())
}

fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(
            path.parent()
                .ok_or_else(|| io::Error::other("Missing settings directory"))?,
        )?
        .sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
