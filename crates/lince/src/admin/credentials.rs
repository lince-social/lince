mod connect;

use cell::FioteSecret as Secret;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
};

pub(super) async fn dispatch(arguments: &[&str]) -> io::Result<()> {
    let directory = utils::config::lince_data_dir()
        .ok_or_else(|| io::Error::other("Cannot locate Lince data"))?;
    if let ["connect-chatgpt", record] = arguments {
        return connect::chatgpt(&directory, record).await;
    }
    let ["import-legacy", password_file] = arguments else {
        return Err(io::Error::other(
            "Use lince credentials connect-chatgpt RECORD_UID to reconnect without a Lince password, or lince credentials import-legacy PROTECTED_PASSWORD_FILE to recover old encrypted credentials.",
        ));
    };
    let (imported, retained) = import_legacy(&directory, Path::new(password_file)).await?;
    println!(
        "Imported {imported} credentials; retained {retained} newer local credentials. The original encrypted Record was preserved. No unlock is needed on subsequent starts."
    );
    Ok(())
}

async fn import_legacy(directory: &Path, password_file: &Path) -> io::Result<(usize, usize)> {
    let store = store::Store::open_readonly(&directory.join("lince.db"))
        .await
        .map_err(io::Error::other)?;
    let record = store::records::resolve(&store.pool, "fiote-vault")
        .await
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("No old provider credential Record exists"))?;
    let envelope = store::records::get_extension(&store.pool, &record.uid, "lince.vault.providers")
        .await
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("No old encrypted provider credentials exist"))?;
    let password = password_file_read(password_file)?;
    let clear = Secret(
        utils::vault::unlock(
            &record.uid,
            &password.0,
            envelope["ciphertext"]
                .as_str()
                .ok_or_else(|| io::Error::other("The old credential envelope is damaged"))?,
        )
        .map_err(|_| {
            io::Error::other("The old credentials did not open. Check the migration password file.")
        })?,
    );
    let entries: BTreeMap<String, Secret> = serde_json::from_str(&clear.0)
        .map_err(|_| io::Error::other("The old credential contents are damaged"))?;
    store.pool.close().await;
    let credentials = cell::fiote_credentials::Store::new(directory.join("fiote/credentials"));
    let (mut imported, mut retained) = (0, 0);
    for (slot, secret) in entries {
        if credentials
            .import_if_missing(slot, secret)
            .await
            .map_err(io::Error::other)?
        {
            imported += 1;
        } else {
            retained += 1;
        }
    }
    Ok((imported, retained))
}

fn password_file_read(path: &Path) -> io::Result<Secret> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(io::Error::other(
            "Use a regular migration password file of at most 4096 bytes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "Migration password file must be private (chmod 600)",
            ));
        }
    }
    let mut password = Secret::default();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_string(&mut password.0)?;
    if password.0.is_empty() || password.0.len() > 4096 {
        return Err(io::Error::other("Invalid migration password file size"));
    }
    password
        .0
        .truncate(password.0.trim_end_matches(['\r', '\n']).len());
    Ok(password)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn migration_password_file_must_be_private_and_preserves_spaces() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("password");
        std::fs::write(&path, " password with spaces \n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(password_file_read(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            password_file_read(&path).unwrap().0,
            " password with spaces "
        );
    }
    #[tokio::test]
    async fn imports_old_credentials_without_overwriting_new_logins_or_changing_the_old_record() {
        use engine::actions::Action;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("lince.db");
        let engine = engine::Engine::open(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: Some("fiote-vault".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Old encrypted credentials".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let ciphertext = utils::vault::lock(
            &uid,
            "old password",
            r#"{"account":"old login","other":"recover me"}"#,
        )
        .unwrap();
        engine
            .act(
                Action::SetExtension {
                    target: uid.clone(),
                    namespace: "lince.vault.providers".into(),
                    fds: serde_json::json!({"ciphertext":ciphertext}),
                },
                None,
            )
            .await
            .unwrap();
        let password = directory.path().join("migration-password");
        std::fs::write(&password, "old password\n").unwrap();
        std::fs::set_permissions(&password, std::fs::Permissions::from_mode(0o600)).unwrap();
        let credentials =
            cell::fiote_credentials::Store::new(directory.path().join("fiote/credentials"));
        credentials
            .put("account".into(), Secret("new login".into()))
            .await
            .unwrap();
        assert_eq!(
            import_legacy(directory.path(), &password).await.unwrap(),
            (1, 1)
        );
        assert_eq!(
            credentials.key("account").await.unwrap().unwrap().0,
            "new login"
        );
        assert_eq!(
            credentials.key("other").await.unwrap().unwrap().0,
            "recover me"
        );
        assert_eq!(
            import_legacy(directory.path(), &password).await.unwrap(),
            (0, 2)
        );
        std::fs::write(&password, "wrong password").unwrap();
        assert!(import_legacy(directory.path(), &password).await.is_err());
        assert_eq!(
            store::records::get_extension(&engine.store.pool, &uid, "lince.vault.providers")
                .await
                .unwrap()
                .unwrap()["ciphertext"],
            ciphertext
        );
        engine.store.pool.close().await;
    }
}
