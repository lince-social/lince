use super::{EngineError, invalid, wikimedia};
use nucleus::knowledge::{Archive, ITEM_LIMIT, Source};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn parse(format: &str, raw: &str, language: &str) -> Result<Archive, EngineError> {
    if raw.len() > ITEM_LIMIT {
        return Err(invalid(
            "Import an uncompressed dump selection of at most 4 MiB",
        ));
    }
    wikimedia::Wikimedia::from_source(&format!("wikipedia:{language}"))?
        .ok_or_else(|| invalid("Choose a source language"))?;
    let archive = match format {
        "wikidata_json" => wikidata(raw, language)?,
        "wikipedia_xml" => wikipedia(raw, language)?,
        _ => {
            return Err(invalid(
                "Supported dumps are Wikidata JSON/JSONL and Wikipedia XML selections",
            ));
        }
    };
    store::knowledge::read_archive(&serde_json::to_string(&archive)?)?;
    Ok(archive)
}

fn wikidata(raw: &str, language: &str) -> Result<Archive, EngineError> {
    let trimmed = raw.trim();
    let entities: Vec<Value> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)?
    } else {
        trimmed
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line.trim().trim_end_matches(',')))
            .collect::<Result<_, _>>()?
    };
    if entities.is_empty() || entities.len() > 256 {
        return Err(invalid(
            "Choose 1–256 entities with their property and unit definitions",
        ));
    }
    let mut definitions = BTreeMap::new();
    for entity in &entities {
        let id = entity["id"]
            .as_str()
            .filter(|id| wikimedia::entity_id(id))
            .ok_or_else(|| invalid("Dump entity identity unavailable"))?;
        if definitions.insert(id.to_owned(), entity.clone()).is_some() {
            return Err(invalid("Choose one revision per dump entity"));
        }
    }
    let source = format!("dump-wikidata-{language}");
    let items = entities
        .iter()
        .map(|entity| wikimedia::wikidata_item(&source, language, entity, &definitions))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Archive {
        format: 1,
        source: Source {
            id: source,
            name: format!("Wikidata offline ({language})"),
            uri: "https://www.wikidata.org".into(),
            online: false,
            license: items.first().and_then(|item| item.license.clone()),
        },
        items,
    })
}

fn wikipedia(raw: &str, language: &str) -> Result<Archive, EngineError> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(raw);
    let mut path = Vec::new();
    let mut fields = BTreeMap::<String, String>::new();
    let mut page_start = 0;
    let mut items = Vec::new();
    let source = format!("dump-wikipedia-{language}");
    let base = format!("https://{language}.wikipedia.org");
    loop {
        let before = reader.buffer_position() as usize;
        match reader
            .read_event()
            .map_err(|error| invalid(&format!("Invalid Wikipedia XML: {error}")))?
        {
            Event::Start(tag) => {
                let name = String::from_utf8_lossy(tag.local_name().as_ref()).into_owned();
                if name == "page" {
                    fields.clear();
                    page_start = before;
                }
                if name == "revision" {
                    fields.retain(|key, _| !key.contains("/revision/"));
                }
                path.push(name);
                if path.len() > 32 {
                    return Err(invalid("Dump XML exceeds its nesting limit"));
                }
            }
            Event::Text(text) => {
                fields.entry(path.join("/")).or_default().push_str(
                    &text
                        .xml10_content()
                        .map_err(|error| invalid(&error.to_string()))?,
                );
            }
            Event::CData(text) => {
                fields
                    .entry(path.join("/"))
                    .or_default()
                    .push_str(&text.decode().map_err(|error| invalid(&error.to_string()))?);
            }
            Event::GeneralRef(reference) => {
                let field = fields.entry(path.join("/")).or_default();
                if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|error| invalid(&error.to_string()))?
                {
                    field.push(character)
                } else {
                    let entity = reference
                        .decode()
                        .map_err(|error| invalid(&error.to_string()))?;
                    field.push_str(match entity.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "apos" => "'",
                        "quot" => "\"",
                        _ => return Err(invalid("External XML entities are unsupported")),
                    });
                }
            }
            Event::End(tag) => {
                if tag.local_name().as_ref() == b"page" {
                    let get = |suffix: &str| {
                        fields
                            .iter()
                            .find(|(key, _)| key.ends_with(suffix))
                            .map(|(_, value)| value.as_str())
                            .unwrap_or("")
                    };
                    let pageid = get("/page/id")
                        .parse::<u64>()
                        .map_err(|_| invalid("Dump page identity unavailable"))?;
                    let revid = get("/revision/id")
                        .parse::<u64>()
                        .map_err(|_| invalid("Dump revision identity unavailable"))?;
                    let page = json!({"pageid":pageid,"title":get("/page/title"),"revisions":[{"revid":revid,"timestamp":get("/revision/timestamp"),"user":get("/revision/contributor/username"),"slots":{"main":{"content":get("/revision/text")}}}]});
                    let mut item = wikimedia::wikipedia_item(&source, &base, &page, &Value::Null)?;
                    item.raw = raw[page_start..reader.buffer_position() as usize].to_owned();
                    items.push(item);
                    if items.len() > 256 {
                        return Err(invalid("Choose a dump selection with at most 256 articles"));
                    }
                }
                path.pop();
            }
            Event::DocType(_) => return Err(invalid("XML document declarations are unsupported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if items.is_empty() || !path.is_empty() {
        return Err(invalid("Choose a complete Wikipedia XML selection"));
    }
    Ok(Archive {
        format: 1,
        source: Source {
            id: source,
            name: format!("Wikipedia offline ({language})"),
            uri: base,
            online: false,
            license: items.first().and_then(|item| item.license.clone()),
        },
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_selections_keep_revisions_attribution_and_raw_source() {
        let xml = "<mediawiki><page><title>العربية</title><id>42</id><revision><id>51</id><contributor><username>Alice</username></contributor><text>日本語 &amp; café</text></revision></page></mediawiki>";
        let archive = parse("wikipedia_xml", xml, "ar").unwrap();
        assert_eq!(archive.items[0].content, "日本語 & café");
        assert_eq!(archive.items[0].revision, "51");
        assert!(!archive.source.online);
        assert!(archive.items[0].raw.contains("<username>Alice</username>"));
        let entity =
            json!({"id":"Q1","lastrevid":1,"labels":{"en":{"value":"Universe"}},"claims":{}});
        assert_eq!(
            parse("wikidata_json", &entity.to_string(), "en")
                .unwrap()
                .items
                .len(),
            1
        );
        assert!(
            parse(
                "wikipedia_xml",
                "<!DOCTYPE foo [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><mediawiki/>",
                "en"
            )
            .is_err()
        );
        assert!(parse("wikipedia_xml", &"a".repeat(ITEM_LIMIT + 1), "en").is_err());
    }
}
