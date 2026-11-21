// non-lean-yet
// Scope: struct Requirement, fn discover, fn discover_in, fn nodes.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 13.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 7,
//   8, 9, 10, 13.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   126.

use std::{collections::BTreeSet, path::Path};

use rust_sitter::tree_sitter::Node;

use crate::{Diagnostic, ast, diagnostic, parse_with_tree};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Requirement {
    pub id: String,
    pub record_uid: String,
    pub line: usize,
    pub hidden: bool,
}

pub fn discover(path: &Path, source: &str) -> Result<Vec<Requirement>, Diagnostic> {
    discover_in(source).map_err(|mut error| {
        error.path = Some(path.to_path_buf());
        error
    })
}

fn discover_in(source: &str) -> Result<Vec<Requirement>, Diagnostic> {
    let (document, tree) = parse_with_tree(source)?;
    let mut locations = Vec::new();
    nodes(tree.root_node(), "Record", &mut locations);
    let records = document.declarations.iter().filter_map(|declaration| {
        if let ast::Declaration::Record(record) = declaration {
            Some(record)
        } else {
            None
        }
    });
    let mut requirements = Vec::new();
    let mut locations = locations.into_iter();
    for record in records {
        let location = locations
            .next()
            .ok_or_else(|| diagnostic("Missing Record source location"))?;
        let header = location
            .child_by_field_name("header")
            .ok_or_else(|| diagnostic("Missing Record header source location"))?;
        let mut fields = Vec::new();
        nodes(header, "RecordHeaderRest", &mut fields);
        if fields.len() != record.header.rest.len() {
            return Err(diagnostic("Record header and source locations disagree"));
        }
        for (field, location) in record.header.rest.iter().zip(fields) {
            if let Some(comment) = &field.comment {
                let at = location
                    .child_by_field_name("comment")
                    .ok_or_else(|| diagnostic("Missing Record comment source location"))?;
                let comment = comment
                    .0
                    .strip_prefix("//")
                    .ok_or_else(|| diagnostic("Invalid Record header comment"))?;
                add(
                    &mut requirements,
                    record,
                    comment,
                    at.start_position().row + 1,
                    true,
                )?;
            }
        }
        if let Some(description) = &record.description {
            let at = location
                .child_by_field_name("description")
                .ok_or_else(|| diagnostic("Missing Record description source location"))?;
            let mut fence = None;
            for (offset, line) in description.0.lines().enumerate() {
                let trimmed = line.trim_start_matches([' ', '\t']);
                if let Some((marker, length, rest)) = fence_marker(trimmed) {
                    if let Some((open, width)) = fence {
                        if marker == open && length >= width && rest.trim().is_empty() {
                            fence = None;
                        }
                    } else if marker != b'`' || !rest.contains('`') {
                        fence = Some((marker, length));
                    }
                    continue;
                }
                if fence.is_none() {
                    add(
                        &mut requirements,
                        record,
                        trimmed,
                        at.start_position().row + offset + 2,
                        false,
                    )?;
                }
            }
        }
    }
    if locations.next().is_some() {
        return Err(diagnostic(
            "Record source location has no matching declaration",
        ));
    }
    let mut seen = BTreeSet::new();
    for requirement in &requirements {
        if !seen.insert(requirement.id.as_str()) {
            return Err(diagnostic(format!(
                "Duplicate requirement [{}] at line {}",
                requirement.id, requirement.line
            )));
        }
    }
    Ok(requirements)
}

fn nodes<'tree>(node: Node<'tree>, kind: &str, output: &mut Vec<Node<'tree>>) {
    if node.kind() == kind {
        output.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        nodes(child, kind, output);
    }
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str)> {
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let length = line.bytes().take_while(|&byte| byte == marker).count();
    (length >= 3).then(|| (marker, length, &line[length..]))
}

fn add(
    output: &mut Vec<Requirement>,
    record: &ast::Record,
    line: &str,
    number: usize,
    hidden: bool,
) -> Result<(), Diagnostic> {
    let Some(body) = line.trim_start_matches([' ', '\t']).strip_prefix("|>") else {
        return Ok(());
    };
    let (id, text) = body
        .trim_start()
        .strip_prefix('[')
        .and_then(|body| body.split_once(']'))
        .ok_or_else(|| diagnostic(format!("Missing requirement identity at line {number}")))?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || text.trim().is_empty()
    {
        return Err(diagnostic(format!(
            "Invalid requirement [{id}] at line {number}"
        )));
    }
    let uid = record.opening.uid().ok_or_else(|| {
        diagnostic(format!(
            "Requirement [{id}] at line {number} needs a stable Record UID"
        ))
    })?;
    output.push(Requirement {
        id: id.into(),
        record_uid: uid.into(),
        line: number,
        hidden,
    });
    Ok(())
}
