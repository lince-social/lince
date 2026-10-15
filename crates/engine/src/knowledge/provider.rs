use crate::EngineError;
use async_trait::async_trait;
use nucleus::knowledge::{
    Archive, Claim, Credit, Item, License, Meaning, RESULT_LIMIT, Reference, SearchResult, Source,
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
    changed: Arc<tokio::sync::Notify>,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            changed: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

impl Control {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.changed.notify_waiters();
    }
    pub fn checkpoint(&self) -> Result<(), EngineError> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(EngineError::Consequence(
                "Knowledge request cancelled".into(),
            ));
        }
        Ok(())
    }
    pub async fn cancelled(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.cancelled.load(Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn description(&self) -> Source;
    async fn search(
        &self,
        text: &str,
        offset: u32,
        control: &Control,
    ) -> Result<SearchResult, EngineError>;
    async fn read(&self, id: &str, control: &Control) -> Result<Item, EngineError>;
}

pub struct Collection {
    pub archive: Archive,
}

#[async_trait]
impl Provider for Collection {
    fn description(&self) -> Source {
        self.archive.source.clone()
    }
    async fn search(
        &self,
        text: &str,
        offset: u32,
        control: &Control,
    ) -> Result<SearchResult, EngineError> {
        control.checkpoint()?;
        let text = text.to_lowercase();
        let matches: Vec<_> = self
            .archive
            .items
            .iter()
            .filter(|item| {
                item.title.to_lowercase().contains(&text)
                    || item.content.to_lowercase().contains(&text)
            })
            .collect();
        let items = matches
            .iter()
            .skip(offset as usize)
            .take(RESULT_LIMIT)
            .map(|item| Reference {
                source: item.source.clone(),
                id: item.id.clone(),
                title: item.title.clone(),
                uri: item.uri.clone(),
                required: false,
            })
            .collect::<Vec<_>>();
        let end = offset as usize + items.len();
        Ok(SearchResult {
            source: self.archive.source.id.clone(),
            items,
            complete: end >= matches.len(),
            next_offset: (end < matches.len()).then_some(end as u32),
            offline: true,
        })
    }
    async fn read(&self, id: &str, control: &Control) -> Result<Item, EngineError> {
        tokio::task::yield_now().await;
        control.checkpoint()?;
        self.archive
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| EngineError::Consequence("This source item is unavailable".into()))
    }
}

pub fn test_source() -> Collection {
    let license = Some(License {
        name: "CC0 1.0".into(),
        uri: "https://creativecommons.org/publicdomain/zero/1.0/".into(),
    });
    let mut items = Vec::new();
    for (id, title, content) in [
        (
            "reading",
            "Temperature reading",
            "A reading of 21.5 degrees Celsius with its time and reference.",
        ),
        (
            "reference",
            "Example reference",
            "The source reference for the example reading.",
        ),
    ] {
        let uri = format!("urn:lince:knowledge:test:{id}");
        let claim = json!({"amount":"21.5","unit":"celsius"});
        items.push(Item {source:"test".into(),id:id.into(),revision:"1".into(),title:title.into(),content:content.into(),content_type:"text/plain".into(),uri:uri.clone(),license:license.clone(),credits:vec![Credit {name:"Lince test source".into(),uri:uri.clone()}],meanings:if id=="reading" {vec![Meaning {id:"measurement".into(),revision:"1".into(),title:"Temperature measurement".into(),definition:"A temperature reading with its stated unit, time, and reference.".into(),raw:json!({"id":"measurement","revision":"1"}).to_string()},Meaning {id:"celsius".into(),revision:"1".into(),title:"Degrees Celsius".into(),definition:"The Celsius temperature scale.".into(),raw:json!({"id":"celsius","revision":"1"}).to_string()}]} else {vec![]},claims:if id=="reading" {vec![Claim {id:"reading-claim".into(),property:"measurement".into(),value:claim.clone(),conditions:json!({"measured_at":"2026-10-13T12:00:00Z"}),references:json!([{ "source":"test","id":"reference","revision":"1"}]),source:"test".into(),revision:"1".into(),raw:json!({"value":claim,"conditions":{"measured_at":"2026-10-13T12:00:00Z"},"references":[{"id":"reference","revision":"1"}]}).to_string()}]} else {vec![]},dependencies:if id=="reading" {vec![Reference {source:"test".into(),id:"reference".into(),title:"Example reference".into(),uri:"urn:lince:knowledge:test:reference".into(),required:true}]} else {vec![]},assets:vec![],raw:json!({"id":id,"revision":"1","content":content}).to_string()});
    }
    Collection {
        archive: Archive {
            format: 1,
            source: Source {
                id: "test".into(),
                name: "Example knowledge collection".into(),
                uri: "urn:lince:knowledge:test".into(),
                online: false,
                license,
            },
            items,
        },
    }
}
