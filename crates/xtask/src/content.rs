use crate::Result;
use std::{ffi::OsString, fs, io, path::Path};

pub fn package(repository: &Path, caller: &Path, arguments: &[OsString]) -> Result<()> {
    if !(1..=2).contains(&arguments.len()) {
        return Err("Usage: cargo xtask content DESTINATION.zip [EXECUTABLE]".into());
    }
    let destination = caller.join(&arguments[0]);
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| error.to_string())?;
    let mut archive = zip::ZipWriter::new(file);
    let root = repository.join("institute/trails");
    let mut pending = vec![root.clone()];
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(directory)
            .map_err(|error| error.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                let path = entry.path();
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|error| error.to_string())?;
                let name = format!("content/{}", relative.to_string_lossy().replace('\\', "/"));
                archive
                    .start_file(name, options)
                    .map_err(|error| error.to_string())?;
                io::copy(
                    &mut fs::File::open(path).map_err(|error| error.to_string())?,
                    &mut archive,
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    if let Some(executable) = arguments.get(1) {
        archive
            .start_file("bin/lince", options.unix_permissions(0o755))
            .map_err(|error| error.to_string())?;
        io::copy(
            &mut fs::File::open(caller.join(executable)).map_err(|error| error.to_string())?,
            &mut archive,
        )
        .map_err(|error| error.to_string())?;
    }
    for (path, name) in [(repository.join("LICENSE"), "LICENSE")] {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        io::copy(
            &mut fs::File::open(path).map_err(|error| error.to_string())?,
            &mut archive,
        )
        .map_err(|error| error.to_string())?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    println!("{}", destination.display());
    Ok(())
}
