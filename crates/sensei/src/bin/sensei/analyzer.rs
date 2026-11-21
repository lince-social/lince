use std::{io, ops::Range, path::Path};

use sensei::rules::no_comments::comments_in;
use serde_json::{Value, json};

use super::lsp::{Client, file_uri};

const PATHS: &[&str] = &[
    "std::env::args",
    "std::env::args_os",
    "std::env::current_dir",
    "std::env::current_exe",
    "std::env::var_os",
    "std::fs::canonicalize",
    "std::fs::create_dir_all",
    "std::fs::read_to_string",
    "std::process::exit",
    "std::ptr::from_ref",
    "std::ptr::from_mut",
    "std::mem::take",
    "std::mem::replace",
    "std::mem::swap",
];

pub(super) fn shorten(
    client: &mut Client,
    path: &Path,
    source: &str,
) -> io::Result<Option<String>> {
    if !PATHS.iter().any(|path| source.contains(path)) {
        return Ok(None);
    }
    let uri = file_uri(path)?;
    let doc = json!({"uri":uri});
    client.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {"uri":uri,"languageId":"rust","version":1,"text":source}
        }),
    )?;
    let result = shorten_open(client, &uri, &doc, source);
    client.notify("textDocument/didClose", json!({"textDocument":doc}))?;
    result
}

fn shorten_open(
    client: &mut Client,
    uri: &str,
    doc: &Value,
    source: &str,
) -> io::Result<Option<String>> {
    let mut current = source.to_owned();
    let mut version = 1;
    for path in PATHS {
        let (module, name) = path
            .rsplit_once("::")
            .expect("standard function has a module");
        let found = names(&current, name);
        let qualified: Vec<_> = found
            .iter()
            .copied()
            .filter(|&at| {
                at.checked_sub(module.len() + 2)
                    .and_then(|start| current.get(start..at + name.len()))
                    == Some(path)
                    && current[at + name.len()..].trim_start().starts_with('(')
            })
            .collect();
        let may_add_import = qualified.len() == found.len();
        let Some(&first) = qualified.first() else {
            continue;
        };

        let mut before = Vec::with_capacity(qualified.len());
        for offset in qualified {
            before.push(definition(client, doc, &current, offset)?);
        }
        if !before.iter().all(sysroot_definition) {
            continue;
        }
        let at = position(&current, first + 1);
        let actions = client.call(
            "textDocument/codeAction",
            json!({
                "textDocument":doc, "range":{"start":at,"end":at},
                "context":{"diagnostics":[],"only":["refactor.rewrite"],"triggerKind":1}
            }),
        )?;
        let Some(edits) = actions.as_array().into_iter().flatten().find_map(|action| {
            if action["title"] != "Replace qualified path with use" {
                return None;
            }
            file_edits(&action["edit"], uri)
        }) else {
            continue;
        };
        let Some((candidate, after_offsets)) = replace(&current, edits, path, name, may_add_import)
            .or_else(|| {
                if may_add_import {
                    replace_with_new_import(&current, edits, path, name)
                } else {
                    None
                }
            })
        else {
            continue;
        };
        if after_offsets.len() != before.len() || size(&candidate) >= size(&current) {
            continue;
        }
        version += 1;
        change(client, uri, version, &candidate)?;
        let mut same = true;
        for (offset, expected) in after_offsets.into_iter().zip(before) {
            if definition(client, doc, &candidate, offset)? != expected {
                same = false;
                break;
            }
        }
        if same {
            current = candidate;
        } else {
            version += 1;
            change(client, uri, version, &current)?;
        }
    }
    Ok((current != source).then_some(current))
}

fn file_edits<'a>(edit: &'a Value, uri: &str) -> Option<&'a [Value]> {
    if let Some(changes) = edit["documentChanges"].as_array() {
        let [change] = changes.as_slice() else {
            return None;
        };
        if change["textDocument"]["uri"] == uri {
            return Some(change["edits"].as_array()?.as_slice());
        }
    } else if let Some(changes) = edit["changes"].as_object()
        && changes.len() == 1
    {
        return Some(changes.get(uri)?.as_array()?.as_slice());
    }
    None
}

fn replace(
    source: &str,
    edits: &[Value],
    path: &str,
    name: &str,
    may_add_import: bool,
) -> Option<(String, Vec<usize>)> {
    let edits = text_edits(source, edits)?;
    let comments = comments_in(source);
    let mut cursor = 0;
    let mut output = String::with_capacity(source.len());
    let mut offsets = Vec::new();
    let mut import = false;
    for (range, text) in edits {
        let old = source.get(range.clone())?;
        if range.start < cursor
            || comments
                .iter()
                .any(|comment| range.start < comment.end && comment.start < range.end)
        {
            return None;
        }
        output.push_str(&source[cursor..range.start]);
        if old == path && text == name {
            offsets.push(output.len());
        } else if may_add_import && !import && names(text, name).len() == 1 {
            // Accept a new private use or a merge into an existing private use.
            let prefix = source[..range.start].rsplit('\n').next()?.trim_start();
            if !(old.is_empty() && text.trim_start().starts_with("use ")
                || prefix.starts_with("use "))
            {
                return None;
            }
            import = true;
        } else {
            return None;
        }
        output.push_str(text);
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);
    (!offsets.is_empty()).then_some((output, offsets))
}

fn text_edits<'a>(source: &str, edits: &'a [Value]) -> Option<Vec<(Range<usize>, &'a str)>> {
    let mut edits = edits
        .iter()
        .map(|edit| {
            let range = &edit["range"];
            Some((
                offset(source, &range["start"])?..offset(source, &range["end"])?,
                edit["newText"].as_str()?,
            ))
        })
        .collect::<Option<Vec<(Range<usize>, &str)>>>()?;
    edits.sort_unstable_by_key(|(range, _)| range.start);
    Some(edits)
}

fn replace_with_new_import(
    source: &str,
    edits: &[Value],
    path: &str,
    name: &str,
) -> Option<(String, Vec<usize>)> {
    // The analyzer sometimes emits larger changes when it creates the first import.
    // Compare their full result to exactly the qualified call replacements and one use.
    let mut cursor = 0;
    let mut output = String::with_capacity(source.len());
    for (range, text) in text_edits(source, edits)? {
        if range.start < cursor || range.end < range.start {
            return None;
        }
        output.push_str(source.get(cursor..range.start)?);
        output.push_str(text);
        cursor = range.end;
    }
    output.push_str(source.get(cursor..)?);
    let (prefix, suffix) = output.split_once(&format!("use {path};"))?;
    let expected = source.replace(path, name);
    let rest = expected.strip_prefix(prefix.trim_end_matches(['\r', '\n']))?;
    if rest.trim_start_matches(['\r', '\n']) != suffix.trim_start_matches(['\r', '\n']) {
        return None;
    }
    let offsets = names(&output, name)
        .into_iter()
        .filter(|&at| output[at + name.len()..].trim_start().starts_with('('))
        .collect();
    Some((output, offsets))
}

fn names(source: &str, name: &str) -> Vec<usize> {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    source
        .match_indices(name)
        .filter_map(|(at, _)| {
            (!source[..at].chars().next_back().is_some_and(ident)
                && !source[at + name.len()..].chars().next().is_some_and(ident))
            .then_some(at)
        })
        .collect()
}

fn definition(client: &mut Client, doc: &Value, source: &str, byte: usize) -> io::Result<Value> {
    client.call(
        "textDocument/definition",
        json!({"textDocument":doc,"position":position(source, byte + 1)}),
    )
}

fn sysroot_definition(definition: &Value) -> bool {
    let location = match definition.as_array().map(Vec::as_slice) {
        Some([location]) => location,
        _ => return false,
    };
    location["uri"]
        .as_str()
        .is_some_and(|uri| uri.contains("/rustlib/src/rust/library/"))
}

fn change(client: &mut Client, uri: &str, version: usize, text: &str) -> io::Result<()> {
    client.notify(
        "textDocument/didChange",
        json!({
            "textDocument":{"uri":uri,"version":version}, "contentChanges":[{"text":text}]
        }),
    )
}

fn position(source: &str, byte: usize) -> Value {
    let prefix = &source[..byte];
    json!({"line":prefix.bytes().filter(|&b| b==b'\n').count(),
        "character":prefix.rsplit('\n').next().unwrap_or_default().len()})
}

fn offset(source: &str, position: &Value) -> Option<usize> {
    let line = usize::try_from(position["line"].as_u64()?).ok()?;
    let column = usize::try_from(position["character"].as_u64()?).ok()?;
    let mut lines = source.split('\n');
    let start = lines
        .by_ref()
        .take(line)
        .map(|line| line.len() + 1)
        .sum::<usize>();
    lines.next()?.get(..column)?;
    Some(start + column)
}

fn size(source: &str) -> usize {
    source.chars().filter(|ch| !ch.is_whitespace()).count()
}
