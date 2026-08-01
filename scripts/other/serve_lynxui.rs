use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::UNIX_EPOCH,
};

const ADDRESS: &str = "127.0.0.1:6175";
const RELOAD: &str = r#"<script>
  (() => {
    let revision;
    setInterval(async () => {
      try {
        const next = await fetch("/__lynxui_revision", { cache: "no-store" }).then(response => response.text());
        if (revision && revision !== next) location.reload();
        revision = next;
      } catch (_) {}
    }, 500);
  })();
</script>"#;

fn gallery_path(name: &str) -> PathBuf {
    Path::new("crates/web/src/sand/lynx_ui").join(name)
}

fn shared_path(name: &str) -> PathBuf {
    Path::new("crates/web/static/presentation/board").join(name)
}

fn revision() -> Result<String, std::io::Error> {
    [
        gallery_path("index.html"),
        gallery_path("demo.css"),
        gallery_path("demo.js"),
        gallery_path("styles/catppuccin-macchiato.css"),
        shared_path("lynx-ui.css"),
        shared_path("lynx-ui.js"),
    ]
    .iter()
    .map(|path| {
        let metadata = fs::metadata(path)?;
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Ok(format!("{}:{modified}", metadata.len()))
    })
    .collect::<Result<Vec<_>, std::io::Error>>()
    .map(|parts| parts.join("|"))
}

fn response(status: &str, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    [head.into_bytes(), body].concat()
}

fn content(path: &str) -> Result<Vec<u8>, std::io::Error> {
    match path {
        "/" => {
            let html = fs::read_to_string(gallery_path("index.html"))?
                .replace("href=\"lynx-ui.css\"", "href=\"/lynx-ui.css\"")
                .replace("href=\"demo.css\"", "href=\"/demo.css\"")
                .replace(
                    "href=\"styles/catppuccin-macchiato.css\"",
                    "href=\"/styles/catppuccin-macchiato.css\"",
                )
                .replace("src=\"lynx-ui.js\"", "src=\"/lynx-ui.js\"")
                .replace("src=\"demo.js\"", "src=\"/demo.js\"")
                .replace("</body>", &format!("{RELOAD}</body>"));
            Ok(response(
                "200 OK",
                "text/html; charset=utf-8",
                html.into_bytes(),
            ))
        }
        "/__lynxui_revision" => Ok(response(
            "200 OK",
            "text/plain; charset=utf-8",
            revision()?.into_bytes(),
        )),
        "/lynx-ui.css" => Ok(response(
            "200 OK",
            "text/css; charset=utf-8",
            fs::read(shared_path("lynx-ui.css"))?,
        )),
        "/lynx-ui.js" => Ok(response(
            "200 OK",
            "text/javascript; charset=utf-8",
            fs::read(shared_path("lynx-ui.js"))?,
        )),
        "/demo.css" => Ok(response(
            "200 OK",
            "text/css; charset=utf-8",
            fs::read(gallery_path("demo.css"))?,
        )),
        "/demo.js" => Ok(response(
            "200 OK",
            "text/javascript; charset=utf-8",
            fs::read(gallery_path("demo.js"))?,
        )),
        "/styles/catppuccin-macchiato.css" => Ok(response(
            "200 OK",
            "text/css; charset=utf-8",
            fs::read(gallery_path("styles/catppuccin-macchiato.css"))?,
        )),
        "/fonts/Lato-Regular.ttf" => Ok(response(
            "200 OK",
            "font/ttf",
            fs::read("assets/fonts/Lato/Lato-Regular.ttf")?,
        )),
        "/fonts/Lato-Bold.ttf" => Ok(response(
            "200 OK",
            "font/ttf",
            fs::read("assets/fonts/Lato/Lato-Bold.ttf")?,
        )),
        "/fonts/Lato-Italic.ttf" => Ok(response(
            "200 OK",
            "font/ttf",
            fs::read("assets/fonts/Lato/Lato-Italic.ttf")?,
        )),
        "/fonts/Aleo-VariableFont_wght.ttf" => Ok(response(
            "200 OK",
            "font/ttf",
            fs::read("assets/fonts/Aleo/Aleo-VariableFont_wght.ttf")?,
        )),
        _ => Ok(response(
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not found".to_vec(),
        )),
    }
}

fn serve(mut stream: TcpStream) {
    let mut request = [0_u8; 2048];
    let Ok(size) = stream.read(&mut request) else {
        return;
    };
    let first_line = String::from_utf8_lossy(&request[..size]);
    let path = first_line
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");
    let body = content(path).unwrap_or_else(|error| {
        response(
            "500 Internal Server Error",
            "text/plain; charset=utf-8",
            error.to_string().into_bytes(),
        )
    });
    let _ = stream.write_all(&body);
}

fn main() -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(ADDRESS)?;
    println!("LynxUI Gallery: http://{ADDRESS}");
    for stream in listener.incoming().flatten() {
        thread::spawn(|| serve(stream));
    }
    Ok(())
}
