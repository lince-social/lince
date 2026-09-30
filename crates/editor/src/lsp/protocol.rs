use crate::{Edit, Result};
use ropey::Rope;
use serde_json::{Value, json};
use std::{ops::Range, path::Path};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) async fn read(input: &mut (impl AsyncBufRead + Unpin)) -> Result<Value> {
    let mut length = None;
    let mut total = 0;
    loop {
        let mut header = Vec::new();
        let count = input
            .take(8193)
            .read_until(b'\n', &mut header)
            .await
            .map_err(|error| error.to_string())?;
        total += count;
        if count == 0 {
            return Err("Language server closed its output".into());
        }
        if total > 8192 {
            return Err("Language server header is too large".into());
        }
        if header == b"\r\n" {
            break;
        }
        let header = std::str::from_utf8(&header).map_err(|_| "Invalid language server header")?;
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                if length.is_some() {
                    return Err("Duplicate Content-Length".into());
                }
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "Invalid Content-Length")?,
                );
            }
        }
    }
    let length = length
        .filter(|length| *length <= 8 * 1024 * 1024)
        .ok_or("Missing or oversized language server message")?;
    let mut bytes = vec![0; length];
    input
        .read_exact(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

pub(super) async fn write(output: &mut (impl AsyncWrite + Unpin), message: Value) -> Result<()> {
    let bytes = serde_json::to_vec(&message).map_err(|error| error.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        output.write_all(header.as_bytes()).await?;
        output.write_all(&bytes).await?;
        output.flush().await
    })
    .await
    .map_err(|_| "Language server stopped reading requests")?
    .map_err(|error| error.to_string())
}

pub fn uri(path: &Path) -> Result<String> {
    url::Url::from_file_path(path)
        .map(String::from)
        .map_err(|_| "Choose an absolute file path".into())
}

pub fn position(rope: &Rope, offset: usize) -> Value {
    let offset = offset.min(rope.len_chars());
    let line = rope.char_to_line(offset);
    let character: usize = rope
        .slice(rope.line_to_char(line)..offset)
        .chars()
        .map(char::len_utf16)
        .sum();
    json!({"line": line, "character": character})
}

pub fn offset(rope: &Rope, position: &Value) -> Result<usize> {
    let line = position["line"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|line| *line < rope.len_lines())
        .ok_or("Tool returned an invalid line")?;
    let character = position["character"]
        .as_u64()
        .ok_or("Tool returned an invalid column")?;
    let mut units = 0;
    let start = rope.line_to_char(line);
    for (index, ch) in rope.line(line).chars().enumerate() {
        if units == character {
            return Ok(start + index);
        }
        if matches!(ch, '\r' | '\n') {
            break;
        }
        units += ch.len_utf16() as u64;
        if units > character {
            return Err("Tool returned a column inside a Unicode character".into());
        }
    }
    if units == character {
        Ok(start + rope.line(line).len_chars())
    } else {
        Err("Tool returned a column past the end of a line".into())
    }
}

pub fn range(rope: &Rope, value: &Value) -> Result<Range<usize>> {
    let start = offset(rope, &value["start"])?;
    let end = offset(rope, &value["end"])?;
    if start > end {
        return Err("Tool returned a reversed range".into());
    }
    Ok(start..end)
}

pub fn edits(rope: &Rope, value: &Value) -> Result<Vec<Edit>> {
    let values = value.as_array().ok_or("Tool returned invalid edits")?;
    if values.len() > 4096 {
        return Err("Tool returned too many edits".into());
    }
    let mut bytes = 0;
    let mut edits = Vec::new();
    for value in values {
        let text = value["newText"]
            .as_str()
            .ok_or("Tool returned an invalid text edit")?;
        bytes += text.len();
        if bytes > crate::MAX_FILE_BYTES || text.contains('\0') {
            return Err("Tool returned too much or invalid text".into());
        }
        edits.push(Edit {
            range: range(rope, &value["range"])?,
            text: text.into(),
        });
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    if edits.windows(2).any(|pair| {
        pair[0].range.end > pair[1].range.start || pair[0].range.start == pair[1].range.start
    }) {
        return Err("Tool returned overlapping edits".into());
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_edits_preserve_unicode_and_reject_invalid_ranges() {
        let rope = Rope::from_str("a🐈猫\r\nsecond\n");
        assert_eq!(position(&rope, 3), json!({"line":0,"character":4}));
        assert_eq!(offset(&rope, &json!({"line":0,"character":3})).unwrap(), 2);
        assert!(offset(&rope, &json!({"line":0,"character":2})).is_err());
        assert!(offset(&rope, &json!({"line":0,"character":8})).is_err());
        let edit = json!({"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"dog"});
        assert_eq!(edits(&rope, &json!([edit.clone()])).unwrap()[0].range, 1..2);
        assert!(edits(&rope, &json!([edit.clone(), edit])).is_err());
    }
    #[tokio::test]
    async fn framing_handles_unicode_and_rejects_unbounded_messages() {
        let (mut writer, reader) = tokio::io::duplex(4096);
        let message = json!({"method":"text","params":"猫"});
        write(&mut writer, message.clone()).await.unwrap();
        assert_eq!(
            read(&mut tokio::io::BufReader::new(reader)).await.unwrap(),
            message
        );
        assert!(
            read(&mut &b"Content-Length: 99999999\r\n\r\n"[..])
                .await
                .is_err()
        );
        assert!(
            read(&mut &b"Content-Length: 1\r\nContent-Length: 1\r\n\r\n0"[..])
                .await
                .is_err()
        );
    }
}
