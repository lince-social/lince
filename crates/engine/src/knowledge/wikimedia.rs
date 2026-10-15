use super::{
    EngineError, invalid,
    provider::{Control, Provider},
};
use async_trait::async_trait;
use nucleus::knowledge::{
    Claim, Credit, ITEM_LIMIT, Item, License, Meaning, RESULT_LIMIT, Reference, SearchResult,
    Source,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub struct Wikimedia {
    language: String,
    wikidata: bool,
    client: reqwest::Client,
}

pub fn catalog() -> Vec<Source> {
    [
        "wikidata:en",
        "wikidata:pt",
        "wikipedia:en",
        "wikipedia:pt",
        "wikipedia:es",
        "wikipedia:ar",
        "wikipedia:he",
        "wikipedia:zh",
        "wikipedia:ja",
        "wikipedia:ru",
    ]
    .into_iter()
    .filter_map(|id| {
        Wikimedia::from_source(id)
            .ok()
            .flatten()
            .map(|source| source.description())
    })
    .collect()
}

pub fn entity_id(id: &str) -> bool {
    matches!(id.as_bytes().first(), Some(b'Q' | b'P'))
        && id.len() > 1
        && id.len() <= 32
        && id.as_bytes()[1..].iter().all(u8::is_ascii_digit)
        && id.as_bytes()[1] != b'0'
}

fn wiki_license() -> License {
    License {
        name: "CC BY-SA 4.0".into(),
        uri: "https://creativecommons.org/licenses/by-sa/4.0/".into(),
    }
}
fn data_license() -> License {
    License {
        name: "CC0 1.0".into(),
        uri: "https://creativecommons.org/publicdomain/zero/1.0/".into(),
    }
}

impl Wikimedia {
    pub fn from_source(id: &str) -> Result<Option<Self>, EngineError> {
        let Some((kind, language)) = id.split_once(':') else {
            return Ok(None);
        };
        if !matches!(kind, "wikidata" | "wikipedia") {
            return Ok(None);
        }
        if language.is_empty()
            || language.len() > 24
            || !language
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c == b'-')
            || language.starts_with('-')
            || language.ends_with('-')
        {
            return Err(invalid("Choose a valid Wikipedia or Wikidata language"));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            .user_agent("Lince/0.8 (https://github.com/lince-social/lince; knowledge reader)")
            .build()
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(Some(Self {
            language: language.into(),
            wikidata: kind == "wikidata",
            client,
        }))
    }

    fn base(&self) -> String {
        if self.wikidata {
            "https://www.wikidata.org".into()
        } else {
            format!("https://{}.wikipedia.org", self.language)
        }
    }

    async fn get(
        &self,
        params: &[(&str, String)],
        budget: &mut usize,
        control: &Control,
    ) -> Result<Value, EngineError> {
        control.checkpoint()?;
        let request = self
            .client
            .get(format!("{}/w/api.php", self.base()))
            .query(&[("format", "json"), ("formatversion", "2")])
            .query(params)
            .send();
        let mut response = tokio::select! {_=control.cancelled()=>return Err(invalid("Knowledge request cancelled")),result=request=>result.map_err(|error|invalid(&format!("Knowledge source unavailable: {error}")))?};
        if !response.status().is_success() {
            return Err(invalid(&format!(
                "Knowledge source returned {}; retry later{}",
                response.status(),
                response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .map(|value| format!(" after {value} seconds"))
                    .unwrap_or_default()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > ITEM_LIMIT as u64)
        {
            return Err(invalid("Knowledge response exceeds 4 MiB"));
        }
        let mut bytes = Vec::new();
        loop {
            let chunk = tokio::select! {_=control.cancelled()=>return Err(invalid("Knowledge request cancelled")),result=response.chunk()=>result.map_err(|error|invalid(&error.to_string()))?};
            let Some(chunk) = chunk else { break };
            if bytes.len() + chunk.len() > ITEM_LIMIT || *budget + chunk.len() > 16 * 1024 * 1024 {
                return Err(invalid("Knowledge download exceeds its data limit"));
            }
            *budget += chunk.len();
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if let Some(error) = value.get("error") {
            return Err(invalid(&format!(
                "Knowledge source: {}",
                error["info"].as_str().unwrap_or("request failed")
            )));
        }
        Ok(value)
    }

    async fn entities(
        &self,
        ids: &str,
        budget: &mut usize,
        control: &Control,
    ) -> Result<Value, EngineError> {
        self.get(
            &[
                ("action", "wbgetentities".into()),
                ("ids", ids.into()),
                ("languages", format!("{}|en", self.language)),
                ("languagefallback", "1".into()),
            ],
            budget,
            control,
        )
        .await
    }
}

#[async_trait]
impl Provider for Wikimedia {
    fn description(&self) -> Source {
        Source {
            id: format!(
                "{}:{}",
                if self.wikidata {
                    "wikidata"
                } else {
                    "wikipedia"
                },
                self.language
            ),
            name: format!(
                "{} ({})",
                if self.wikidata {
                    "Wikidata"
                } else {
                    "Wikipedia"
                },
                self.language
            ),
            uri: self.base(),
            online: true,
            license: Some(if self.wikidata {
                data_license()
            } else {
                wiki_license()
            }),
        }
    }
    async fn search(
        &self,
        text: &str,
        offset: u32,
        control: &Control,
    ) -> Result<SearchResult, EngineError> {
        let mut budget = 0;
        if self.wikidata {
            let response = self
                .get(
                    &[
                        ("action", "query".into()),
                        ("list", "search".into()),
                        ("srsearch", text.into()),
                        ("srnamespace", "0|120".into()),
                        ("srlimit", RESULT_LIMIT.to_string()),
                        ("sroffset", offset.to_string()),
                    ],
                    &mut budget,
                    control,
                )
                .await?;
            let mut items = Vec::new();
            for entry in response["query"]["search"].as_array().into_iter().flatten() {
                let id = entry["title"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("Property:");
                if entity_id(id) {
                    items.push(Reference {
                        source: self.description().id,
                        id: id.into(),
                        title: format!(
                            "{id} · {}",
                            html_text(entry["snippet"].as_str().unwrap_or(""))?
                        ),
                        uri: format!("https://www.wikidata.org/wiki/{id}"),
                        required: false,
                    });
                }
            }
            let next = response["continue"]["sroffset"]
                .as_u64()
                .map(|value| value as u32);
            Ok(SearchResult {
                source: self.description().id,
                items,
                complete: next.is_none(),
                next_offset: next,
                offline: false,
            })
        } else {
            let response = self
                .get(
                    &[
                        ("action", "query".into()),
                        ("list", "search".into()),
                        ("srsearch", text.into()),
                        ("srlimit", RESULT_LIMIT.to_string()),
                        ("sroffset", offset.to_string()),
                    ],
                    &mut budget,
                    control,
                )
                .await?;
            let items = response["query"]["search"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    let id = entry["pageid"].as_u64()?.to_string();
                    Some(Reference {
                        source: self.description().id,
                        id: id.clone(),
                        title: entry["title"].as_str()?.into(),
                        uri: format!("{}/?curid={id}", self.base()),
                        required: false,
                    })
                })
                .collect();
            let next = response["continue"]["sroffset"]
                .as_u64()
                .map(|value| value as u32);
            Ok(SearchResult {
                source: self.description().id,
                items,
                complete: next.is_none(),
                next_offset: next,
                offline: false,
            })
        }
    }
    async fn read(&self, id: &str, control: &Control) -> Result<Item, EngineError> {
        let mut budget = 0;
        if self.wikidata {
            if !entity_id(id) {
                return Err(invalid("Choose a Wikidata item or property identity"));
            }
            let response = self.entities(id, &mut budget, control).await?;
            let entity = &response["entities"][id];
            let mut meanings = BTreeMap::new();
            let ids = external_meanings(entity)?;
            for chunk in ids.into_iter().collect::<Vec<_>>().chunks(50) {
                let definitions = self
                    .entities(&chunk.join("|"), &mut budget, control)
                    .await?;
                for (id, entity) in definitions["entities"].as_object().into_iter().flatten() {
                    if entity.get("missing").is_none() {
                        meanings.insert(id.clone(), entity.clone());
                    }
                }
            }
            wikidata_item(&self.description().id, &self.language, entity, &meanings)
        } else {
            if id.is_empty() || id.len() > 32 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid("Choose a Wikipedia page identity"));
            }
            let response = self
                .get(
                    &[
                        ("action", "query".into()),
                        ("pageids", id.into()),
                        ("prop", "revisions|info".into()),
                        ("inprop", "url".into()),
                        ("rvprop", "ids|timestamp|user|content".into()),
                        ("rvslots", "main".into()),
                        ("meta", "siteinfo".into()),
                        ("siprop", "rightsinfo".into()),
                    ],
                    &mut budget,
                    control,
                )
                .await?;
            let page = response["query"]["pages"]
                .as_array()
                .and_then(|pages| pages.first())
                .ok_or_else(|| invalid("Wikipedia page unavailable"))?;
            let revision = page["revisions"][0]["revid"]
                .as_u64()
                .ok_or_else(|| invalid("Wikipedia revision unavailable"))?;
            let rendered = self
                .get(
                    &[
                        ("action", "parse".into()),
                        ("oldid", revision.to_string()),
                        ("prop", "text".into()),
                        ("disableeditsection", "1".into()),
                    ],
                    &mut budget,
                    control,
                )
                .await?;
            let text = rendered["parse"]["text"]
                .as_str()
                .ok_or_else(|| invalid("Wikipedia revision text unavailable"))?;
            let mut item = wikipedia_item(
                &self.description().id,
                &self.base(),
                page,
                &response["query"]["rightsinfo"],
            )?;
            item.content = html_text(text)?;
            item.content_type = "text/plain".into();
            item.raw=json!({"page":page,"rightsinfo":response["query"]["rightsinfo"],"rendered":rendered}).to_string();
            store::knowledge::validate_item(&item)?;
            Ok(item)
        }
    }
}

fn wording(entity: &Value, field: &str, language: &str) -> String {
    entity[field][language]["value"]
        .as_str()
        .or_else(|| entity[field]["en"]["value"].as_str())
        .or_else(|| {
            entity[field]
                .as_object()
                .and_then(|values| values.values().find_map(|value| value["value"].as_str()))
        })
        .unwrap_or("")
        .to_owned()
}

pub(super) fn external_meanings(entity: &Value) -> Result<BTreeSet<String>, EngineError> {
    fn collect(value: &Value, ids: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if key == "property"
                        && let Some(id) = value.as_str().filter(|id| entity_id(id))
                    {
                        ids.insert(id.into());
                    }
                    if key == "unit"
                        && let Some(id) = value
                            .as_str()
                            .and_then(|uri| {
                                uri.strip_prefix("http://www.wikidata.org/entity/")
                                    .or_else(|| {
                                        uri.strip_prefix("https://www.wikidata.org/entity/")
                                    })
                            })
                            .filter(|id| entity_id(id))
                    {
                        ids.insert(id.into());
                    }
                    collect(value, ids);
                }
            }
            Value::Array(values) => {
                for value in values {
                    collect(value, ids)
                }
            }
            _ => {}
        }
    }
    let mut ids = BTreeSet::new();
    if let Some(id) = entity["id"]
        .as_str()
        .filter(|id| id.starts_with('P') && entity_id(id))
    {
        ids.insert(id.into());
    }
    collect(&entity["claims"], &mut ids);
    if ids.len() > 256 {
        return Err(invalid("This entity uses more than 256 external meanings"));
    }
    Ok(ids)
}

pub(super) fn wikidata_item(
    source: &str,
    language: &str,
    entity: &Value,
    definitions: &BTreeMap<String, Value>,
) -> Result<Item, EngineError> {
    let id = entity["id"]
        .as_str()
        .filter(|id| entity_id(id))
        .ok_or_else(|| invalid("Wikidata entity unavailable"))?;
    let revision = entity["lastrevid"]
        .as_u64()
        .ok_or_else(|| invalid("Wikidata source revision unavailable"))?
        .to_string();
    let title = wording(entity, "labels", language);
    let content = wording(entity, "descriptions", language);
    let mut claims = Vec::new();
    for (property, statements) in entity["claims"].as_object().into_iter().flatten() {
        for statement in statements.as_array().into_iter().flatten() {
            let claim_id = statement["id"]
                .as_str()
                .ok_or_else(|| invalid("Wikidata statement has no identity"))?;
            claims.push(Claim {id:claim_id.into(),property:property.into(),value:statement["mainsnak"].clone(),conditions:json!({"qualifiers":statement["qualifiers"],"qualifiers_order":statement["qualifiers-order"],"rank":statement["rank"]}),references:statement["references"].clone(),source:source.into(),revision:revision.clone(),raw:statement.to_string()});
        }
    }
    let required = external_meanings(entity)?;
    if required.iter().any(|id| !definitions.contains_key(id)) {
        return Err(invalid(
            "A Wikidata property or unit definition is unavailable",
        ));
    }
    let meanings = required
        .iter()
        .map(|id| {
            let value = &definitions[id];
            let revision = value["lastrevid"]
                .as_u64()
                .ok_or_else(|| invalid("External meaning revision unavailable"))?
                .to_string();
            Ok(Meaning {
                id: id.clone(),
                revision,
                title: wording(value, "labels", language),
                definition: wording(value, "descriptions", language),
                raw: value.to_string(),
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    let uri =
        format!("https://www.wikidata.org/wiki/Special:EntityData/{id}.json?revision={revision}");
    let item = Item {
        source: source.into(),
        id: id.into(),
        revision,
        title: if title.is_empty() { id.into() } else { title },
        content,
        content_type: "text/plain".into(),
        uri: uri.clone(),
        license: Some(data_license()),
        credits: vec![Credit {
            name: "Wikidata contributors".into(),
            uri,
        }],
        meanings,
        claims,
        dependencies: vec![],
        assets: vec![],
        raw: entity.to_string(),
    };
    store::knowledge::validate_item(&item)?;
    Ok(item)
}

pub(super) fn wikipedia_item(
    source: &str,
    base: &str,
    page: &Value,
    rights: &Value,
) -> Result<Item, EngineError> {
    let id = page["pageid"]
        .as_u64()
        .ok_or_else(|| invalid("Wikipedia page identity unavailable"))?
        .to_string();
    let revision = &page["revisions"][0];
    let revision_id = revision["revid"]
        .as_u64()
        .ok_or_else(|| invalid("Wikipedia revision unavailable"))?
        .to_string();
    let uri = format!("{base}/w/index.php?oldid={revision_id}");
    let license = if let (Some(name), Some(uri)) = (rights["text"].as_str(), rights["url"].as_str())
    {
        License {
            name: name.into(),
            uri: uri.into(),
        }
    } else {
        wiki_license()
    };
    let content = revision["slots"]["main"]["content"]
        .as_str()
        .or_else(|| revision["content"].as_str())
        .ok_or_else(|| invalid("Wikipedia revision content unavailable"))?;
    let item = Item {
        source: source.into(),
        id,
        revision: revision_id,
        title: page["title"]
            .as_str()
            .ok_or_else(|| invalid("Wikipedia title unavailable"))?
            .into(),
        content: content.into(),
        content_type: "text/x-wiki".into(),
        uri: uri.clone(),
        license: Some(license),
        credits: vec![
            Credit {
                name: "Wikipedia contributors (article and revision history)".into(),
                uri,
            },
            Credit {
                name: revision["user"]
                    .as_str()
                    .unwrap_or("Revision author unavailable")
                    .into(),
                uri: format!("{base}/w/index.php?curid={}&action=history", page["pageid"]),
            },
        ],
        meanings: vec![],
        claims: vec![],
        dependencies: vec![],
        assets: vec![],
        raw: json!({"page":page,"rightsinfo":rights}).to_string(),
    };
    store::knowledge::validate_item(&item)?;
    Ok(item)
}

pub(super) fn html_text(html: &str) -> Result<String, EngineError> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(html);
    reader.config_mut().check_end_names = false;
    reader.config_mut().allow_unmatched_ends = true;
    let mut result = String::new();
    let mut hidden = 0usize;
    loop {
        match reader
            .read_event()
            .map_err(|error| invalid(&format!("Invalid article text: {error}")))?
        {
            Event::Start(tag) => {
                if matches!(tag.name().as_ref(), b"script" | b"style") {
                    hidden += 1;
                }
                if hidden == 0
                    && matches!(
                        tag.name().as_ref(),
                        b"p" | b"div" | b"h1" | b"h2" | b"h3" | b"li" | b"tr" | b"br"
                    )
                {
                    result.push('\n');
                }
            }
            Event::End(tag) => {
                if matches!(tag.name().as_ref(), b"script" | b"style") {
                    hidden = hidden.saturating_sub(1);
                }
                if hidden == 0
                    && matches!(
                        tag.name().as_ref(),
                        b"p" | b"div" | b"h1" | b"h2" | b"h3" | b"li" | b"tr"
                    )
                {
                    result.push('\n');
                }
            }
            Event::Empty(tag) if hidden == 0 && tag.name().as_ref() == b"br" => result.push('\n'),
            Event::Text(text) if hidden == 0 => result.push_str(
                &text
                    .xml10_content()
                    .map_err(|error| invalid(&error.to_string()))?,
            ),
            Event::GeneralRef(reference) if hidden == 0 => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|error| invalid(&error.to_string()))?
                {
                    result.push(character);
                } else {
                    let name = reference
                        .decode()
                        .map_err(|error| invalid(&error.to_string()))?;
                    result.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        "nbsp" => " ",
                        _ => "�",
                    });
                }
            }
            Event::DocType(_) => {
                return Err(invalid("Article document declarations are unsupported"));
            }
            Event::Eof => break,
            _ => {}
        }
        if result.len() > ITEM_LIMIT {
            return Err(invalid("Article text exceeds 4 MiB"));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_article_text_handles_multilingual_text_and_ignores_scripts() {
        let text =
            html_text("<h2>العربية</h2><p>日本語 &amp; café<br>אחד</p><script>bad()</script>")
                .unwrap();
        assert!(text.contains("日本語 & café"));
        assert!(text.contains("אחד"));
        assert!(!text.contains("bad()"));
        assert!(Wikimedia::from_source("wikipedia:en.evil.test").is_err());
        assert!(Wikimedia::from_source("wikipedia:en/../local").is_err());
    }
    #[test]
    fn structured_statements_keep_full_values_qualifiers_references_and_versions() {
        let entity = json!({"id":"Q1","lastrevid":12,"labels":{"en":{"value":"Universe"}},"claims":{"P1":[{"id":"Q1$abc","mainsnak":{"property":"P1","snaktype":"value","datavalue":{"value":{"amount":"+12","unit":"http://www.wikidata.org/entity/Q2"}}},"qualifiers":{"P3":[{"property":"P3","snaktype":"novalue"}]},"references":[{"snaks":{"P4":[{"property":"P4","datavalue":{"value":"citation"}}]}}],"rank":"preferred"}]}});
        let defs=["P1","Q2","P3","P4"].into_iter().map(|id|(id.into(),json!({"id":id,"lastrevid":9,"labels":{"en":{"value":id}},"descriptions":{"en":{"value":"definition"}}}))).collect();
        let item = wikidata_item("wikidata:en", "en", &entity, &defs).unwrap();
        assert_eq!(item.revision, "12");
        assert_eq!(item.claims[0].conditions["rank"], "preferred");
        assert_eq!(
            item.claims[0].value["datavalue"]["value"]["unit"],
            "http://www.wikidata.org/entity/Q2"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&item.claims[0].raw).unwrap(),
            entity["claims"]["P1"][0]
        );
        assert_eq!(item.meanings.len(), 4);
    }

    async fn live_read(source: &str, text: &str) {
        let control = Control::default();
        let provider = Wikimedia::from_source(source).unwrap().unwrap();
        let result = provider.search(text, 0, &control).await.unwrap();
        assert!(!result.items.is_empty());
        let item = provider.read(&result.items[0].id, &control).await.unwrap();
        assert!(!item.revision.is_empty());
        assert!(item.license.is_some());
        assert!(!item.raw.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn live_wikidata_search_and_revision_read() {
        live_read("wikidata:en", "Douglas Adams").await;
    }

    #[tokio::test]
    #[ignore]
    async fn live_wikipedia_search_and_revision_read() {
        live_read("wikipedia:pt", "Vocabulário").await;
    }
}
