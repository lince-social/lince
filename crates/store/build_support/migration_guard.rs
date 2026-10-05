use sha2::{Digest, Sha384};
use std::{collections::BTreeSet, fs, path::Path};

pub fn verify(root: &Path) -> Result<(), String> {
    let lock_path = root.join("migrations.sha384");
    let lock = fs::read_to_string(&lock_path)
        .map_err(|error| format!("cannot read {}: {error}", lock_path.display()))?;
    let mut registered = BTreeSet::new();
    let mut previous_version = 0_i64;

    for (index, line) in lock.lines().enumerate() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2
            || fields[0].len() != 96
            || !fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("invalid checksum entry at line {}", index + 1));
        }
        let filename = fields[1];
        if filename.contains(['/', '\\']) || !filename.ends_with(".sql") {
            return Err(format!("invalid migration filename: {filename}"));
        }
        let version = filename
            .split_once('_')
            .and_then(|(version, _)| version.parse::<i64>().ok())
            .filter(|version| *version > previous_version)
            .ok_or_else(|| {
                format!("{filename}: migration numbers must be positive and strictly increasing")
            })?;
        previous_version = version;
        registered.insert(filename.to_owned());
        let path = root.join("migrations").join(filename);
        let bytes = fs::read(&path).map_err(|error| {
            format!("cannot read locked migration {filename}: {error}. Restore the existing file; migrations cannot be deleted or renamed")
        })?;
        let actual: String = Sha384::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if !actual.eq_ignore_ascii_case(fields[0]) {
            return Err(format!(
                "{filename} was modified. Restore its original contents and put changes in a new numbered migration. Do not rewrite its checksum in migrations.sha384"
            ));
        }
    }

    if registered.is_empty() {
        return Err("migrations.sha384 must contain the existing migrations".to_owned());
    }

    let migrations = root.join("migrations");
    for entry in fs::read_dir(&migrations)
        .map_err(|error| format!("cannot read {}: {error}", migrations.display()))?
    {
        let entry = entry.map_err(|error| format!("cannot read migration entry: {error}"))?;
        let filename = entry.file_name();
        let filename = filename
            .to_str()
            .ok_or_else(|| "migration filenames must be UTF-8".to_owned())?;
        if filename.ends_with(".sql") && !registered.contains(filename) {
            return Err(format!(
                "{filename} is not registered. Append new migrations with increasing numbers and their SHA-384 checksums to migrations.sha384; never replace existing entries"
            ));
        }
    }

    Ok(())
}
