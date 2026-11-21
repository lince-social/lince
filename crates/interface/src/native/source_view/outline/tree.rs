//! Record and file trees for the Source View outline.

use super::{bare, link_label};
use nucleus::performance::Span;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ops::Bound::{Included, Unbounded},
};

pub(super) enum Content<'a> {
    Records(&'a [Value]),
    Files(Vec<&'a str>),
}

pub(super) struct Branch {
    pub title: String,
    pub members: Vec<usize>,
    pub children: Vec<String>,
}

impl Branch {
    fn new(title: String) -> Self {
        Self {
            title,
            members: Vec::new(),
            children: Vec::new(),
        }
    }
}

pub(super) struct Tree<'a> {
    pub nodes: BTreeMap<String, Branch>,
    pub roots: Vec<String>,
    pub titles: BTreeMap<&'a str, &'a str>,
    pub content: Content<'a>,
}

impl<'a> Tree<'a> {
    pub(super) fn records(data: &'a Value) -> Self {
        let _prepare = Span::new("source-outline-preparation");
        let records = data["records"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let documents = data["documents"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut nodes = BTreeMap::<String, Branch>::new();
        for document in documents {
            let Some(uri) = document["uri"].as_str() else {
                continue;
            };
            let children = document["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            nodes.insert(
                uri.into(),
                Branch {
                    title: String::new(),
                    members: Vec::new(),
                    children,
                },
            );
        }
        let mut titles = BTreeMap::new();
        for (index, record) in records.iter().enumerate() {
            if let (Some(uid), Some(title)) = (record["uid"].as_str(), record["title"].as_str()) {
                titles.insert(uid, title);
            }
            if let Some(branch) = record["base"].as_str().and_then(|base| nodes.get_mut(base)) {
                branch.members.push(index);
            }
        }
        for (uri, branch) in &mut nodes {
            branch.title = branch
                .members
                .first()
                .and_then(|&index| records.get(index))
                .and_then(|record| record["title"].as_str())
                .map_or_else(|| link_label(uri), str::to_owned);
        }
        let entry = data["entry"].as_str().map(bare).map(|entry| {
            documents
                .iter()
                .find(|document| document["source_uri"] == entry || document["uri"] == entry)
                .and_then(|document| document["uri"].as_str())
                .map(str::to_owned)
                .unwrap_or(entry)
        });
        Self::new(nodes, entry, titles, Content::Records(records))
    }

    pub(super) fn files(data: &'a Value) -> Self {
        let _prepare = Span::new("source-outline-preparation");
        let files: Vec<_> = data["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut nodes = BTreeMap::<String, Branch>::new();
        for (index, &uri) in files.iter().enumerate() {
            nodes
                .entry(uri.into())
                .or_insert_with(|| Branch::new(link_label(uri)))
                .members
                .push(index);
        }
        let entry = Some(bare(data["entry"].as_str().unwrap_or_default()));
        Self::new(nodes, entry, BTreeMap::new(), Content::Files(files))
    }

    fn new(
        mut nodes: BTreeMap<String, Branch>,
        entry: Option<String>,
        titles: BTreeMap<&'a str, &'a str>,
        content: Content<'a>,
    ) -> Self {
        if let Some(entry) = entry.as_deref()
            && !nodes.contains_key(entry)
        {
            add_directories(&mut nodes, entry);
        }
        let roots = if let Some(entry) = entry.filter(|entry| nodes.contains_key(entry)) {
            vec![entry]
        } else if !nodes.is_empty() {
            nodes.keys().cloned().collect()
        } else if let Content::Records(records) = &content {
            records
                .iter()
                .enumerate()
                .map(|(index, record)| {
                    let key = record["uid"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("record-{index}"));
                    nodes.insert(
                        key.clone(),
                        Branch {
                            title: record["title"].as_str().unwrap_or("Record").into(),
                            members: vec![index],
                            children: Vec::new(),
                        },
                    );
                    key
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            nodes,
            roots,
            titles,
            content,
        }
    }
}

fn add_directories(nodes: &mut BTreeMap<String, Branch>, entry: &str) {
    let prefix = format!("{}/", entry.trim_end_matches('/'));
    let descendants: Vec<_> = nodes
        .range::<str, _>((Included(prefix.as_str()), Unbounded))
        .map(|(path, _)| path)
        .take_while(|path| path.starts_with(&prefix))
        .cloned()
        .collect();
    if descendants.is_empty() {
        return;
    }
    nodes.insert(entry.into(), Branch::new(link_label(entry)));
    for key in descendants {
        let Some(relative) = key.strip_prefix(&prefix) else {
            continue;
        };
        let mut parent = entry;
        let mut end = prefix.len();
        for part in relative.split('/') {
            // Parts and separators come from this key, so each prefix ends on a character boundary.
            end += part.len();
            let child = &key[..end];
            if !nodes.contains_key(child) {
                nodes.insert(child.into(), Branch::new(part.into()));
            }
            if let Some(branch) = nodes.get_mut(parent)
                && !branch.children.iter().any(|path| path == child)
            {
                branch.children.push(child.into());
            }
            parent = child;
            end += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_keep_aliases_first_member_titles_and_last_document_children() {
        let alias = "https://example.invalid/alias";
        let path = "https://example.invalid/first-document.lingua";
        let child = "https://example.invalid/child.lingua";
        let data = json!({
            "entry": format!("{alias}#section"),
            "records": [
                {"uid": "same", "base": path},
                {"uid": "same", "base": path, "title": "Second member"},
                {"uid": "orphan", "base": "unknown", "title": "Orphan"},
            ],
            "documents": [
                {"source_uri": alias, "uri": path, "children": ["discarded"]},
                {"uri": path, "children": [child, path, child, false]},
                {"uri": child},
            ],
        });
        let tree = Tree::records(&data);
        assert_eq!(tree.roots, [path]);
        assert_eq!(tree.nodes[path].members, [0, 1]);
        assert_eq!(tree.nodes[path].title, "first document");
        assert_eq!(tree.nodes[path].children, [child, path, child]);
        assert_eq!(tree.titles.get("same"), Some(&"Second member"));
        assert_eq!(tree.titles.get("orphan"), Some(&"Orphan"));
        assert!(matches!(tree.content, Content::Records(records) if records.len() == 3));
    }

    #[test]
    fn files_keep_duplicate_members_and_unicode_folders_in_path_order() {
        let root = "https://example.invalid/source/";
        let prefix = format!("{root}école");
        let first = format!("{prefix}/a.lingua");
        let second = format!("{prefix}/z.lingua");
        let detached = "https://example.invalid/unrelated.lingua";
        let data = json!({"entry": root, "files": [second, first, second, detached]});
        let tree = Tree::files(&data);
        assert_eq!(tree.roots, [root]);
        assert_eq!(tree.nodes[root].children, [prefix.as_str()]);
        assert_eq!(
            tree.nodes[&prefix].children,
            [first.as_str(), second.as_str()]
        );
        assert_eq!(tree.nodes[&prefix].title, "école");
        assert_eq!(tree.nodes[&second].members, [0, 2]);
        assert!(tree.nodes.contains_key(detached));
        assert!(matches!(tree.content, Content::Files(files) if files.len() == 4));
    }
}
