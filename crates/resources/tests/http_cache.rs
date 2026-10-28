use lince_resources::Library;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

struct Server {
    uri: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(reply: impl Fn(usize, &str) -> String + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let uri = format!("http://{}/document", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let requests = count.clone();
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => {
                        thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    request.push(byte[0]);
                }
                let index = requests.fetch_add(1, Ordering::Relaxed);
                let response = reply(
                    index,
                    &String::from_utf8(request).unwrap().to_ascii_lowercase(),
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            uri,
            count,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn fresh_fragments_share_bytes_and_stale_responses_validate_then_change() {
    let server = Server::new(|index, request| match index {
        0 => response(
            "200 OK",
            "Cache-Control: max-age=3600\r\nETag: \"one\"\r\n",
            "one",
        ),
        1 => {
            assert!(request.contains("if-none-match: \"one\""));
            response("304 Not Modified", "Cache-Control: no-cache\r\n", "")
        }
        _ => {
            assert!(request.contains("if-none-match: \"one\""));
            response(
                "200 OK",
                "Cache-Control: max-age=3600\r\nETag: \"two\"\r\n",
                "two",
            )
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    assert_eq!(
        library
            .source_text(&format!("{}#first", server.uri), 1024, false)
            .unwrap(),
        "one"
    );
    assert_eq!(
        library
            .source_text(&format!("{}#second", server.uri), 1024, false)
            .unwrap(),
        "one"
    );
    assert_eq!(server.count.load(Ordering::Relaxed), 1);
    assert_eq!(library.source_text(&server.uri, 1024, true).unwrap(), "one");
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "two"
    );
    assert_eq!(server.count.load(Ordering::Relaxed), 3);
}

#[test]
fn no_store_and_vary_star_leave_no_persisted_response() {
    for policy in [
        "Cache-Control: no-store\r\n",
        "Cache-Control: max-age=3600\r\nVary: *\r\n",
    ] {
        let server = Server::new(move |_, _| response("200 OK", policy, "private"));
        let directory = tempfile::tempdir().unwrap();
        let library = Library::new(directory.path());
        for _ in 0..2 {
            assert_eq!(
                library.source_text(&server.uri, 1024, false).unwrap(),
                "private"
            );
        }
        assert_eq!(server.count.load(Ordering::Relaxed), 2);
        assert_eq!(
            std::fs::read_dir(library.cache().join("http"))
                .unwrap()
                .count(),
            0
        );
        assert!(!directory.path().join("files/catalogue.json").exists());
    }
}

#[test]
fn age_expires_and_request_variation_do_not_reuse_the_wrong_representation() {
    let server = Server::new(|_, request| {
        let language = if request.contains("accept-language: pt") {
            "pt"
        } else {
            "en"
        };
        response(
            "200 OK",
            "Cache-Control: max-age=60\r\nAge: 61\r\nVary: Accept-Language\r\n",
            language,
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    for language in ["en", "pt", "pt"] {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("accept-language", language.parse().unwrap());
        let file = library
            .http_file(&server.uri, headers, 1024, false)
            .unwrap();
        assert_eq!(std::fs::read_to_string(file.path).unwrap(), language);
    }
    assert_eq!(server.count.load(Ordering::Relaxed), 3);
}

#[test]
fn replacing_a_cached_response_with_no_store_removes_the_old_automatic_copy() {
    let server = Server::new(|index, _| {
        if index == 0 {
            response("200 OK", "Cache-Control: max-age=3600\r\n", "old")
        } else {
            response("200 OK", "Cache-Control: no-store\r\n", "new")
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "old"
    );
    assert_eq!(library.source_text(&server.uri, 1024, true).unwrap(), "new");
    assert_eq!(
        std::fs::read_dir(library.cache().join("http"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn evicted_response_bytes_are_refetched_without_losing_explicit_snapshots() {
    let server = Server::new(|index, request| {
        assert!(!request.contains("if-none-match:"));
        response(
            "200 OK",
            "Cache-Control: max-age=3600\r\nETag: \"edition\"\r\n",
            if index == 0 { "original" } else { "updated" },
        )
    });
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let original = library
        .http_file(&server.uri, Default::default(), 1024, false)
        .unwrap();
    let snapshot = library
        .retain_bytes(
            &std::fs::read(&original.path).unwrap(),
            &server.uri,
            &original.uri,
        )
        .unwrap();
    std::fs::remove_file(original.path).unwrap();
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "updated"
    );
    assert_eq!(std::fs::read_to_string(snapshot.path).unwrap(), "original");
    assert_eq!(server.count.load(Ordering::Relaxed), 2);
}

#[test]
fn revalidation_uses_the_current_redirect_destination_as_the_document_base() {
    let server = Server::new(|index, request| match index {
        0 => response("302 Found", "Location: /one/Document.lingua\r\n", ""),
        1 => {
            assert!(request.starts_with("get /one/document.lingua "));
            response(
                "200 OK",
                "Cache-Control: no-cache\r\nETag: \"edition\"\r\n",
                "unchanged",
            )
        }
        2 => {
            assert!(request.contains("if-none-match: \"edition\""));
            response("302 Found", "Location: /two/Document.lingua\r\n", "")
        }
        3 => {
            assert!(request.starts_with("get /two/document.lingua "));
            assert!(request.contains("if-none-match: \"edition\""));
            response("304 Not Modified", "Cache-Control: max-age=3600\r\n", "")
        }
        _ => panic!("An unvisited target was retrieved"),
    });
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "unchanged"
    );
    assert!(
        library
            .source_base(&server.uri)
            .unwrap()
            .ends_with("/one/Document.lingua")
    );
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "unchanged"
    );
    assert!(
        library
            .source_base(&server.uri)
            .unwrap()
            .ends_with("/two/Document.lingua")
    );
    assert_eq!(
        library.source_text(&server.uri, 1024, false).unwrap(),
        "unchanged"
    );
    assert_eq!(server.count.load(Ordering::Relaxed), 4);
}
