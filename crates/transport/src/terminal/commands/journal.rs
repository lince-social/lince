//! Line-delimited command journals, output paging, and history retention.

use std::{
    fs::{File, OpenOptions, create_dir, read_dir, remove_file, symlink_metadata},
    io::{BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom::Start, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{
    fs::{Permissions, set_permissions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

use serde_json::{from_slice, from_str, to_vec};

use crate::command::{Event, Run};

const MAX_LINE_BYTES: u64 = 1024 * 1024;
const SUMMARY_TAIL_BYTES: u64 = 16 * 1024;
const PAGE_BYTES: u64 = 64 * 1024;
const PAGE_EVENTS: usize = 32;
const COMPLETED_RUNS: usize = 10;

pub(super) fn directory(root: &Path, command: &str) -> Result<PathBuf, String> {
    if !nucleus::valid_uid(command, "r") {
        return Err("Invalid Command identity".into());
    }
    let mut path = root.to_path_buf();
    for part in ["trash", "terminal", command] {
        path.push(part);
        match create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                set_permissions(&path, Permissions::from_mode(0o700))
                    .map_err(|error| error.to_string())?;
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        if !symlink_metadata(&path)
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
    if !symlink_metadata(path)
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
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    append(&mut file, &Event::Started { run: run.clone() })?;
    append(&mut file, &Event::Resize { cols: 80, rows: 24 })?;
    Ok(file)
}

pub(super) fn append(file: &mut File, event: &Event) -> Result<(), String> {
    let mut bytes = to_vec(event).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    file.write_all(&bytes)
        .map_err(|error| format!("Cannot save command output: {error}"))
}

fn read_line(reader: &mut BufReader<File>, line: &mut String) -> Result<u64, String> {
    line.clear();
    let mut bounded = reader.take(MAX_LINE_BYTES);
    bounded.read_line(line).map_err(|error| error.to_string())?;
    Ok(MAX_LINE_BYTES - bounded.limit())
}

pub(super) fn summary(path: &Path) -> Result<Run, String> {
    let file = open(path)?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    read_line(&mut reader, &mut line)?;
    let Event::Started { mut run } = from_str(&line).map_err(|error| error.to_string())? else {
        return Err("Invalid run header".into());
    };
    reader
        .seek(Start(length.saturating_sub(SUMMARY_TAIL_BYTES)))
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
        .find_map(|line| from_slice::<Event>(line).ok())
    {
        run.finished_ms = Some(finished_ms);
        run.exit_code = exit_code;
        run.error = error;
    }
    Ok(run)
}

pub(super) fn history(
    directory: &Path,
    command: &str,
    is_active: impl Fn(&str) -> bool,
) -> Result<Vec<Run>, String> {
    let mut runs = Vec::new();
    for entry in read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path
            .extension()
            .is_none_or(|extension| extension != "jsonl")
        {
            continue;
        }
        let mut run = summary(&path)?;
        if run.command != command {
            return Err("Run belongs to a different Command".into());
        }
        if run.finished_ms.is_none() && !is_active(&run.id) {
            run.finished_ms = Some(run.started_ms);
            run.error = Some("Interrupted · no saved exit status".into());
        }
        runs.push(run);
    }
    runs.sort_by(|a, b| (b.started_ms, &b.id).cmp(&(a.started_ms, &a.id)));
    Ok(runs)
}

pub(super) fn prune_history(root: &Path, command: &str, runs: &[Run]) -> Result<(), String> {
    for run in runs
        .iter()
        .filter(|run| run.finished_ms.is_some())
        .skip(COMPLETED_RUNS)
    {
        match remove_file(path(root, command, &run.id)?) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(super) fn read(path: &Path, offset: u64) -> Result<(Vec<Event>, u64), String> {
    let mut file = open(path)?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    if offset > length {
        return Err("Output position exceeds the saved run".into());
    }
    if offset > 0 {
        file.seek(Start(offset - 1))
            .map_err(|error| error.to_string())?;
        let mut byte = [0];
        file.read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        if byte != *b"\n" {
            return Err("Output position is not an event boundary".into());
        }
    }
    // A new file starts at zero. Reading the boundary byte above advances to offset.
    let mut reader = BufReader::new(file);
    let mut next = offset;
    let mut events = Vec::new();
    let mut line = String::new();
    while next - offset < PAGE_BYTES && events.len() < PAGE_EVENTS {
        let size = read_line(&mut reader, &mut line)?;
        if !line.ends_with('\n') {
            break;
        }
        let event = from_str(&line).map_err(|error| format!("Invalid run event: {error}"))?;
        let output = matches!(event, Event::Output { .. });
        events.push(event);
        next += size;
        if output {
            break;
        }
    }
    Ok((events, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_position_waits_for_a_complete_event() {
        let directory = tempfile::tempdir().expect("journal directory");
        let path = directory.path().join("run.jsonl");
        let mut file = File::create(&path).expect("journal file");
        append(&mut file, &Event::Resize { cols: 80, rows: 24 }).expect("first event");
        let boundary = file.metadata().expect("journal length").len();
        file.write_all(br#"{"event":"resize","cols":"#)
            .expect("partial event");

        let (events, next) = read(&path, 0).expect("read the completed event");
        assert!(matches!(
            events.as_slice(),
            [Event::Resize { cols: 80, rows: 24 }]
        ));
        assert_eq!(next, boundary);
        let (events, next) = read(&path, boundary).expect("read the incomplete tail");
        assert!(events.is_empty());
        assert_eq!(next, boundary);

        file.write_all(b"4,\"rows\":5}\n")
            .expect("complete the event");
        let (events, next) = read(&path, boundary).expect("resume at the boundary");
        assert!(matches!(
            events.as_slice(),
            [Event::Resize { cols: 4, rows: 5 }]
        ));
        assert_eq!(next, file.metadata().expect("new journal length").len());
        assert_eq!(
            read(&path, boundary + 1).expect_err("offset is inside an event"),
            "Output position is not an event boundary"
        );
    }
}
