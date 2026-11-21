// non-lean-yet
// Scope: struct SourceServer, fn serve.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 167, 169.
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 25, 91.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 29, 55, 74, 75.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 162, 170.
// - [Clippy: clippy::cast_possible_truncation] Prove the bounds or use a checked conversion. Sites
//   162.
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites 47,
//   73, 76, 77.
// - [Clippy: clippy::manual_let_else] this could be rewritten as `let...else`. Sites 62.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 25.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 12.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 12, 13,
//   14, 25.

use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{Library, RangeReader, Resource, Result, fingerprint};

pub struct SourceServer {
    pub uri: String,
    pub version: String,
    stop: Arc<AtomicBool>,
}

impl Drop for SourceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl SourceServer {
    pub fn start(library: Library, resource: &Resource) -> Result<Self> {
        if crate::address(&resource.uri)?.scheme() == "file" || library.find(resource)?.is_some() {
            return Err("Use the local media file directly".into());
        }
        let reader = RangeReader::open(library.clone(), resource)?;
        if library.find(resource)?.is_some() {
            return Err("Use the retained complete source directly".into());
        }
        let length = reader.len();
        let version = reader.version();
        let reader = Arc::new(Mutex::new(reader));
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let token =
            fingerprint(format!("{}:{:?}", resource.uri, std::time::SystemTime::now()).as_bytes());
        let uri = format!(
            "http://{}/{token}",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let media_type = if resource
            .media_type
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n')
        {
            "application/octet-stream".into()
        } else {
            resource.media_type.clone()
        };
        std::thread::Builder::new()
            .name("shared-source-http".into())
            .spawn(move || {
                let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                while !stopped.load(Ordering::Relaxed) {
                    let (stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                    };
                    if active.load(Ordering::Relaxed) >= 4 {
                        continue;
                    }
                    active.fetch_add(1, Ordering::Relaxed);
                    let readers = reader.clone();
                    let token = token.clone();
                    let media_type = media_type.clone();
                    let stop = stopped.clone();
                    let active = active.clone();
                    std::thread::spawn(move || {
                        let _ = serve(stream, readers, &token, &media_type, length, &stop);
                        active.fetch_sub(1, Ordering::Relaxed);
                    });
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { uri, version, stop })
    }
}

fn serve(
    mut stream: TcpStream,
    reader: Arc<Mutex<RangeReader>>,
    token: &str,
    media_type: &str,
    length: u64,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::new();
    while request.len() < 16384 {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8(request).map_err(std::io::Error::other)?;
    let mut lines = request.lines();
    let first = lines.next().unwrap_or("");
    let get = format!("GET /{token} HTTP/1.");
    let head = format!("HEAD /{token} HTTP/1.");
    if !first.starts_with(&get) && !first.starts_with(&head) {
        return stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    let range = lines.find_map(|line| {
        line.split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("range"))
            .map(|(_, value)| value.trim())
    });
    let parsed = range.map(|range| -> Option<(u64, u64)> {
        let (start, end) = range.strip_prefix("bytes=")?.split_once('-')?;
        if start.is_empty() {
            let count = end.parse::<u64>().ok()?;
            if count == 0 {
                return None;
            }
            return Some((length.saturating_sub(count), length.checked_sub(1)?));
        }
        let start = start.parse().ok()?;
        let end = if end.is_empty() {
            length.checked_sub(1)?
        } else {
            end.parse::<u64>().ok()?.min(length.checked_sub(1)?)
        };
        (start <= end && start < length).then_some((start, end))
    });
    if matches!(parsed, Some(None)) {
        return stream.write_all(format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{length}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
    }
    let (start, end) = parsed.flatten().unwrap_or((0, length.saturating_sub(1)));
    let bytes = if length == 0 { 0 } else { end - start + 1 };
    let status = if range.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let content_range = if range.is_some() {
        format!("Content-Range: bytes {start}-{end}/{length}\r\n")
    } else {
        String::new()
    };
    stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {bytes}\r\nContent-Type: {media_type}\r\nAccept-Ranges: bytes\r\n{content_range}Connection: close\r\n\r\n").as_bytes())?;
    if first.starts_with(&head) {
        return Ok(());
    }
    let mut position = start;
    let mut buffer = [0; 32768];
    while position < start + bytes && !stop.load(Ordering::Relaxed) {
        let count = (start + bytes - position).min(buffer.len() as u64) as usize;
        let mut reader = reader
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        reader.seek(SeekFrom::Start(position))?;
        reader.read_exact(&mut buffer[..count])?;
        drop(reader);
        stream.write_all(&buffer[..count])?;
        position += count as u64;
    }
    Ok(())
}
