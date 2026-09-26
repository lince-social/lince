use crate::command::{Event, Run};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub(super) const OUTPUT_LIMIT: u64 = 256 * 1024 * 1024;

pub(super) fn directory(root: &Path, command: &str) -> Result<PathBuf, String> {
    if !nucleus::valid_uid(command, "r") {
        return Err("Invalid Command identity".into());
    }
    let mut path = root.to_path_buf();
    for part in ["trash", "terminal", command] {
        path.push(part);
        match std::fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        if !std::fs::symlink_metadata(&path)
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            return Err("Command history directory must not be a symlink".into());
        }
    }
    Ok(path)
}

pub(super) fn path(root: &Path, command: &str, run: &str) -> Result<PathBuf, String> {
    if !nucleus::valid_uid(run, "run") {
        return Err("Invalid run identity".into());
    }
    Ok(directory(root, command)?.join(format!("{run}.jsonl")))
}

pub(super) fn open(path: &Path) -> Result<File, String> {
    if !std::fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Run file must be a regular file".into());
    }
    File::open(path).map_err(|error| error.to_string())
}

pub(super) fn create(path: &Path, run: &Run) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    append(&mut file, &Event::Started { run: run.clone() })?;
    append(&mut file, &Event::Resize { cols: 80, rows: 24 })?;
    Ok(file)
}

pub(super) fn append(file: &mut File, event: &Event) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(event).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    file.write_all(&bytes)
        .map_err(|error| format!("Cannot save command output: {error}"))
}

pub(super) fn summary(path: &Path) -> Result<Run, String> {
    let file = open(path)?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .by_ref()
        .take(1024 * 1024)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let Event::Started { mut run } =
        serde_json::from_str(&line).map_err(|error| error.to_string())?
    else {
        return Err("Invalid run header".into());
    };
    reader
        .seek(SeekFrom::Start(length.saturating_sub(16384)))
        .map_err(|error| error.to_string())?;
    let mut tail = Vec::new();
    reader
        .read_to_end(&mut tail)
        .map_err(|error| error.to_string())?;
    if let Some(Event::Finished {
        finished_ms,
        exit_code,
        error,
    }) = tail
        .split(|byte| *byte == b'\n')
        .rev()
        .find_map(|line| serde_json::from_slice::<Event>(line).ok())
    {
        run.finished_ms = Some(finished_ms);
        run.exit_code = exit_code;
        run.error = error;
    }
    Ok(run)
}

pub(super) fn read(path: &Path, offset: u64) -> Result<(Vec<Event>, u64), String> {
    let mut file = open(path)?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    if offset > length {
        return Err("Output position exceeds the saved run".into());
    }
    if offset > 0 {
        file.seek(SeekFrom::Start(offset - 1))
            .map_err(|error| error.to_string())?;
        let mut byte = [0];
        file.read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        if byte[0] != b'\n' {
            return Err("Output position is not an event boundary".into());
        }
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut next = offset;
    let mut events = Vec::new();
    while next - offset < 64 * 1024 && events.len() < 32 {
        let mut line = String::new();
        let size = reader
            .by_ref()
            .take(1024 * 1024)
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if size == 0 || !line.ends_with('\n') {
            break;
        }
        let event =
            serde_json::from_str(&line).map_err(|error| format!("Invalid run event: {error}"))?;
        let output = matches!(event, Event::Output { .. });
        events.push(event);
        next += size as u64;
        if output {
            break;
        }
    }
    Ok((events, next))
}
