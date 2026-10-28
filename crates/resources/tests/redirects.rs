#![cfg(feature = "publication")]

use lince_resources::{Library, publication::Publication};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

#[test]
fn source_redirects_keep_the_final_base_and_do_not_follow_bibliography_links() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path());
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let entry = format!("http://{}/entry", server.local_addr().unwrap());
    let worker = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = server.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap();
            let (status, extra, body) = if request.starts_with("GET /entry ") {
                (
                    "302 Found",
                    "Location: /published/Root.lingua\r\n",
                    String::new(),
                )
            } else if request.starts_with("GET /published/Root.lingua ") {
                ("200 OK","Cache-Control: max-age=3600\r\n","Root (@root: 0, #contains @child) { r_00000000000000000000000001\n![Diagram](figure.png)\n[Reference](https://example.invalid/bibliography)\n} r_00000000000000000000000001\nExtension @root \"lince.source\" { dependencies [{ uri \"Child.lingua\" }] }".into())
            } else {
                assert!(request.starts_with("GET /published/Child.lingua "));
                ("200 OK","Cache-Control: max-age=3600\r\n","Child (@child: 0) { r_00000000000000000000000002\n} r_00000000000000000000000002".into())
            };
            stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
        }
    });
    let source = Publication::load(&library, &entry, &[], true).unwrap();
    worker.join().unwrap();
    assert_eq!(source.documents.len(), 2);
    assert!(source.documents[0].base.ends_with("/published/Root.lingua"));
    assert!(
        source.resources().unwrap()[0]
            .1
            .uri
            .ends_with("/published/figure.png")
    );
    let offline = Publication::load(&library, &entry, &[], false).unwrap();
    assert_eq!(offline.revision, source.revision);
}
