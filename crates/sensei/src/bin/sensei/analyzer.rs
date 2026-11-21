use std::{borrow::Cow, io, ops::Range, path::Path};

use serde_json::{Value, from_str, json};

use super::lsp::{Client, file_uri};

pub(super) fn shorten(
    client: &mut Client,
    path: &Path,
    source: &str,
) -> io::Result<Option<String>> {
    if !source.contains("::") {
        return Ok(None);
    }
    let source = if source.contains("\r\n") {
        Cow::Owned(source.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(source)
    };
    let uri = file_uri(path)?;
    let doc = json!({"uri":uri});
    client.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {"uri":uri,"languageId":"rust","version":1,"text":source}
        }),
    )?;
    let result = shorten_open(client, &doc, &source);
    client.notify("textDocument/didClose", json!({"textDocument":doc}))?;
    result
}

fn shorten_open(client: &mut Client, doc: &Value, source: &str) -> io::Result<Option<String>> {
    let mut current = source.to_owned();
    let mut before = Syntax::read(client, doc, &current)?;
    let mut version = 1;
    for index in 0..before.paths.len() {
        let path = &before.paths[index];
        if !path.call || path.guarded || path.qualifier.is_empty() {
            continue;
        }
        let name = current
            .get(path.name.clone())
            .ok_or_else(|| io::Error::other("analyzer call position is outside the source"))?;
        let at = position(&current, path.name.start);
        let actions = client.call(
            "textDocument/codeAction",
            json!({
                "textDocument":doc, "range":{"start":at,"end":at},
                "context":{"diagnostics":[],"only":["refactor.rewrite"],"triggerKind":1}
            }),
        )?;
        let Some(edits) = actions
            .as_array()
            .into_iter()
            .flatten()
            .find(|action| action["title"] == "Replace qualified path with use")
            .and_then(|action| file_edits(&action["edit"], doc["uri"].as_str()?))
        else {
            continue;
        };
        let Some(candidate) = edited(&current, edits) else {
            continue;
        };

        version += 1;
        change(client, doc, version, &candidate)?;
        let after = Syntax::read(client, doc, &candidate)?;
        let rewrite = before.rewrite(&after, &current, &candidate, index, name);
        let accepted = if let Some(rewrite) = rewrite {
            // Record the candidate's resolutions before restoring the original document.
            let new = definitions(client, doc, &candidate, &rewrite.after, &rewrite.references)?;
            version += 1;
            change(client, doc, version, &current)?;
            let old = definitions(client, doc, &current, &rewrite.before, &rewrite.references)?;
            old == new
                && rewrite
                    .references
                    .iter()
                    .zip(&old)
                    .all(|(reference, definition)| !reference.shortened || definition.is_some())
        } else {
            version += 1;
            change(client, doc, version, &current)?;
            false
        };
        if accepted {
            version += 1;
            change(client, doc, version, &candidate)?;
            current = candidate;
            before = after;
        }
    }
    Ok((current != source).then_some(current))
}

#[derive(Default)]
struct Syntax {
    tokens: Vec<Token>,
    paths: Vec<ReferencePath>,
    bindings: Vec<Vec<Range<usize>>>,
}

struct Token {
    range: Range<usize>,
    name_ref: bool,
    comment: bool,
}

struct ReferencePath {
    qualifier: Range<usize>,
    name: Range<usize>,
    call: bool,
    guarded: bool,
}

struct Rewrite<'a> {
    before: Vec<&'a Token>,
    after: Vec<&'a Token>,
    references: Vec<Reference>,
}

struct Reference {
    token: usize,
    shortened: bool,
}

impl Syntax {
    fn read(client: &mut Client, doc: &Value, source: &str) -> io::Result<Self> {
        let tree = client.call("rust-analyzer/viewSyntaxTree", json!({"textDocument":doc}))?;
        let tree: Value = from_str(
            tree.as_str()
                .ok_or_else(|| io::Error::other("missing Rust syntax tree"))?,
        )
        .map_err(io::Error::other)?;
        if kind(&tree) != "SOURCE_FILE" || range(&tree) != Some(0..source.len()) {
            return Err(io::Error::other(
                "rust-analyzer returned a stale or invalid syntax tree",
            ));
        }
        let mut syntax = Self::default();
        syntax.visit(&tree, source, "", false, false);
        Ok(syntax)
    }

    fn visit(
        &mut self,
        node: &Value,
        source: &str,
        parent: &str,
        private_use: bool,
        guarded: bool,
    ) {
        let node_kind = kind(node);
        let private_use = private_use
            || node_kind == "USE"
                && child(node, "VISIBILITY").is_none()
                && child(node, "ATTR").is_none();

        if node["type"] == "Token" {
            let comment = node_kind == "COMMENT";
            if node_kind != "WHITESPACE"
                && (!private_use || comment)
                && let Some(range) = range(node)
            {
                self.tokens.push(Token {
                    range,
                    name_ref: parent == "NAME_REF",
                    comment,
                });
            }
            return;
        }

        // An import beside a conditional item can become active in other configurations.
        let guarded = guarded
            || children(node)
                .filter(|attr| kind(attr) == "ATTR")
                .any(|attr| {
                    child(attr, "CFG_META").is_some()
                        || child(attr, "CFG_ATTR_META").is_some()
                        || child(attr, "PATH_META")
                            .and_then(|meta| child(meta, "PATH"))
                            .and_then(path_name)
                            .and_then(|name| source.get(name))
                            == Some("test")
                });
        if node_kind == "PATH_EXPR"
            && let Some(path) = child(node, "PATH")
            && let Some(name) = path_name(path)
            && let Some(path) = range(path)
        {
            self.paths.push(ReferencePath {
                qualifier: path.start..name.start,
                name,
                call: parent == "CALL_EXPR",
                guarded,
            });
        }
        if matches!(node_kind, "SOURCE_FILE" | "ITEM_LIST" | "STMT_LIST") {
            let mut bindings = Vec::new();
            for item in children(node) {
                if let Some(tree) = child(item, "USE_TREE") {
                    use_bindings(tree, None, &mut bindings);
                } else if let Some(name) = child(item, "NAME").and_then(range) {
                    bindings.push(name);
                }
            }
            self.bindings.push(bindings);
        }
        for element in children(node) {
            self.visit(element, source, node_kind, private_use, guarded);
        }
    }

    fn rewrite<'a>(
        &'a self,
        after: &'a Self,
        source: &str,
        candidate: &str,
        selected: usize,
        name: &str,
    ) -> Option<Rewrite<'a>> {
        if self.paths.len() != after.paths.len()
            || self.bindings.len() != after.bindings.len()
            || !after.paths[selected].qualifier.is_empty()
        {
            return None;
        }
        let mut removed = Vec::new();
        let mut shortened = Vec::new();
        for (old, new) in self.paths.iter().zip(&after.paths) {
            if source.get(old.qualifier.clone())? == candidate.get(new.qualifier.clone())? {
                continue;
            }
            if old.guarded
                || old.qualifier.is_empty()
                || !new.qualifier.is_empty()
                || source.get(old.name.clone())? != name
            {
                return None;
            }
            removed.push(old.qualifier.clone());
            shortened.push(old.name.start);
        }

        // The assist owns the import edit. Reject a duplicate explicit binding in its scope.
        let count_bindings = |bindings: &[Range<usize>], text: &str| {
            bindings
                .iter()
                .filter(|range| {
                    text.get((*range).clone())
                        .is_some_and(|binding| spelling(binding) == spelling(name))
                })
                .count()
        };
        for (old, new) in self.bindings.iter().zip(&after.bindings) {
            let new = count_bindings(new, candidate);
            if new > 1 && new > count_bindings(old, source) {
                return None;
            }
        }

        // Outside private imports, the only allowed edits remove qualifiers from paths with
        // this name. Keep comments even when a qualifier or an import is rewritten.
        let old: Vec<_> = self
            .tokens
            .iter()
            .filter(|token| {
                token.comment
                    || !removed
                        .iter()
                        .any(|range| range.contains(&token.range.start))
            })
            .collect();
        let new: Vec<_> = after.tokens.iter().collect();
        if old.len() != new.len() {
            return None;
        }
        let mut references = Vec::new();
        for (token, (left, right)) in old.iter().zip(&new).enumerate() {
            let text = source.get(left.range.clone())?;
            if text != candidate.get(right.range.clone())? {
                return None;
            }
            if left.name_ref && spelling(text) == spelling(name) {
                if !right.name_ref {
                    return None;
                }
                references.push(Reference {
                    token,
                    shortened: shortened.contains(&left.range.start),
                });
            }
        }
        if references
            .iter()
            .filter(|reference| reference.shortened)
            .count()
            != shortened.len()
        {
            return None;
        }
        Some(Rewrite {
            before: old,
            after: new,
            references,
        })
    }
}

fn use_bindings(tree: &Value, prefix: Option<Range<usize>>, output: &mut Vec<Range<usize>>) {
    let name = child(tree, "PATH").and_then(path_name);
    if let Some(list) = child(tree, "USE_TREE_LIST") {
        for branch in children(list).filter(|branch| kind(branch) == "USE_TREE") {
            use_bindings(branch, name.clone().or_else(|| prefix.clone()), output);
        }
    } else if !children(tree).any(|token| kind(token) == "STAR") {
        if let Some(rename) = child(tree, "RENAME") {
            output.extend(child(rename, "NAME").and_then(range));
        } else if child(tree, "PATH")
            .and_then(|path| child(path, "PATH_SEGMENT"))
            .and_then(|segment| child(segment, "NAME_REF"))
            .is_some_and(|name| child(name, "SELF_KW").is_some())
        {
            output.extend(prefix.or(name));
        } else {
            output.extend(name);
        }
    }
}

fn path_name(path: &Value) -> Option<Range<usize>> {
    range(child(child(path, "PATH_SEGMENT")?, "NAME_REF")?)
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node["children"].as_array().into_iter().flatten()
}

fn child<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    children(node).find(|node| kind(node) == name)
}

fn kind(node: &Value) -> &str {
    node["kind"].as_str().unwrap_or_default()
}

fn range(node: &Value) -> Option<Range<usize>> {
    Some(
        usize::try_from(node["start"][0].as_u64()?).ok()?
            ..usize::try_from(node["end"][0].as_u64()?).ok()?,
    )
}

fn spelling(name: &str) -> &str {
    name.trim_start_matches("r#")
}

#[derive(PartialEq)]
enum Definition {
    Local { token: usize, offset: usize },
    External(Value),
}

fn definitions(
    client: &mut Client,
    doc: &Value,
    source: &str,
    tokens: &[&Token],
    references: &[Reference],
) -> io::Result<Vec<Option<Definition>>> {
    references.iter().map(|reference| {
        let value = client.call(
            "textDocument/definition",
            json!({"textDocument":doc,"position":position(source, tokens[reference.token].range.start)}),
        )?;
        let Some([location]) = value.as_array().map(Vec::as_slice) else {
            return Ok(None);
        };
        if location["uri"] != doc["uri"] {
            return Ok(Some(Definition::External(location.clone())));
        }
        // Import edits move definitions in the same file. Compare their carried token positions.
        let Some(at) = offset(source, &location["range"]["start"]) else {
            return Ok(None);
        };
        let index = tokens.partition_point(|token| token.range.end <= at);
        Ok(tokens.get(index).filter(|token| token.range.contains(&at)).map(|token| {
            Definition::Local { token: index, offset: at - token.range.start }
        }))
    }).collect()
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

fn edited(source: &str, edits: &[Value]) -> Option<String> {
    let mut edits = edits
        .iter()
        .map(|edit| {
            let range = &edit["range"];
            Some((
                offset(source, &range["start"])?..offset(source, &range["end"])?,
                edit["newText"].as_str()?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    edits.sort_by_key(|(range, _)| range.start);
    let mut cursor = 0;
    let mut output = String::with_capacity(source.len());
    for (range, text) in edits {
        source.get(range.clone())?;
        output.push_str(source.get(cursor..range.start)?);
        output.push_str(text);
        cursor = range.end;
    }
    output.push_str(source.get(cursor..)?);
    Some(output)
}

fn change(client: &mut Client, doc: &Value, version: usize, text: &str) -> io::Result<()> {
    client.notify(
        "textDocument/didChange",
        json!({
            "textDocument":{"uri":doc["uri"],"version":version},"contentChanges":[{"text":text}]
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
