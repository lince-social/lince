use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    match command.as_str() {
        "check" => {
            let root = arguments
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("anicca"));
            no_more(arguments)?;
            match anicca::check_project(&root) {
                Ok(paths) => {
                    let mut records = 0;
                    let mut frequencies = 0;
                    let mut rules = 0;
                    for path in &paths {
                        let document =
                            anicca::parse_path(path).map_err(|error| error.to_string())?;
                        for declaration in document.declarations {
                            match declaration {
                                anicca::grammar::grammar::Declaration::Record(_) => records += 1,
                                anicca::grammar::grammar::Declaration::Frequency(_) => {
                                    frequencies += 1
                                }
                                anicca::grammar::grammar::Declaration::Karma(karma) => {
                                    rules += karma.rules.rules.len()
                                }
                            }
                        }
                    }
                    println!(
                        "checked {} .lingua files: {records} Records, {frequencies} Frequencies, {rules} Rules",
                        paths.len()
                    );
                    Ok(())
                }
                Err(errors) => Err(errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")),
            }
        }
        "fmt" => {
            let mut write = false;
            let mut target = None;
            for argument in arguments {
                if argument == "--write" {
                    write = true;
                } else if target.replace(PathBuf::from(argument)).is_some() {
                    return Err(usage());
                }
            }
            let target = target.unwrap_or_else(|| PathBuf::from("anicca"));
            format_target(&target, write)
        }
        _ => Err(usage()),
    }
}

fn no_more(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    if arguments.next().is_some() {
        Err(usage())
    } else {
        Ok(())
    }
}

fn format_target(target: &Path, write: bool) -> Result<(), String> {
    let paths = if target.is_dir() {
        anicca::lingua_paths(target).map_err(|error| error.to_string())?
    } else {
        vec![target.to_path_buf()]
    };
    let mut changed = Vec::new();
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let canonical = anicca::canonicalize(&source)
            .map_err(|error| format!("{}: {}", path.display(), error.message))?;
        if source != canonical {
            changed.push(path.clone());
            if write {
                atomic_write(&path, canonical.as_bytes())?;
            }
        }
    }
    if !write && !changed.is_empty() {
        return Err(changed
            .into_iter()
            .map(|path| format!("{} is not canonically formatted", path.display()))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    println!(
        "{} file(s) {}",
        changed.len(),
        if write {
            "formatted"
        } else {
            "need formatting"
        }
    );
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    let temporary = parent.join(format!(
        ".{}.anicca-pending",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
    ));
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}

fn usage() -> String {
    "usage: lingua check [DIR] | lingua fmt [--write] [FILE_OR_DIR]".to_string()
}
