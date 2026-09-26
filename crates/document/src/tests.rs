use super::*;
use std::{
    io::{Cursor, Write},
    time::Instant,
};

fn pdf() -> Vec<u8> {
    let stream =
        "0.1 0.3 0.7 rg 32 300 296 140 re f BT /F1 24 Tf 32 260 Td (Lince PDF reader) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 480] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        write!(bytes, "{} 0 obj\n{object}\nendobj\n", index + 1).unwrap();
    }
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
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let paragraphs = "<p>A native reader remembers your place. <em>Italic text</em>, <strong>bold text</strong>, and EPUB styles travel with the book.</p>".repeat(24);
    let chapter = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Lince EPUB</title><link rel="stylesheet" href="style.css"/></head><body><h1>Lince EPUB reader</h1><img src="art.svg"/><p class="accent">Publisher styles, images and flowing text.</p>{paragraphs}<table><tr><td>First</td><td>Second</td></tr></table></body></html>"#
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
    assert!(reader.info.is_pdf);
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
            .chunks_exact(4)
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
            .chunks_exact(4)
            .filter(|p| p[0] > 240 && p[1] < 10)
            .count()
            > 2000,
        "external CSS was not painted"
    );
    assert!(
        tile.rgba
            .chunks_exact(4)
            .filter(|p| p[1] > 240 && p[0] < 10)
            .count()
            > 2000,
        "EPUB image was not painted"
    );
    for index in [1, 2, 3] {
        let tile = reader.tile(0, 720, index).unwrap();
        assert!(
            tile.rgba
                .chunks_exact(4)
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
