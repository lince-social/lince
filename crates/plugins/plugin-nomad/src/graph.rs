// non-lean-yet
// Scope: const NAMESPACE, const MAX_NODES, fn digest, fn identity.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 172, 175, 236,
//   239, 243, 281, 304, 411, 416, 464, 492, 514, 525, 597, 612, 614, 615, 624, 625.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 36, 57.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites build: String
//   (270), imported: String (573), lingua: String (603).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   21, 93, 94, 174, 226, 325, 327, 492, 551, 610, 644.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (20, 21, 192, 243, 244, 245, 263, 295, 399, 414, 441, 443, 502, 505, 538, 575,
//   593).

use crate::{schema::*, settings::Settings};
use anicca::{ProjectedAssertion, ProjectedRecord};
use nucleus::plugins::ImportedRecord;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const NAMESPACE: &str = "plugin.lince.nomad.source";
pub const MAX_NODES: usize = 5000;

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn identity(scope: &str, kind: &str, key: &str) -> String {
    let hash = Sha256::digest(serde_json::to_vec(&("lince.nomad.v1", scope, kind, key)).unwrap());
    let bits = u128::from_be_bytes(hash[..16].try_into().unwrap());
    format!("r_{}", nucleus::ulid_from((bits >> 80) as u64, bits))
}

fn text(value: &str, name: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        Err(format!(
            "NOMAD {name} must contain 1–{max} bytes without NUL"
        ))
    } else {
        Ok(())
    }
}

fn source_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "NOMAD resource has an invalid URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.len() > 8192
    {
        return Err("NOMAD sources need HTTP(S) URLs without credentials".into());
    }
    Ok(url)
}

fn filename(url: &str) -> Result<String, String> {
    let url = source_url(url)?;
    let segment = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("");
    percent_encoding::percent_decode_str(segment)
        .decode_utf8()
        .map(|v| v.into_owned())
        .map_err(|_| "NOMAD resource filename is not UTF-8".into())
}

fn check_file(file: &FileEntry, extension: &str) -> Result<(), String> {
    text(&file.name, "filename", 1024)?;
    if file.kind != "file"
        || !file.name.ends_with(extension)
        || file.name.contains(['/', '\\'])
        || file.name.chars().any(char::is_control)
    {
        return Err(format!("NOMAD returned an invalid {extension} file entry"));
    }
    Ok(())
}

fn lingua_title(title: &str) -> String {
    // Lingua's TitleWord has no escape for its structural ASCII delimiters.
    // Only the printed decoration changes; the exact title stays in metadata.
    title
        .chars()
        .map(|ch| match ch {
            '(' => '（',
            ')' => '）',
            '{' => '｛',
            '}' => '｝',
            '^' => 'ˆ',
            other => other,
        })
        .collect()
}

fn archive_resource_id(name: &str) -> &str {
    let stem = name.strip_suffix(".zim").unwrap_or(name);
    if let Some((id, version)) = stem.rsplit_once('_')
        && version.len() == 7
        && version.as_bytes()[4] == b'-'
        && version[..4].bytes().all(|b| b.is_ascii_digit())
        && version[5..]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
    {
        return id;
    }
    stem
}

fn media_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "txt" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Relation {
    pub predicate: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub uid: String,
    pub slug: String,
    pub kind: String,
    pub remote_id: String,
    pub title: String,
    pub description: String,
    pub source_url: String,
    pub data: Value,
    pub relations: BTreeSet<Relation>,
    pub resource: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Graph {
    pub scope: String,
    pub root: String,
    pub nodes: BTreeMap<String, Node>,
}

impl Graph {
    fn add(
        &mut self,
        kind: &str,
        key: &str,
        title: &str,
        description: &str,
        url: &str,
        data: Value,
    ) -> Result<String, String> {
        text(key, "source identity", 4096)?;
        text(title, "title", 1024)?;
        if description.len() > 12000 || description.contains('\0') {
            return Err("NOMAD description exceeds 12000 bytes or contains NUL".into());
        }
        let uid = identity(&self.scope, kind, key);
        if self.nodes.contains_key(&uid) {
            return Err(format!("Duplicate NOMAD {kind} identity: {key}"));
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(format!("NOMAD snapshot exceeds {MAX_NODES} Records"));
        }
        self.nodes.insert(
            uid.clone(),
            Node {
                slug: format!("nomad-{}", uid[2..].to_ascii_lowercase()),
                uid: uid.clone(),
                kind: kind.into(),
                remote_id: key.into(),
                title: title.into(),
                description: description.into(),
                source_url: url.into(),
                data,
                relations: BTreeSet::new(),
                resource: None,
            },
        );
        Ok(uid)
    }

    fn link(&mut self, subject: &str, predicate: &str, target: &str) {
        self.nodes
            .get_mut(subject)
            .unwrap()
            .relations
            .insert(Relation {
                predicate: predicate.into(),
                target: target.into(),
            });
    }

    fn resource(&mut self, resource: &Resource, kind: &str) -> Result<String, String> {
        text(&resource.id, "resource identity", 4096)?;
        text(&resource.title, "resource title", 1024)?;
        if resource.description.len() > 12000 || resource.description.contains('\0') {
            return Err("NOMAD resource description exceeds 12000 bytes or contains NUL".into());
        }
        if !resource.size_mb.is_finite() || resource.size_mb < 0.0 {
            return Err("NOMAD resource size must be finite and nonnegative".into());
        }
        text(&resource.version, "resource version", 160)?;
        source_url(&resource.url)?;
        let kind = match (kind, resource.kind.as_deref()) {
            ("map", _) => "map",
            (_, None | Some("zim")) => "zim",
            (_, Some("dataset")) => "dataset",
            _ => return Err("Unknown NOMAD catalog resource type".into()),
        };
        let key = format!("{kind}:{}", resource.id);
        let uid = identity(&self.scope, "resource", &key);
        let data = serde_json::to_value(resource).map_err(|e| e.to_string())?;
        if let Some(existing) = self.nodes.get_mut(&uid) {
            // NOMAD itself repeats the same download with different contextual
            // descriptions and rounded size estimates. Preserve those variants
            // under one source identity; incompatible downloads still fail.
            if ["id", "version", "url", "auth"]
                .iter()
                .any(|field| existing.data[field] != data[field])
            {
                return Err(format!(
                    "Conflicting definitions of NOMAD resource {}",
                    resource.id
                ));
            }
            let mut variants = BTreeMap::new();
            if let Some(previous) = existing.data["catalog_variants"].as_array() {
                for variant in previous {
                    variants.insert(variant.to_string(), variant.clone());
                }
            } else {
                variants.insert(existing.data.to_string(), existing.data.clone());
            }
            variants.insert(data.to_string(), data);
            // Choosing by canonical serialized value makes source order inert.
            let selected = variants.first_key_value().unwrap().1.clone();
            existing.title = selected["title"].as_str().unwrap().to_owned();
            existing.description = selected["description"].as_str().unwrap().to_owned();
            existing.data = selected;
            if variants.len() > 1 {
                existing.data["catalog_variants"] = Value::Array(variants.into_values().collect());
            }
            if let Some(representation) = &mut existing.resource {
                representation["title"] = json!(existing.title);
            }
            return Ok(uid);
        }
        let uid = self.add(
            "resource",
            &key,
            &resource.title,
            &resource.description,
            &resource.url,
            data,
        )?;
        self.nodes.get_mut(&uid).unwrap().resource = Some(json!({
            "uri":resource.url, "source_uri":resource.url, "title":resource.title,
            "media_type": match kind { "zim" => "application/x-zim", "map" => "application/vnd.pmtiles", _ => "application/octet-stream" }
        }));
        Ok(uid)
    }

    pub fn build(settings: &Settings, snapshot: Snapshot) -> Result<Self, String> {
        settings.validate()?;
        let scope = settings.scope()?;
        let mut graph = Self {
            scope,
            root: String::new(),
            nodes: BTreeMap::new(),
        };
        let root = graph.add("instance", "root", &format!("NOMAD — {}", graph.scope),
            "Offline source catalog. Bundle membership describes available material; learning routes and demonstrated progress are authored separately.",
            settings.url("")?.as_str(), json!({"base_url":settings.url("")?.as_str()}))?;
        graph.root = root.clone();

        for category in snapshot.categories {
            let cat = graph.add(
                "category",
                &category.slug,
                &category.name,
                &category.description,
                settings.url("settings/zim")?.as_str(),
                json!({"language":category.language}),
            )?;
            graph.link(&root, "contains", &cat);
            let mut tiers = BTreeMap::new();
            for tier in &category.tiers {
                let key = serde_json::to_string(&(&category.slug, &tier.slug)).unwrap();
                let id = graph.add(
                    "tier",
                    &key,
                    &format!("{} — {}", category.name, tier.name),
                    &tier.description,
                    settings.url("settings/zim")?.as_str(),
                    json!({"language":category.language}),
                )?;
                if tiers.insert(tier.slug.clone(), id.clone()).is_some() {
                    return Err(format!("Duplicate tier {}", tier.slug));
                }
                graph.link(&cat, "contains", &id);
            }
            let parents: BTreeMap<_, _> = category
                .tiers
                .iter()
                .map(|t| (t.slug.as_str(), t.includes_tier.as_deref()))
                .collect();
            for tier in &category.tiers {
                let mut seen = BTreeSet::new();
                let mut current = Some(tier.slug.as_str());
                while let Some(slug) = current {
                    if !seen.insert(slug) {
                        return Err(format!("Cyclic includesTier in {}", category.slug));
                    }
                    current = *parents
                        .get(slug)
                        .ok_or_else(|| format!("Missing inherited tier {slug}"))?;
                }
                let id = &tiers[&tier.slug];
                if let Some(parent) = &tier.includes_tier {
                    graph.link(id, "nomad-includes-bundle", &tiers[parent]);
                }
                for resource in &tier.resources {
                    let res = graph.resource(resource, "zim")?;
                    graph.link(id, "contains", &res);
                }
            }
        }

        if settings.include_wikipedia {
            let group = graph.add(
                "catalog",
                "wikipedia",
                "Wikipedia editions",
                "Available Wikipedia archive editions.",
                settings.url("settings/zim")?.as_str(),
                json!({}),
            )?;
            graph.link(&root, "contains", &group);
            for option in snapshot.wikipedia {
                if let Some(url) = option.url {
                    let res = graph.resource(
                        &Resource {
                            id: format!("wikipedia:{}", option.id),
                            version: option.version.ok_or("Wikipedia edition has no version")?,
                            title: option.name,
                            description: option.description,
                            url,
                            size_mb: option.size_mb,
                            kind: None,
                            auth: None,
                        },
                        "zim",
                    )?;
                    graph.link(&group, "contains", &res);
                }
            }
        }

        if settings.include_maps {
            let group = graph.add(
                "catalog",
                "maps",
                "Offline maps",
                "Map bundles and locally installed map files.",
                settings.url("maps")?.as_str(),
                json!({}),
            )?;
            graph.link(&root, "contains", &group);
            for collection in snapshot.map_collections {
                let col = graph.add(
                    "map-collection",
                    &collection.slug,
                    &collection.name,
                    &collection.description,
                    settings.url("maps")?.as_str(),
                    json!({"language":collection.language}),
                )?;
                graph.link(&group, "contains", &col);
                for resource in collection.resources {
                    let res = graph.resource(&resource, "map")?;
                    graph.link(&col, "contains", &res);
                }
            }
            for file in snapshot.maps {
                check_file(&file, ".pmtiles")?;
                let id = graph.add(
                    "installed-map",
                    &file.name,
                    &file.name,
                    "Locally installed PMTiles map.",
                    settings.url("maps")?.as_str(),
                    serde_json::to_value(&file).unwrap(),
                )?;
                graph.link(&group, "contains", &id);
            }
        }

        let mut by_filename = BTreeMap::<String, Vec<String>>::new();
        let mut by_resource_id = BTreeMap::<String, Vec<String>>::new();
        for node in graph.nodes.values().filter(|n| n.kind == "resource") {
            by_filename
                .entry(filename(&node.source_url)?)
                .or_default()
                .push(node.uid.clone());
            if node.remote_id.starts_with("zim:") {
                by_resource_id
                    .entry(node.data["id"].as_str().unwrap().to_owned())
                    .or_default()
                    .push(node.uid.clone());
            }
        }
        let library = graph.add(
            "library",
            "archives",
            "Installed archives",
            "ZIM archives present on this NOMAD.",
            settings.url("settings/zim")?.as_str(),
            json!({}),
        )?;
        graph.link(&root, "contains", &library);
        let mut archives = BTreeMap::<String, String>::new();
        for file in snapshot.archives {
            check_file(&file, ".zim")?;
            let url = settings.archive_url(&file.name)?;
            let id = graph.add(
                "archive",
                &file.name,
                file.title
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(&file.name),
                file.summary.as_deref().unwrap_or(""),
                &url,
                serde_json::to_value(&file).unwrap(),
            )?;
            graph.nodes.get_mut(&id).unwrap().resource = Some(json!({
                "uri":url, "title":file.title.as_deref().unwrap_or(&file.name), "media_type":"text/html",
                "attribution":file.author.as_deref().unwrap_or("")
            }));
            graph.link(&library, "contains", &id);
            if let Some(resources) = by_filename
                .get(&file.name)
                .or_else(|| by_resource_id.get(archive_resource_id(&file.name)))
            {
                for target in resources {
                    graph.link(&id, "source-reference", target);
                }
            }
            archives.insert(file.name, id);
        }
        // Map filenames and archive filenames can be shared by multiple catalog
        // references; preserve every match rather than inventing reverse edges.
        let map_links: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| n.kind == "installed-map")
            .map(|n| (n.uid.clone(), n.remote_id.clone()))
            .collect();
        for (id, name) in map_links {
            if let Some(targets) = by_filename.get(&name) {
                for target in targets {
                    graph.link(&id, "source-reference", target);
                }
            }
        }

        if settings.include_documents {
            let group = graph.add(
                "library",
                "documents",
                "Knowledge-base files",
                "Files reported by NOMAD's knowledge-base backend.",
                settings.url("chat?knowledge_base=true")?.as_str(),
                json!({}),
            )?;
            graph.link(&root, "contains", &group);
            let mut collections = BTreeMap::<String, String>::new();
            for file in snapshot.documents {
                text(&file.source, "document source", 4096)?;
                let mut url = settings.url("api/rag/files/download")?;
                url.query_pairs_mut().append_pair("source", &file.source);
                let url = if file.is_user_upload {
                    url.to_string()
                } else if let Some(archive) = archives.get(&file.file_name) {
                    graph.nodes[archive].source_url.clone()
                } else {
                    settings.url("chat?knowledge_base=true")?.to_string()
                };
                let id = graph.add(
                    "document",
                    &file.source,
                    &file.file_name,
                    "Source file; indexing state does not establish learning progress.",
                    &url,
                    serde_json::to_value(&file).unwrap(),
                )?;
                if file.is_user_upload {
                    graph.nodes.get_mut(&id).unwrap().resource = Some(
                        json!({"uri":url,"title":file.file_name,"media_type":media_type(&file.file_name)}),
                    );
                }
                if let Some(archive) = archives.get(&file.file_name) {
                    graph.link(&id, "source-reference", archive);
                }
                if let Some(collection) = file.collection.filter(|s| !s.is_empty()) {
                    let col = if let Some(id) = collections.get(&collection) {
                        id.clone()
                    } else {
                        let id = graph.add(
                            "document-collection",
                            &collection,
                            &collection,
                            "NOMAD knowledge-base collection.",
                            settings.url("chat?knowledge_base=true")?.as_str(),
                            json!({}),
                        )?;
                        graph.link(&group, "contains", &id);
                        collections.insert(collection, id.clone());
                        id
                    };
                    graph.link(&col, "contains", &id);
                } else {
                    graph.link(&group, "contains", &id);
                }
            }
        }
        Ok(graph)
    }

    pub fn fingerprint(&self) -> String {
        digest(&serde_json::to_vec(self).unwrap())
    }

    fn metadata(&self, node: &Node) -> Value {
        json!({"identity":node.uid,"kind":node.kind,"original_title":node.title,"source_url":node.source_url,
            "data":node.data,"relations":node.relations})
    }

    fn body(&self, node: &Node, linked: bool) -> String {
        let mut body = format!("{}\n\nSource: <{}>", node.description, node.source_url);
        if linked && !node.relations.is_empty() {
            body.push_str("\n\nConnected sources:\n");
            for relation in &node.relations {
                let target = &self.nodes[&relation.target];
                let label: String = target
                    .title
                    .chars()
                    .map(|ch| {
                        if "[]|".contains(ch) || ch.is_control() {
                            ' '
                        } else {
                            ch
                        }
                    })
                    .collect();
                let link = format!("- {}: [[{}|{}]]\n", relation.predicate, label, target.uid);
                if body.len() + link.len() > 16000 {
                    break;
                }
                body.push_str(&link);
            }
        }
        body
    }

    pub fn imported(&self, node: &Node) -> Result<ImportedRecord, String> {
        let metadata = self.metadata(node);
        if serde_json::to_vec(&metadata).unwrap().len() > 65536 {
            return Err(format!(
                "{} exceeds the plugin metadata budget; use connected export",
                node.title
            ));
        }
        let body = self.body(node, false);
        if body.len() > 16384 {
            return Err(format!(
                "{} exceeds the plugin text budget; use connected export",
                node.title
            ));
        }
        // Source snapshots are immutable under the current plugin host. The
        // canonical identity remains in metadata; a changed source is a new
        // retained version, never a conflict or overwrite of a learner's notes.
        let hash = digest(
            &serde_json::to_vec(&json!({"head":node.title,"body":body,"metadata":metadata}))
                .unwrap(),
        );
        Ok(ImportedRecord {
            external_id: format!("{}:{hash}", node.uid),
            head: node.title.clone(),
            body,
            metadata,
        })
    }

    pub fn lingua(&self) -> Result<String, String> {
        let mut output = String::new();
        for node in self.nodes.values() {
            let assertions = node
                .relations
                .iter()
                .map(|relation| {
                    let target = &self.nodes[&relation.target];
                    ProjectedAssertion {
                        predicate: relation.predicate.clone(),
                        identity: false,
                        object_slug: Some(target.slug.clone()),
                        object_uid: Some(target.uid.clone()),
                        object_uri: None,
                        quantity: None,
                        unit: None,
                    }
                })
                .collect();
            output.push_str(
                &anicca::render_record(&ProjectedRecord {
                    uid: node.uid.clone(),
                    slug: Some(node.slug.clone()),
                    head: lingua_title(&node.title),
                    body: self.body(node, true),
                    quantity: Some(("0".into(), None)),
                    assertions,
                })
                .map_err(|e| format!("Cannot render NOMAD source {:?}: {e}", node.title))?,
            );
            output.push_str(
                &anicca::extension::render(&node.slug, NAMESPACE, &self.metadata(node))
                    .map_err(|e| e.to_string())?,
            );
            if let Some(resource) = &node.resource {
                output.push_str(
                    &anicca::extension::render(&node.slug, "lince.resource", resource)
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        let root = &self.nodes[&self.root];
        output.push_str(&anicca::extension::render(&root.slug, "lince.vocabulary", &json!({"definitions": {
            "nomad-includes-bundle":"This download bundle also includes the target bundle's resources. This does not assert a learning prerequisite.",
            "source-reference":"The source represented here is described by, or available through, the target source Record."
        }})).map_err(|e| e.to_string())?);
        if output.len() > 16 * 1024 * 1024 {
            return Err("Connected export exceeds Lingua's 16 MiB limit".into());
        }
        anicca::parse(&output).map_err(|e| e.to_string())?;
        Ok(output)
    }
}
