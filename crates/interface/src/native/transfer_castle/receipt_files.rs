use super::*;
use model::FieldKind;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn read(path: &Path) -> Result<String, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = options.open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Choose a regular receipt document file".into());
    }
    let mut bytes = Vec::new();
    file.take((nucleus::transfer::receipt::MAX_BUNDLE_BYTES + 65) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    nucleus::transfer::receipt::Document::parse(&bytes, nucleus::execution::now().timestamp())?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

fn write(path: &Path, raw: &str, expected: &Value) -> Result<(), String> {
    if serde_json::from_str::<Value>(raw).map_err(|error| error.to_string())? != *expected {
        return Err(
            "The receipt changed. Reopen file export and review its current document.".into(),
        );
    }
    let document = nucleus::transfer::receipt::Document::parse(
        raw.as_bytes(),
        nucleus::execution::now().timestamp(),
    )?;
    if document.expires_at() <= nucleus::execution::now().timestamp() {
        return Err(
            "The sharing term expired before saving. Review new terms before exporting.".into(),
        );
    }
    let save = || -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(raw.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    };
    save().map_err(|error| error.to_string())
}

fn current(world: &World, owner: Entity, person_uid: &str, job: &str) -> bool {
    world
        .get::<View>(owner)
        .is_some_and(|view| view.receipt_file_job.as_deref() == Some(job))
        && person(world, owner) == person_uid
}

pub(super) fn choose(world: &mut World, owner: Entity, export: bool) {
    let person_uid = person(world, owner);
    if person_uid.is_empty() || world.get::<View>(owner).unwrap().receipt_file_job.is_some() {
        return;
    }
    let receipt = world.get::<View>(owner).unwrap().receipt.clone();
    if export
        && receipt
            .as_ref()
            .is_none_or(|receipt| receipt["record"].as_str().is_none())
    {
        return;
    }
    let job = nucleus::new_uid("r");
    world.get_mut::<View>(owner).unwrap().receipt_file_job = Some(job.clone());
    let result = crate::native::file_explorer::worker::run(
        world,
        move || {
            let dialog = rfd::FileDialog::new().add_filter("Lince receipt JSON", &["json"]);
            if export {
                dialog
                    .set_title("Save signed receipt")
                    .set_file_name("transfer-receipt.json")
                    .save_file()
                    .map(|path| Ok((path, None)))
                    .transpose()
            } else {
                dialog
                    .set_title("Open signed receipt")
                    .pick_file()
                    .map(|path| read(&path).map(|raw| (path, Some(raw))))
                    .transpose()
            }
        },
        move |world, result: Result<Option<(PathBuf, Option<String>)>, String>| {
            if !current(world, owner, &person_uid, &job) {
                return;
            }
            world.get_mut::<View>(owner).unwrap().receipt_file_job = None;
            match result {
                Ok(Some((path, raw))) => {
                    let Some(path) = path.to_str() else {
                        status(
                            world,
                            owner,
                            "Choose a file path that can be represented as text",
                        );
                        return;
                    };
                    let mut form = if let Some(raw) = raw {
                        let mut form = receipts::action_form(
                            &person_uid,
                            "Review receipt file import",
                            json!({"kind":"import","document":raw}),
                        );
                        form.field("/request/document", "Signed document", FieldKind::Text);
                        form.review["Verification"] = json!(
                            "Opening a file does not import evidence. Import verifies its signatures and recognized identities; the document cannot perform Actions."
                        );
                        form
                    } else {
                        let receipt = receipt.unwrap();
                        let request=if receipt["historical"]==true {
                            json!({"kind":"export_version","record":receipt["record"],"version":receipt["version"]})
                        } else {json!({"kind":"export","record":receipt["record"]})};
                        let mut form = receipts::action_form(
                            &person_uid,
                            "Review receipt file export",
                            request,
                        );
                        form.review["Reviewed document"] = receipt["document"].clone();
                        form.mode = "receipt-file-export".into();
                        form
                    };
                    form.review["File"] = json!(path);
                    ui::Command::Open(form).apply(world, owner);
                }
                Ok(None) => status(world, owner, "Receipt file selection cancelled"),
                Err(error) => status(world, owner, error),
            }
        },
    );
    if let Err(error) = result {
        world.get_mut::<View>(owner).unwrap().receipt_file_job = None;
        status(world, owner, error);
    }
}

pub(super) fn complete(world: &mut World, owner: Entity, form: &Form, data: Option<&Value>) {
    if form.mode != "receipt-file-export" {
        return;
    }
    let Some(raw) = data
        .and_then(|data| data["receipt_export"].as_str())
        .map(str::to_owned)
    else {
        return;
    };
    let Some(path) = form.review["File"].as_str().map(PathBuf::from) else {
        return;
    };
    let expected = form.review["Reviewed document"].clone();
    let person_uid = person(world, owner);
    let result = crate::native::file_explorer::worker::run(
        world,
        move || write(&path, &raw, &expected),
        move |world, result| {
            if world.get::<View>(owner).is_none() || person(world, owner) != person_uid {
                return;
            }
            status(
                world,
                owner,
                match result {
                    Ok(()) => "Signed receipt file saved".into(),
                    Err(error) => format!("Receipt export failed: {error}"),
                },
            );
        },
    );
    if let Err(error) = result {
        status(world, owner, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(expires_at: i64) -> nucleus::transfer::receipt::Document {
        nucleus::transfer::receipt::Document::Authority(nucleus::transfer::receipt::Authority {
            organ: nucleus::new_uid("r"),
            roster_version: 1,
            root_key: "a".repeat(44),
            signing_key: "b".repeat(44),
            key_id: "test".into(),
            issued_at: nucleus::execution::now().timestamp() - 60,
            expires_at,
            successions: vec![],
            participation: None,
            signature: "a".repeat(88),
        })
    }

    #[test]
    fn file_input_rejects_actions_oversized_documents_and_non_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(read(directory.path()).is_err());
        let path = directory.path().join("receipt.json");
        std::fs::write(&path, br#"{"action":"create-record","quantity":1000}"#).unwrap();
        assert!(read(&path).is_err());
        std::fs::write(
            &path,
            vec![b' '; nucleus::transfer::receipt::MAX_BUNDLE_BYTES + 65],
        )
        .unwrap();
        assert!(read(&path).is_err());
        assert!(write(&path, "{}", &json!({})).is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            (nucleus::transfer::receipt::MAX_BUNDLE_BYTES + 65) as u64
        );
    }

    #[test]
    fn file_export_preserves_original_bytes_and_refuses_changed_or_expired_reviews() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt.json");
        let current_document = document(nucleus::execution::now().timestamp() + 3600);
        let value = serde_json::to_value(&current_document).unwrap();
        let raw = serde_json::to_string_pretty(&current_document).unwrap();
        std::fs::write(&path, b"previous document").unwrap();
        write(&path, &raw, &value).unwrap();
        assert_eq!(read(&path).unwrap(), raw);
        let mut altered = value.clone();
        altered["value"]["key_id"] = json!("other-key");
        assert!(
            write(&path, &raw, &altered)
                .unwrap_err()
                .contains("changed")
        );
        let expired = document(nucleus::execution::now().timestamp() - 1);
        let expired_raw = serde_json::to_string(&expired).unwrap();
        assert!(
            write(&path, &expired_raw, &serde_json::to_value(expired).unwrap())
                .unwrap_err()
                .contains("expired")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    }
}
