use std::{borrow::Cow, io::Cursor};

use base64::{Engine, engine::general_purpose::STANDARD};
use blitz_traits::net::Url;
use image::ImageReader;
use quick_xml::{
    Reader, Writer, XmlVersion,
    events::{BytesStart, Event},
};

use crate::Result;

const MAX_SVG_DEPTH: usize = 4;
const MAX_EMBEDDED_BASE64_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const BOOK_HOST: &str = "book.invalid";
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

pub(crate) fn load(book: &rbook::Epub, url: &Url, depth: usize) -> Result<Vec<u8>> {
    if depth > MAX_SVG_DEPTH {
        return Err("SVG resource nesting exceeds the limit".into());
    }
    if url.scheme() == "data" {
        let (header, data) = url
            .as_str()
            .split_once(',')
            .ok_or("Invalid embedded image")?;
        if !header.starts_with("data:image/")
            || !header.ends_with(";base64")
            || data.len() > MAX_EMBEDDED_BASE64_BYTES
        {
            return Err("Unsupported embedded resource".into());
        }
        let bytes = STANDARD.decode(data).map_err(|error| error.to_string())?;
        let reader = ImageReader::new(Cursor::new(bytes.as_slice()))
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        if pixel_count(reader)? > MAX_IMAGE_PIXELS {
            return Err("Embedded image is too large".into());
        }
        return Ok(bytes);
    }
    if url.scheme() != "https" || url.host_str() != Some(BOOK_HOST) {
        return Err("Book resources must be inside the EPUB".into());
    }
    let bytes = book
        .read_resource_bytes(url.path())
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_RESOURCE_BYTES {
        return Err(format!(
            "EPUB resource exceeds {} MiB",
            MAX_RESOURCE_BYTES >> 20
        ));
    }
    if let Ok(reader) = ImageReader::new(Cursor::new(bytes.as_slice())).with_guessed_format()
        && reader.format().is_some()
    {
        if pixel_count(reader)? > MAX_IMAGE_PIXELS {
            return Err(format!(
                "EPUB image exceeds {} million pixels",
                MAX_IMAGE_PIXELS / 1_000_000
            ));
        }
        return Ok(bytes);
    }
    if bytes.starts_with(&GZIP_MAGIC) {
        return Err("Compressed SVG resources are not supported".into());
    }
    if is_svg(&bytes) {
        return markup(book, url, &bytes, depth);
    }
    Ok(bytes)
}

pub(crate) fn markup(
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
                let is_empty = matches!(event, Event::Empty(_));
                let svg = tag.local_name().as_ref() == "svg";
                if svg && !is_empty {
                    svg_depth += 1;
                }
                if svg_depth > 0 || svg {
                    let clean = rewrite_svg_tag(book, base, tag, depth)?;
                    let rewritten = if is_empty {
                        Event::Empty(clean)
                    } else {
                        Event::Start(clean)
                    };
                    writer
                        .write_event(rewritten)
                        .map_err(|error| error.to_string())?;
                    continue;
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == "svg" => {
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

fn pixel_count(reader: ImageReader<Cursor<&[u8]>>) -> Result<u64> {
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    Ok(u64::from(width) * u64::from(height))
}

fn rewrite_svg_tag<'a>(
    book: &rbook::Epub,
    base: &Url,
    tag: &'a BytesStart<'a>,
    depth: usize,
) -> Result<BytesStart<'a>> {
    let mut clean = BytesStart::new(tag.name().into_inner());
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.into_inner();
        if attribute.key.local_name().as_ref() == "href" {
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| error.to_string())?;
            // An unloadable reference becomes an empty `href`, so the rest of
            // the SVG still renders.
            let replacement = if value.starts_with('#') {
                value
            } else {
                Cow::Owned(embed(book, base, &value, depth + 1).unwrap_or_default())
            };
            clean.push_attribute((key, replacement.as_ref()));
        } else {
            clean.push_attribute(attribute);
        }
    }
    Ok(clean)
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
                return tag.local_name().as_ref() == "svg";
            }
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_)) => {}
            Ok(Event::Text(text)) if text.as_ref().trim_ascii().is_empty() => {}
            _ => return false,
        }
    }
}
