// non-lean-yet
// Scope: enum Content, struct Branch, struct Tree.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 120, 132, 135,
//   139, 149, 171.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (130, 146).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   50, 137.

use super::*;

pub(super) enum Content<'a> {
    Records(&'a [Value]),
    Files(Vec<&'a str>),
}

pub(super) struct Branch {
    pub title: String,
    pub members: Vec<usize>,
    pub children: Vec<String>,
}

pub(super) struct Tree<'a> {
    pub nodes: BTreeMap<String, Branch>,
    pub roots: Vec<String>,
    pub titles: BTreeMap<&'a str, &'a str>,
    pub content: Content<'a>,
}

impl<'a> Tree<'a> {
    pub(super) fn records(data: &'a Value) -> Self {
        let _prepare = nucleus::performance::Span::new("source-outline-preparation");
        let records = data["records"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let documents = data["documents"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let titles = records
            .iter()
            .filter_map(|record| Some((record["uid"].as_str()?, record["title"].as_str()?)))
            .collect();
        let mut members_by_base = BTreeMap::<&str, Vec<usize>>::new();
        for (index, record) in records.iter().enumerate() {
            if let Some(base) = record["base"].as_str() {
                members_by_base.entry(base).or_default().push(index);
            }
        }
        let mut nodes = BTreeMap::new();
        for document in documents {
            let Some(uri) = document["uri"].as_str() else {
                continue;
            };
            let members = members_by_base.get(uri).cloned().unwrap_or_default();
            let title = members
                .first()
                .and_then(|index| records[*index]["title"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| link_label(uri));
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
                    title,
                    members,
                    children,
                },
            );
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
        let _prepare = nucleus::performance::Span::new("source-outline-preparation");
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
                .or_insert_with(|| Branch {
                    title: link_label(uri),
                    members: Vec::new(),
                    children: Vec::new(),
                })
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
        if let Some(entry) = &entry {
            if !nodes.contains_key(entry) {
                let prefix = format!("{}/", entry.trim_end_matches('/'));
                let descendants = nodes
                    .keys()
                    .filter(|key| key.starts_with(&prefix))
                    .cloned()
                    .collect::<Vec<_>>();
                if !descendants.is_empty() {
                    nodes.insert(
                        entry.clone(),
                        Branch {
                            title: link_label(entry),
                            members: Vec::new(),
                            children: Vec::new(),
                        },
                    );
                    for key in descendants {
                        let relative = key
                            .strip_prefix(&prefix)
                            .expect("only paths below this prefix were collected");
                        let parts = relative.split('/').collect::<Vec<_>>();
                        let mut parent = entry.clone();
                        for (index, part) in parts.iter().enumerate() {
                            let child = if index + 1 == parts.len() {
                                key.clone()
                            } else {
                                format!("{}{}", prefix, parts[..=index].join("/"))
                            };
                            nodes.entry(child.clone()).or_insert_with(|| Branch {
                                title: part.to_string(),
                                members: Vec::new(),
                                children: Vec::new(),
                            });
                            let children = &mut nodes
                                .get_mut(&parent)
                                .expect("the parent branch was inserted before its child")
                                .children;
                            if !children.contains(&child) {
                                children.push(child.clone());
                            }
                            parent = child;
                        }
                    }
                }
            }
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
