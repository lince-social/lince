// non-lean-yet
// Scope: fn pdf, fn pdf_fixture, fn epub, fn epub_fixture.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 137, 146, 147, 165, 173, 183, 372, 432.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 144.
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites
//   402, 403.
// - [Clippy: clippy::manual_let_else] this could be rewritten as `let...else`. Sites 406.

use std::{
    io::{Cursor, Read, Write},
    time::Instant,
};

use lince_resources::{Library, Locator, Resource};

use super::*;
use crate::{
    geometry::{MAX_PDF_HEIGHT, bounded_page_height, scale_ceil, scale_floor},
    reader::Content,
    tile::PixelRect,
};

fn pdf() -> Vec<u8> {
    pdf_fixture(0)
}

fn pdf_fixture(padding: usize) -> Vec<u8> {
    let stream =
        "0.1 0.3 0.7 rg 32 300 296 140 re f BT /F1 24 Tf 32 260 Td (Lince PDF reader) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F1 4 0 R >> >> /Contents 7 0 R >>".into(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        if index == 6 {
            bytes.resize(bytes.len() + padding / 2, b' ');
        }
        offsets.push(bytes.len());
        write!(bytes, "{} 0 obj\n{object}\nendobj\n", index + 1).unwrap();
    }
    bytes.resize(bytes.len() + padding - padding / 2, b' ');
    let xref = bytes.len();
    write!(
        bytes,
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    )
    .unwrap();
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    )
    .unwrap();
    bytes
}

fn epub(encrypted: bool) -> Vec<u8> {
    epub_fixture(encrypted, 0)
}

fn epub_fixture(encrypted: bool, padding: usize) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let paragraphs = "<p>A native reader remembers your place. <em>Italic text</em>, <strong>bold text</strong>, and EPUB styles travel with the book.</p>".repeat(24);
    let chapter = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Lince EPUB</title><link rel="stylesheet" href="style.css"/></head><body><h1>Lince EPUB reader</h1><img src="art.svg"/><p class="accent">Publisher styles, images and flowing text.</p>{paragraphs}<p id="argument">A particular argument near the end.</p><table><tr><td>First</td><td>Second</td></tr></table></body></html>"#
    );
    let entries = [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="EPUB/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        ),
        (
            "EPUB/book.opf",
            r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">lince-test</dc:identifier><dc:title>Lince reader</dc:title><dc:language>en</dc:language></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="css" href="style.css" media-type="text/css"/><item id="art" href="art.svg" media-type="image/svg+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>"#,
        ),
        ("EPUB/one.xhtml", chapter.as_str()),
        (
            "EPUB/two.xhtml",
            "<html><body><h1>Chapter two</h1><p>Your reading progress survives a restart.</p></body></html>",
        ),
        (
            "EPUB/style.css",
            "@import 'colors.css'; body { font-family: Lato; } h1 { color: #123d80; } .accent { background: #ff0000; padding: 16px; } td { border: 1px solid black; padding: 8px; }",
        ),
        ("EPUB/colors.css", "p { color: #153333; }"),
        (
            "EPUB/art.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="60"><rect width="160" height="60" fill="#00ff00"/></svg>"##,
        ),
    ];
    for (name, content) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    let mut image = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(80, 40, image::Rgba([20, 30, 200, 255]))
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    zip.start_file("EPUB/picture.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(image.get_ref()).unwrap();
    if padding > 0 {
        zip.start_file(
            "EPUB/unused.bin",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(&vec![0; padding]).unwrap();
    }
    if encrypted {
        zip.start_file(
            "META-INF/encryption.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<encryption/>").unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn renders_pdf_text_vectors_and_reuses_page_pixels() {
    let mut reader = Reader::from_bytes(pdf(), "test.pdf").unwrap();
    assert!(reader.info.is_pdf());
    assert_eq!(reader.info.sections.len(), 2);
    let layout = reader.layout(0, 720).unwrap();
    assert_eq!(
        layout,
        Layout {
            width: 720,
            height: 960
        }
    );
    let first = reader.tile(0, 720, 0).unwrap();
    assert!(
        first
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 100 && p[0] < 50)
            .count()
            > 50_000
    );
    let cached = reader.tile(0, 720, 0).unwrap();
    assert_eq!(first.rgba, cached.rgba);
    let second = reader.tile(0, 720, 1).unwrap();
    let overlap = (TILE_OVERLAP * 720 * 4) as usize;
    assert_eq!(
        &first.rgba[first.rgba.len() - overlap..],
        &second.rgba[..overlap]
    );
    assert_eq!(reader.tile(0, 720, 1).unwrap().height, 448);
    assert!(reader.tile(0, 720, 2).is_err());
    assert!(reader.tile(2, 720, 0).is_err());
}

#[test]
fn renders_epub_external_css_imports_images_and_reflow() {
    let start = Instant::now();
    let mut reader = Reader::from_bytes(epub(false), "test.epub").unwrap();
    assert_eq!(reader.info.sections.len(), 2);
    let layout = reader.layout(0, 720).unwrap();
    assert!(layout.height > 1500);
    let tile = reader.tile(0, 720, 0).unwrap();
    assert!(
        tile.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 240 && p[1] < 10)
            .count()
            > 2000,
        "external CSS was not painted"
    );
    assert!(
        tile.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[1] > 240 && p[0] < 10)
            .count()
            > 2000,
        "EPUB image was not painted"
    );
    for index in [1, 2, 3] {
        let tile = reader.tile(0, 720, index).unwrap();
        assert!(
            tile.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[0] < 120)
                .count()
                > 500,
            "Text disappeared from EPUB tile {index}"
        );
    }
    assert!(reader.layout(0, 400).unwrap().height > layout.height);
    assert!(reader.layout(1, 720).unwrap().height < layout.height);
    eprintln!(
        "EPUB parse, CSS, image, three layouts and four tiles: {:?}",
        start.elapsed()
    );
}

#[test]
fn rejects_bad_documents_and_invalid_positions() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(MAX_FILE_BYTES + 1).unwrap();
    assert!(Reader::open(file.path()).is_err());
    assert!(Reader::from_bytes(b"garbage".to_vec(), "bad.pdf").is_err());
    assert!(Reader::from_bytes(b"PKtruncated".to_vec(), "bad.epub").is_err());
    assert!(Reader::from_bytes(epub(true), "encrypted.epub").is_err());
    assert!(
        !Position {
            fraction: f32::NAN,
            ..Default::default()
        }
        .valid()
    );
    assert!(
        !Position {
            fraction: 2.0,
            ..Default::default()
        }
        .valid()
    );
    let mut position = Position {
        section: 1,
        mode: Mode::Pages,
        ..Default::default()
    };
    position.set_offset(750.0, 2000.0, 500.0);
    let restored: Position =
        serde_json::from_slice(&serde_json::to_vec(&position).unwrap()).unwrap();
    assert_eq!(restored.offset(2000.0, 500.0), 750.0);
    assert_eq!(restored.mode, Mode::Pages);
    assert_eq!(restored.section, 1);
}

#[test]
fn svg_resources_are_confined_to_the_epub() {
    let book = rbook::Epub::read(Cursor::new(epub(false))).unwrap();
    let base = blitz_traits::net::Url::parse("https://book.invalid/EPUB/one.xhtml").unwrap();
    for address in ["file:///etc/passwd", "https://example.com/image.png"] {
        assert!(
            super::resources::load(&book, &blitz_traits::net::Url::parse(address).unwrap(), 0)
                .is_err()
        );
        let markup = format!(r#"<html><body><svg><image href="{address}"/></svg></body></html>"#);
        let sanitized = super::resources::markup(&book, &base, markup.as_bytes(), 0).unwrap();
        assert!(!String::from_utf8(sanitized).unwrap().contains(address));
    }
    let markup = br##"<svg><image href="art.svg"/><use href="#shape"/></svg>"##;
    let sanitized =
        String::from_utf8(super::resources::markup(&book, &base, markup, 0).unwrap()).unwrap();
    assert!(sanitized.contains("data:image/svg+xml;base64,"));
    assert!(sanitized.contains("#shape"));
}

#[test]
fn export_visual_fixtures_when_requested() {
    let Some(directory) = std::env::var_os("LINCE_DOCUMENT_FIXTURES") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for (name, bytes) in [("sample.pdf", pdf()), ("sample.epub", epub(false))] {
        std::fs::write(directory.join(name), &bytes).unwrap();
        let mut reader = Reader::from_bytes(bytes, name).unwrap();
        let tile = reader.tile(0, 720, 0).unwrap();
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &tile.rgba,
            tile.width,
            tile.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn chapter_passage_image_and_page_region_references_use_shared_locators() {
    let mut reader = Reader::from_bytes(epub(false), "test.epub").unwrap();
    let locator = Locator {
        href: "EPUB/one.xhtml".into(),
        fragment: "argument".into(),
        ..Default::default()
    };
    for width in [400, 720] {
        let position = reader.locate(&locator, width).unwrap();
        assert_eq!(position.section, 0);
        assert!(position.fraction > 0.5);
        assert!(!reader.excerpt(&locator, width).unwrap().rgba.is_empty());
    }
    let quote = Locator {
        href: "EPUB/one.xhtml".into(),
        quote: "A particular argument near the end.".into(),
        ..Default::default()
    };
    assert!(reader.locate(&quote, 720).unwrap().fraction > 0.5);
    assert!(
        reader
            .locate(
                &Locator {
                    fragment: "absent".into(),
                    ..locator
                },
                720
            )
            .is_err()
    );
    let image = reader
        .excerpt(
            &Locator {
                image: "EPUB/picture.png".into(),
                region: Some([0.25, 0.25, 0.5, 0.5]),
                ..Default::default()
            },
            80,
        )
        .unwrap();
    assert_eq!((image.width, image.height), (40, 20));
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [20, 30, 200, 255])
    );
    let mut reader = Reader::from_bytes(pdf(), "test.pdf").unwrap();
    let locator = Locator {
        page: Some(2),
        end_page: Some(2),
        region: Some([0.25, 0.25, 0.5, 0.5]),
        ..Default::default()
    };
    assert_eq!(reader.locate(&locator, 720).unwrap().section, 1);
    let region = reader.excerpt(&locator, 720).unwrap();
    assert_eq!((region.width, region.height), (360, 480));
    assert!(
        reader
            .locate(
                &Locator {
                    end_page: Some(3),
                    ..locator
                },
                720
            )
            .is_err()
    );
    assert!(
        Tile {
            width: 1,
            height: 1,
            rgba: vec![0; 4]
        }
        .crop([0.8, 0.0, 0.5, 1.0])
        .is_err()
    );
}

#[test]
fn local_pdf_uses_the_seekable_backend_when_pdfium_is_available() {
    if std::env::var_os("LINCE_PDFIUM_LIBRARY").is_none() {
        return;
    }
    let temporary = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temporary.path(), pdf()).unwrap();
    let mut reader = Reader::open(temporary.path()).unwrap();
    assert!(matches!(&reader.content, Content::PdfOnline(_)));
    assert_eq!(reader.info.sections.len(), 2);
    assert!(
        reader
            .tile(1, 720, 0)
            .unwrap()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[2] > 100 && pixel[0] < 50)
    );
}

struct OnlineSource {
    resource: Resource,
    sent: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl OnlineSource {
    fn new(bytes: Vec<u8>, media_type: &str) -> Self {
        use std::{
            net::TcpListener,
            sync::{
                Arc,
                atomic::{AtomicBool, AtomicUsize, Ordering},
            },
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let resource = Resource {
            uri: format!("http://{}/book", listener.local_addr().unwrap()),
            media_type: media_type.into(),
            hash: Some(lince_resources::fingerprint(&bytes)),
            ..Default::default()
        };
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicUsize::new(0));
        let stopped = stop.clone();
        let transferred = sent.clone();
        let server = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") && request.len() < 16_384 {
                    let mut byte = [0];
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                let range = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .expect("The reader must request source ranges");
                let (start, end) = range.split_once('-').unwrap();
                let start: usize = start.parse().unwrap();
                let end: usize = end.parse().unwrap();
                let content = &bytes[start..=end];
                let header = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nETag: \"edition-1\"\r\nConnection: close\r\n\r\n",
                    content.len(),
                    bytes.len()
                );
                if stream.write_all(header.as_bytes()).is_ok() {
                    let _ = stream.write_all(content);
                }
                transferred.fetch_add(content.len(), Ordering::Relaxed);
            }
        });
        Self {
            resource,
            sent,
            stop,
            server: Some(server),
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

impl Drop for OnlineSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
fn online_books_render_from_ranges_and_reopen_seen_content_without_the_server() {
    use std::sync::atomic::Ordering;
    let mut books = vec![(
        epub_fixture(false, 20 * 1024 * 1024),
        "application/epub+zip",
    )];
    if std::env::var_os("LINCE_PDFIUM_LIBRARY").is_some() {
        books.push((pdf_fixture(20 * 1024 * 1024), "application/pdf"));
    }
    for (bytes, media_type) in books {
        let mut source = OnlineSource::new(bytes, media_type);
        let temporary = tempfile::tempdir().unwrap();
        let library = Library::new(temporary.path());
        let mut reader = Reader::open_resource(library.clone(), &source.resource).unwrap();
        if media_type == "application/pdf" {
            assert!(matches!(&reader.content, Content::PdfOnline(_)));
        }
        let tile = reader.tile(0, 720, 0).unwrap();
        assert!(!tile.rgba.is_empty());
        assert!(
            source.sent.load(Ordering::Relaxed) < 4 * 1024 * 1024,
            "Reading one page fetched the complete source"
        );
        assert!(library.find(&source.resource).unwrap().is_none());
        source.stop();
        drop(reader);
        let mut offline = Reader::open_resource(library, &source.resource).unwrap();
        assert_eq!(offline.tile(0, 720, 0).unwrap().rgba, tile.rgba);
        if media_type == "application/pdf" {
            assert!(offline.tile(1, 720, 0).is_err());
        }
    }
}

/// A tile whose pixel at `(x, y)` is `[x, y, x ^ y, 255]`.
fn grid(width: u8, height: u8) -> Tile {
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[x, y, x ^ y, 255]);
        }
    }
    Tile {
        width: width.into(),
        height: height.into(),
        rgba,
    }
}

/// Copies `rect` out of `tile` by direct indexing, independently of the code under test.
fn pixels_in(tile: &Tile, rect: PixelRect) -> Vec<u8> {
    let stride = tile.width as usize * 4;
    (rect.top..rect.top + rect.height)
        .flat_map(|row| {
            let start = row as usize * stride + rect.left as usize * 4;
            tile.rgba[start..start + rect.width as usize * 4]
                .iter()
                .copied()
        })
        .collect()
}

fn rect(left: u32, top: u32, width: u32, height: u32) -> PixelRect {
    PixelRect {
        left,
        top,
        width,
        height,
    }
}

#[test]
fn crop_keeps_every_partially_covered_pixel() {
    let cropped = grid(4, 3).crop([0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0]).unwrap();
    assert_eq!((cropped.width, cropped.height), (2, 2));
    assert_eq!(
        cropped.rgba,
        [1, 1, 0, 255, 2, 1, 3, 255, 1, 2, 3, 255, 2, 2, 0, 255]
    );

    let tile = grid(9, 7);
    for (region, expected) in [
        ([0.0, 0.0, 1.0, 1.0], rect(0, 0, 9, 7)),
        ([0.25, 0.25, 0.5, 0.5], rect(2, 1, 5, 4)),
        ([0.33, 0.66, 0.67, 0.34], rect(2, 4, 7, 3)),
        // Validation tolerates a region that overshoots the edge by 0.0001,
        // and the crop clamps it to the tile.
        ([0.5, 0.0, 0.5001, 1.0], rect(4, 0, 5, 7)),
        // A region smaller than a pixel still keeps the pixel it touches.
        ([0.1, 0.3, 0.0001, 0.0001], rect(0, 2, 1, 1)),
        ([0.9999, 0.9999, 0.0001, 0.0001], rect(8, 6, 1, 1)),
    ] {
        let cropped = tile.clone().crop(region).unwrap();
        assert_eq!(
            (cropped.width, cropped.height),
            (expected.width, expected.height),
            "{region:?}"
        );
        assert_eq!(cropped.rgba, pixels_in(&tile, expected), "{region:?}");
    }
}

#[test]
fn crop_rejects_invalid_regions_and_tiles() {
    let invalid_region = "Choose a valid page, passage, image region or time interval";
    for region in [
        [0.8, 0.0, 0.5, 1.0],
        [0.0, 0.0, 0.0, 0.0],
        [-0.1, 0.0, 0.5, 0.5],
        [0.5, 0.5, f32::NAN, 0.1],
    ] {
        assert_eq!(
            grid(9, 7).crop(region).unwrap_err(),
            invalid_region,
            "{region:?}"
        );
    }
    for (width, height, rgba) in [
        (0, 1, Vec::new()),
        (2, 1, vec![0; 4]),
        (u32::MAX, u32::MAX, Vec::new()),
    ] {
        let tile = Tile {
            width,
            height,
            rgba,
        };
        assert_eq!(
            tile.crop([0.0, 0.0, 1.0, 1.0]).unwrap_err(),
            "Invalid image dimensions",
            "{width}x{height}"
        );
    }
}

#[test]
fn rows_and_rects_stay_inside_the_buffer() {
    let tile = grid(4, 3);
    let rows = Tile::from_rows(&tile.rgba, 4, 1, 2).unwrap();
    assert_eq!(rows.rgba, pixels_in(&tile, rect(0, 1, 4, 2)));
    assert!(Tile::from_rows(&tile.rgba, 4, 2, 2).is_none());
    assert!(Tile::from_rows(&tile.rgba, 4, u32::MAX, 1).is_none());

    let band = tile.clone().into_rect(rect(0, 1, 4, 2)).unwrap();
    assert_eq!(band.rgba, rows.rgba);
    let inner = tile.clone().into_rect(rect(1, 1, 2, 2)).unwrap();
    assert_eq!(inner.rgba, pixels_in(&tile, rect(1, 1, 2, 2)));
    for outside in [rect(3, 0, 2, 1), rect(0, 2, 1, 2), rect(u32::MAX, 0, 2, 1)] {
        assert!(tile.clone().into_rect(outside).is_none(), "{outside:?}");
    }
    let short = Tile {
        rgba: vec![0; 4],
        ..tile
    };
    assert!(short.into_rect(rect(0, 0, 1, 1)).is_none());
}

#[test]
fn page_heights_accept_only_their_limits() {
    assert_eq!(bounded_page_height(1.0, MAX_PDF_HEIGHT, "limit"), Ok(1));
    assert_eq!(
        bounded_page_height(8192.0, MAX_PDF_HEIGHT, "limit"),
        Ok(8192)
    );
    for height in [0.0, 0.999, 8193.0, f32::NAN, f32::INFINITY, -1.0] {
        assert_eq!(
            bounded_page_height(height, MAX_PDF_HEIGHT, "limit"),
            Err("limit".into()),
            "{height}"
        );
    }
}

#[test]
fn scaled_fractions_round_at_the_extent_edges() {
    assert_eq!(scale_floor(1.0, 7), 7);
    assert_eq!(scale_floor(0.9999, 10_000), 9999);
    assert_eq!(scale_ceil(0.9999, 7), 7);
    assert_eq!(scale_ceil(0.0001, 7), 1);
    assert_eq!(scale_floor(0.0, 7), 0);
    // `u32::MAX` rounds up to 2^32 in f32, so the conversion saturates.
    assert_eq!(scale_floor(1.0, u32::MAX), u32::MAX);
}

#[test]
fn excerpt_regions_cut_the_same_pixels_as_the_whole_section() {
    let mut readers = vec![
        Reader::from_bytes(pdf(), "test.pdf").unwrap(),
        Reader::from_bytes(epub(false), "test.epub").unwrap(),
    ];
    let pdfium = std::env::var_os("LINCE_PDFIUM_LIBRARY").map(|_| {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temporary.path(), pdf()).unwrap();
        Reader::open(temporary.path()).unwrap()
    });
    readers.extend(pdfium);
    for mut reader in readers {
        let (page, layout) = if reader.info.is_pdf() {
            (
                Locator {
                    page: Some(2),
                    ..Default::default()
                },
                Layout {
                    width: 400,
                    height: 534,
                },
            )
        } else {
            (
                Locator {
                    href: "EPUB/two.xhtml".into(),
                    ..Default::default()
                },
                Layout {
                    width: 400,
                    height: 207,
                },
            )
        };
        let whole = reader
            .excerpt(
                &Locator {
                    region: Some([0.0, 0.0, 1.0, 1.0]),
                    ..page.clone()
                },
                400,
            )
            .unwrap();
        assert_eq!((whole.width, whole.height), (layout.width, layout.height));
        // A PDF excerpt starts at the region's top row. An EPUB excerpt
        // starts at the located passage, here the chapter start.
        let top = |pdf_top: u32| if reader.info.is_pdf() { pdf_top } else { 0 };
        for (region, expected) in [
            (
                [0.25, 0.25, 0.5, 0.5],
                rect(100, top(133), 200, layout.height / 2),
            ),
            ([0.5, 0.0, 0.5001, 1.0], rect(200, 0, 200, layout.height)),
            (
                [0.3, 0.1, 0.4, 0.05],
                rect(120, top(53), 160, layout.height / 20),
            ),
            ([0.9999, 0.9999, 0.0001, 0.0001], rect(399, top(533), 1, 1)),
        ] {
            let excerpt = reader
                .excerpt(
                    &Locator {
                        region: Some(region),
                        ..page.clone()
                    },
                    400,
                )
                .unwrap();
            assert_eq!(
                (excerpt.width, excerpt.height),
                (expected.width, expected.height),
                "{region:?}"
            );
            assert_eq!(excerpt.rgba, pixels_in(&whole, expected), "{region:?}");
        }
    }
}

#[test]
fn reader_debug_names_the_backend_and_metadata() {
    let debug = format!("{:?}", Reader::from_bytes(pdf(), "debug.pdf").unwrap());
    assert!(
        debug.starts_with(r#"Reader { backend: "Pdf", info: Info { title: "debug.pdf""#),
        "{debug}"
    );
}

#[test]
fn svg_resources_nest_only_to_the_limit() {
    let book = rbook::Epub::read(Cursor::new(epub(false))).unwrap();
    let url = blitz_traits::net::Url::parse("https://book.invalid/EPUB/art.svg").unwrap();
    assert!(super::resources::load(&book, &url, 4).is_ok());
    assert_eq!(
        super::resources::load(&book, &url, 5).unwrap_err(),
        "SVG resource nesting exceeds the limit"
    );
}

#[test]
fn open_resource_rejects_unrecognized_bytes_before_downloading_them() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::new(directory.path().join("library"));
    let resource = |name: &str, bytes: &[u8]| {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        Resource {
            uri: path.to_string_lossy().into_owned(),
            title: name.into(),
            ..Default::default()
        }
    };
    let text = resource("notes.txt", b"not a document");
    assert_eq!(
        Reader::open_resource(library.clone(), &text).unwrap_err(),
        "This file is not a PDF or EPUB"
    );
    let book = resource("book.epub", &epub(false));
    let reader = Reader::open_resource(library, &book).unwrap();
    assert_eq!(reader.info.title, "book.epub");
    assert_eq!(reader.info.sections, ["/EPUB/one.xhtml", "/EPUB/two.xhtml"]);
}
