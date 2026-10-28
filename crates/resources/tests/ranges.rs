use lince_resources::{Library, RangeReader, Resource};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

#[test]
fn large_sources_are_read_incrementally_and_seen_ranges_survive_offline() {
    let temporary = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}/book", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(AtomicUsize::new(0));
    let stop = stopped.clone();
    let bytes_sent = sent.clone();
    let server = thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => {
                    thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut byte = [0];
                if stream.read_exact(&mut byte).is_err() {
                    break;
                }
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            let range = request
                .lines()
                .find_map(|line| line.strip_prefix("range: bytes="))
                .unwrap();
            let (start, end) = range.split_once('-').unwrap();
            let start: usize = start.parse().unwrap();
            let end: usize = end.parse().unwrap();
            let length = end - start + 1;
            let validator = if request.contains("get /weak ") {
                "ETag: W/\"edition-1\"\r\nLast-Modified: Sun, 25 Oct 2026 10:00:00 GMT"
            } else {
                "ETag: \"edition-1\""
            };
            let header = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {length}\r\nContent-Range: bytes {start}-{end}/20971520\r\n{validator}\r\nConnection: close\r\n\r\n"
            );
            if stream.write_all(header.as_bytes()).is_ok() {
                let _ = stream.write_all(&vec![7; length]);
            }
            bytes_sent.fetch_add(length, Ordering::Relaxed);
        }
    });
    let resource = Resource {
        uri,
        hash: Some(lince_resources::fingerprint(&vec![7; 20 * 1024 * 1024])),
        ..Default::default()
    };
    let library = Library::new(temporary.path());
    let mut reader = RangeReader::open(library.clone(), &resource).unwrap();
    assert_eq!(reader.len(), 20 * 1024 * 1024);
    let mut bytes = [0; 32];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, [7; 32]);
    reader.seek(SeekFrom::End(-32)).unwrap();
    reader.read_exact(&mut bytes).unwrap();
    assert!(sent.load(Ordering::Relaxed) < 200_000);
    stopped.store(true, Ordering::Relaxed);
    server.join().unwrap();
    let mut offline = RangeReader::open(library, &resource).unwrap();
    offline.read_exact(&mut bytes).unwrap();
    offline.seek(SeekFrom::End(-32)).unwrap();
    offline.read_exact(&mut bytes).unwrap();
    offline.seek(SeekFrom::Start(2 * 1024 * 1024)).unwrap();
    assert!(offline.read_exact(&mut bytes).is_err());
}

#[test]
fn complete_retention_recovers_when_the_origin_stops_honouring_ranges() {
    let temporary = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}/book", listener.local_addr().unwrap());
    let body = vec![9; 1024];
    let hash = lince_resources::fingerprint(&body);
    let server = thread::spawn(move || {
        for request_index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            if request_index == 0 {
                assert!(request.contains("range: bytes=0-0"));
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/1024\r\nETag: \"edition\"\r\nConnection: close\r\n\r\n").unwrap();
                let _ = stream.write_all(&body[..1]);
            } else {
                assert_eq!(request.contains("range: bytes="), request_index == 1);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                let _ = stream.write_all(&body);
            }
        }
    });
    let library = Library::new(temporary.path());
    let entry = library
        .fetch(&Resource {
            uri,
            hash: Some(hash.clone()),
            ..Default::default()
        })
        .unwrap();
    server.join().unwrap();
    assert_eq!(entry.hash, hash);
    assert_eq!(entry.bytes, 1024);
    assert_eq!(std::fs::read(entry.path).unwrap(), vec![9; 1024]);
}

#[test]
fn weak_etags_and_last_modified_use_a_full_response_without_combining_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}/weak", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            if index == 0 {
                assert!(request.contains("range: bytes=0-0"));
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/1024\r\nETag: W/\"one\"\r\nLast-Modified: Sun, 25 Oct 2026 10:00:00 GMT\r\nConnection: close\r\n\r\nX").unwrap();
            } else {
                assert!(!request.contains("if-range:"));
                assert!(!request.contains("range:"));
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nCache-Control: max-age=3600\r\nConnection: close\r\n\r\n").unwrap();
                stream.write_all(&vec![8; 1024]).unwrap();
            }
        }
    });
    let library = Library::new(directory.path());
    let resource = Resource {
        uri,
        ..Default::default()
    };
    let mut reader = RangeReader::open(library.clone(), &resource).unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, vec![8; 1024]);
    assert_eq!(RangeReader::cached(&library, &resource), (0, None));
    server.join().unwrap();
}

#[test]
fn a_complete_range_snapshot_must_match_its_whole_file_pin() {
    let temporary = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}/book", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for length in [1usize, 1024] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let header = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {length}\r\nContent-Range: bytes 0-{}/1024\r\nETag: \"edition\"\r\nConnection: close\r\n\r\n",
                length - 1
            );
            stream.write_all(header.as_bytes()).unwrap();
            let _ = stream.write_all(&vec![7; length]);
        }
    });
    let mut reader = RangeReader::open(
        Library::new(temporary.path()),
        &Resource {
            uri,
            hash: Some(lince_resources::fingerprint(&vec![8; 1024])),
            ..Default::default()
        },
    )
    .unwrap();
    let error = reader.read_exact(&mut [0; 16]).unwrap_err();
    assert!(error.to_string().contains("content hash"), "{error}");
    assert!(reader.read_exact(&mut [0; 16]).is_err());
    server.join().unwrap();
}
