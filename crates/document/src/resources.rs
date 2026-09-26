use super::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use blitz_traits::net::Url;
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use std::io::Cursor;

pub(super) fn load(book: &rbook::Epub, url: &Url, depth: usize) -> Result<Vec<u8>> {
    if depth > 4 {
        return Err("SVG resource nesting exceeds the limit".into());
    }
    if url.scheme() == "data" {
        let (header, data) = url
            .as_str()
            .split_once(',')
            .ok_or("Invalid embedded image")?;
        if !header.starts_with("data:image/")
            || !header.ends_with(";base64")
            || data.len() > 8 * 1024 * 1024
        {
            return Err("Unsupported embedded resource".into());
        }
        let bytes = STANDARD.decode(data).map_err(|error| error.to_string())?;
        let reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| error.to_string())?;
        if u64::from(width) * u64::from(height) > 16_000_000 {
            return Err("Embedded image is too large".into());
        }
        return Ok(bytes);
    }
    if url.scheme() != "https" || url.host_str() != Some("book.invalid") {
        return Err("Book resources must be inside the EPUB".into());
    }
    let bytes = book
        .read_resource_bytes(url.path())
        .map_err(|error| error.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("EPUB resource exceeds 32 MiB".into());
    }
    if let Ok(reader) = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()
        && reader.format().is_some()
    {
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| error.to_string())?;
        if u64::from(width) * u64::from(height) > 16_000_000 {
            return Err("EPUB image exceeds 16 million pixels".into());
        }
        return Ok(bytes);
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Err("Compressed SVG resources are not supported".into());
    }
    if is_svg(&bytes) {
        return markup(book, url, &bytes, depth);
    }
    Ok(bytes)
}

pub(super) fn markup(
    book: &rbook::Epub,
    base: &Url,
    source: &[u8],
    depth: usize,
) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(source);
    reader.config_mut().check_end_names = false;
    let mut writer = Writer::new(Vec::with_capacity(source.len()));
    let mut svg_depth = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("Invalid EPUB markup: {error}"))?;
        match &event {
            Event::Start(tag) | Event::Empty(tag) => {
                let svg = tag.local_name().as_ref() == b"svg";
                if svg && matches!(event, Event::Start(_)) {
                    svg_depth += 1;
                }
                if svg_depth > 0 || svg {
                    let name = String::from_utf8_lossy(tag.name().as_ref()).into_owned();
                    let mut clean = BytesStart::new(name);
                    for attribute in tag.attributes() {
                        let attribute = attribute.map_err(|error| error.to_string())?;
                        let key = String::from_utf8_lossy(attribute.key.as_ref());
                        if attribute.key.local_name().as_ref() == b"href" {
                            let value = attribute
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .map_err(|error| error.to_string())?;
                            let value = if value.starts_with('#') {
                                value.into_owned()
                            } else {
                                embed(book, base, &value, depth + 1).unwrap_or_default()
                            };
                            clean.push_attribute((key.as_ref(), value.as_str()));
                        } else {
                            clean.push_attribute(attribute);
                        }
                    }
                    let event = if matches!(event, Event::Empty(_)) {
                        Event::Empty(clean)
                    } else {
                        Event::Start(clean)
                    };
                    writer
                        .write_event(event)
                        .map_err(|error| error.to_string())?;
                    continue;
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"svg" => {
                svg_depth = svg_depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
        writer
            .write_event(event)
            .map_err(|error| error.to_string())?;
    }
    Ok(writer.into_inner())
}

fn embed(book: &rbook::Epub, base: &Url, value: &str, depth: usize) -> Result<String> {
    let url = base.join(value).map_err(|error| error.to_string())?;
    let bytes = load(book, &url, depth)?;
    let mime = if is_svg(&bytes) {
        "image/svg+xml"
    } else {
        image::guess_format(&bytes)
            .map_err(|error| error.to_string())?
            .to_mime_type()
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

fn is_svg(bytes: &[u8]) -> bool {
    let mut reader = Reader::from_reader(bytes);
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                return tag.local_name().as_ref() == b"svg";
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_)) => {}
            Ok(Event::Text(text)) if text.iter().all(u8::is_ascii_whitespace) => {}
            _ => return false,
        }
    }
}
