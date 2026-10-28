mod import;

use crate::{Engine, EngineError, actions::ActionOutcome};
use lince_resources::{Library, RESOURCE_NAMESPACE, Resource, publication::Publication};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

pub const IMPORT_NAMESPACE: &str = "lince.source-import";
pub const BINDING_NAMESPACE: &str = "lince.source-binding";

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Open {
        reference: lince_resources::Reference,
        #[serde(default)]
        record: Option<String>,
    },
    Preview {
        uri: String,
        #[serde(default)]
        selected: Vec<String>,
        #[serde(default)]
        bindings: BTreeMap<String, String>,
        #[serde(default)]
        refresh: bool,
    },
    Import {
        uri: String,
        selected: Vec<String>,
        bindings: BTreeMap<String, String>,
        revision: String,
        #[serde(default)]
        update: bool,
    },
    Inspect {
        record: String,
    },
    Download {
        record: String,
        #[serde(default)]
        representation: Option<usize>,
    },
    RetainInterval {
        record: String,
        start_ms: u64,
        end_ms: u64,
        #[serde(default)]
        representation: Option<usize>,
    },
    Register {
        record: String,
        path: String,
    },
    Export {
        uri: String,
        #[serde(default)]
        selected: Vec<String>,
        destination: String,
        #[serde(default)]
        media: bool,
    },
    Audit {
        uri: String,
        #[serde(default)]
        inverses: Vec<(String, String)>,
    },
    Extract {
        uri: String,
        destination: String,
    },
    Activate {
        record: String,
    },
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::Import { .. } => "record:create",
            Self::Activate { .. } => "frequency:create",
            _ => "record:read",
        }
    }
}

fn invalid(error: impl ToString) -> EngineError {
    EngineError::Consequence(error.to_string())
}

pub fn library() -> Result<Library, EngineError> {
    let library = utils::config::lince_data_dir()
        .map(Library::new)
        .ok_or_else(|| invalid("Lince's data directory is unavailable"))?;
    Ok(if let Some(cache) = utils::config::lince_cache_dir() {
        library.with_cache(cache)
    } else {
        library
    })
}

impl Engine {
    async fn known_reference(
        &self,
        target: &str,
        context: Option<&str>,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let target = target.trim_start_matches('@');
        if lince_resources::reference::record_uid(target) {
            if let Ok(uid) = self.resolve(target).await {
                self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                    .await?;
                return Ok(Some(uid));
            }
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("The local Organ is unavailable"))?
                .uid;
            let found: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT r.uid FROM record_source_location l JOIN record r ON r.uid=l.record_uid WHERE l.source_uid=? AND r.organ_uid=? AND r.deleted_at IS NULL LIMIT 2").bind(target).bind(organ).fetch_all(&self.store.pool).await?;
            if found.len() == 1 {
                self.refuse_unreadable(actor, &found).await?;
                return Ok(found.into_iter().next());
            }
        } else if let Some(context) = context {
            let uid = self.resolve(context).await?;
            self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                .await?;
            if let Some(source) =
                store::records::get_extension(&self.store.pool, &uid, IMPORT_NAMESPACE).await?
            {
                let found: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT r.uid FROM record_extension e JOIN record r ON r.uid=e.record_uid JOIN record context ON context.uid=? WHERE e.namespace=? AND json_extract(e.fds,'$.slug')=? AND (json_extract(e.fds,'$.document')=? OR json_extract(e.fds,'$.entry')=?) AND r.organ_uid IS context.organ_uid AND r.deleted_at IS NULL LIMIT 2")
                    .bind(&uid).bind(IMPORT_NAMESPACE).bind(target).bind(source["document"].as_str()).bind(source["entry"].as_str()).fetch_all(&self.store.pool).await?;
                if found.len() == 1 {
                    self.refuse_unreadable(actor, &found).await?;
                    return Ok(found.into_iter().next());
                }
                if found.len() > 1 {
                    return Err(invalid(
                        "This reference has several accepted definitions; use a Record UID or source URI",
                    ));
                }
                return Ok(None);
            }
        }
        if !lince_resources::reference::is_address(target)
            && let Ok(uid) = self.resolve(target).await
        {
            self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                .await?;
            return Ok(Some(uid));
        }
        Ok(None)
    }

    async fn source_publication(
        &self,
        uri: String,
        selected: Vec<String>,
        refresh: bool,
        actor: Option<&str>,
    ) -> Result<Publication, EngineError> {
        let mut reference = lince_resources::Reference::new(&uri);
        if lince_resources::reference::is_id(&uri)
            && let Ok(uid) = self.resolve(uri.trim_start_matches('@')).await
        {
            self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                .await?;
            if let Some(metadata) =
                store::records::get_extension(&self.store.pool, &uid, IMPORT_NAMESPACE).await?
            {
                let original = metadata["original"]
                    .as_str()
                    .ok_or_else(|| invalid("The Record has no authored identity"))?;
                let document = metadata["document"]
                    .as_str()
                    .ok_or_else(|| invalid("The Record has no defining source"))?;
                reference.target = format!("{document}#{original}");
                reference.uid = Some(original.into());
            }
        }
        let library = if actor.is_some() {
            library()?.remote()
        } else {
            library()?
        };
        tokio::task::spawn_blocking(move || {
            Publication::load_reference(&library, reference, &selected, refresh)
        })
        .await
        .map_err(invalid)?
        .map_err(invalid)
    }

    pub(crate) async fn source_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, command.permission()).await?;
        if actor.is_some() {
            let uri = match &command {
                Command::Preview { uri, .. }
                | Command::Import { uri, .. }
                | Command::Export { uri, .. } => Some(uri),
                _ => None,
            };
            if uri.is_some_and(|uri| {
                !lince_resources::reference::is_id(uri)
                    && lince_resources::address(uri).is_ok_and(|uri| uri.scheme() == "file")
            }) {
                return Err(EngineError::Forbidden(
                    "Reading local source files requires the local owner session".into(),
                ));
            }
        }
        let data = match command {
            Command::Open {
                mut reference,
                record,
            } => {
                if !lince_resources::reference::is_address(&reference.target)
                    && (reference.base.is_none()
                        || record.is_some()
                        || lince_resources::reference::record_uid(
                            reference.target.trim_start_matches('@'),
                        ))
                    && let Some(uid) = self
                        .known_reference(&reference.target, record.as_deref(), actor)
                        .await?
                {
                    return Ok(ActionOutcome {
                        data: Some(json!({"local_record":uid})),
                        ..Default::default()
                    });
                }
                if reference.base.is_none()
                    && let Some(record) = record
                {
                    let uid = self.resolve(&record).await?;
                    self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                        .await?;
                    reference.base =
                        store::records::get_extension(&self.store.pool, &uid, IMPORT_NAMESPACE)
                            .await?
                            .and_then(|value| value["base"].as_str().map(str::to_owned));
                }
                let library = if actor.is_some() {
                    library()?.remote()
                } else {
                    library()?
                };
                let mut failures = Vec::new();
                let mut result = None;
                for candidate in reference.addresses(&library).map_err(invalid)? {
                    if actor.is_some()
                        && lince_resources::address(&candidate.uri)
                            .is_ok_and(|uri| uri.scheme() == "file")
                    {
                        failures.push("Local files require the local owner session".into());
                        continue;
                    }
                    let mut uri = lince_resources::address(&candidate.uri).map_err(invalid)?;
                    uri.set_fragment(candidate.fragment.as_deref());
                    if uri.scheme() == "file"
                        && let Ok(path) = uri.to_file_path()
                        && path.is_dir()
                    {
                        let indexing_library = library.clone();
                        let files = tokio::task::spawn_blocking(move || {
                            Publication::index(&indexing_library, &path)
                        })
                        .await
                        .map_err(invalid)?
                        .map_err(invalid)?
                        .iter()
                        .map(|path| {
                            lince_resources::Reference::path(path).map(|reference| reference.target)
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(invalid)?;
                        result = Some(
                            json!({"entry":uri.to_string(),"directory":true,"files":files,"records":[],"ready":false}),
                        );
                        break;
                    }
                    let extension = uri
                        .path()
                        .rsplit('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let media_type = match extension.as_str() {
                        "pdf" => Some("application/pdf"),
                        "epub" => Some("application/epub+zip"),
                        "mp4" | "webm" | "mkv" => Some("video/*"),
                        "png" | "jpg" | "jpeg" | "webp" | "gif" => Some("image/*"),
                        _ => None,
                    };
                    if let Some(media_type) = media_type {
                        result = Some(
                            json!({"resource":Resource {uri:uri.to_string(), hash:candidate.hash, media_type:media_type.into(), ..Default::default()}}),
                        );
                        break;
                    }
                    if extension != "lingua"
                        && candidate.uid.is_none()
                        && matches!(uri.scheme(), "http" | "https")
                    {
                        result = Some(json!({"web":uri.to_string()}));
                        break;
                    }
                    if candidate.uid.is_none()
                        && matches!(extension.as_str(), "md" | "txt" | "json" | "toml")
                    {
                        let text_library = library.clone();
                        let text_uri = uri.to_string();
                        let text = tokio::task::spawn_blocking(move || {
                            text_library.source_text(&text_uri, 4 * 1024 * 1024, false)
                        })
                        .await
                        .map_err(invalid)?
                        .map_err(invalid)?;
                        if candidate.hash.as_ref().is_some_and(|expected| {
                            !expected.eq_ignore_ascii_case(&lince_resources::fingerprint(
                                text.as_bytes(),
                            ))
                        }) {
                            return Err(invalid(
                                "The source does not match its pinned content hash",
                            ));
                        }
                        let body = if extension == "md" {
                            text
                        } else {
                            format!("````{extension}\n{text}\n````")
                        };
                        result = Some(
                            json!({"entry":uri.to_string(),"ready":false,"records":[{"title":uri.path().rsplit('/').next().unwrap_or("Source"),"body":body,"base":uri.to_string(),"state":"source"}]}),
                        );
                        break;
                    }
                    let library = library.clone();
                    let reference = lince_resources::Reference {
                        target: uri.to_string(),
                        uid: candidate.uid,
                        hash: candidate.hash,
                        ..Default::default()
                    };
                    let opened = tokio::task::spawn_blocking(move || {
                        Publication::load_reference(&library, reference, &[], false)
                    })
                    .await
                    .map_err(invalid)?
                    .map_err(invalid);
                    match opened {
                        Ok(source) => {
                            let definitions: BTreeMap<_, _> = source
                                .documents
                                .iter()
                                .flat_map(|document| {
                                    document.project.records.iter().map(|record| {
                                        (
                                            record.uid.clone(),
                                            format!("{}#{}", document.base, record.uid),
                                        )
                                    })
                                })
                                .collect();
                            let mut body_links = definitions.clone();
                            for document in &source.documents {
                                for record in &document.project.records {
                                    if let Some(slug) = &record.slug {
                                        body_links
                                            .insert(slug.clone(), definitions[&record.uid].clone());
                                    }
                                }
                            }
                            let records = source.documents.iter().flat_map(|document| document.project.records.iter().map(|record| {
                                let resource = document.project.extensions.iter().find(|extension| extension.namespace == RESOURCE_NAMESPACE && extension.target_uid.as_ref() == Some(&record.uid)).map(|extension| &extension.fields);
                                json!({"uid":record.uid,"source_uid":record.uid,"uri":definitions[&record.uid],"title":record.head,"body":linked_body(&record.body, &body_links),"base":document.base,"cached":document.cached,"saved":document.saved,"state":"source","resource":resource,
                                    "assertions":record.assertions.iter().map(|assertion| json!({"predicate":assertion.predicate,"uri":assertion.object_uri.as_ref().or_else(|| assertion.object_uid.as_ref().and_then(|uid| definitions.get(uid))),"quantity":assertion.quantity})).collect::<Vec<_>>()})
                            })).collect::<Vec<_>>();
                            result = Some(
                                json!({"browsing":true,"ready":false,"entry":source.entry,"revision":source.revision,"fragment":source.fragment,"records":records}),
                            );
                            break;
                        }
                        Err(error) => failures.push(error.to_string()),
                    }
                }
                result.ok_or_else(|| invalid(failures.join("\n")))?
            }
            Command::Preview {
                uri,
                selected,
                bindings,
                refresh,
            } => {
                let source = self
                    .source_publication(uri, selected, refresh, actor)
                    .await?;
                self.preview_source(&source, bindings, actor).await?
            }
            Command::Import {
                uri,
                selected,
                bindings,
                revision,
                update,
            } => {
                let source = self.source_publication(uri, selected, false, actor).await?;
                if source.revision != revision {
                    return Err(invalid(
                        "The source changed. Preview its current version before importing.",
                    ));
                }
                self.import_source(&source, bindings, update, actor).await?
            }
            Command::Inspect { record } => {
                let record = self.resolve(&record).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&record))
                    .await?;
                let resource =
                    store::records::get_extension(&self.store.pool, &record, RESOURCE_NAMESPACE)
                        .await?
                        .map(|value| serde_json::from_value::<Resource>(value))
                        .transpose()?;
                let (local, cached) = if let Some(resource) = resource.clone() {
                    let library = library()?;
                    tokio::task::spawn_blocking(move || {
                        library.find(&resource).map(|entry| {
                            (
                                entry,
                                lince_resources::RangeReader::cached(&library, &resource),
                            )
                        })
                    })
                    .await
                    .map_err(invalid)?
                    .map_err(invalid)?
                } else {
                    (None, (0, None))
                };
                let mut related = Vec::new();
                let names = store::vocabulary::display_names(&self.store.pool, actor).await?;
                for assertion in
                    store::assertions::incident_to(&self.store.pool, std::slice::from_ref(&record))
                        .await?
                {
                    let outgoing = assertion.subject_uid == record;
                    let direction = if outgoing { "out" } else { "in" };
                    let predicate = names
                        .get(&assertion.predicate_uid)
                        .unwrap_or(&assertion.predicate_uid);
                    if outgoing && let Some(uri) = assertion.object_uri.as_ref() {
                        related
                            .push(json!({"uri":uri,"title":uri,"predicate":predicate,"direction":direction}));
                    }
                    let target = if outgoing {
                        assertion.object_uid.or(assertion.resolved_object_uid)
                    } else {
                        Some(assertion.subject_uid)
                    };
                    if let Some(target) = target {
                        if self
                            .refuse_unreadable(actor, std::slice::from_ref(&target))
                            .await
                            .is_err()
                        {
                            continue;
                        }
                        if let Some(row) = store::records::get(&self.store.pool, &target).await? {
                            related.push(json!({"uid":row.uid,"title":row.head,"quantity":row.quantity.to_string(),"predicate":predicate,"direction":direction,
                                "resource":store::records::get_extension(&self.store.pool,&row.uid,RESOURCE_NAMESPACE).await?}));
                        }
                    }
                }
                self.refuse_unreadable(actor, std::slice::from_ref(&record))
                    .await?;
                json!({"record":record,"resource":resource,"local":local,"related":related,"cached":{"bytes":cached.0,"total_bytes":cached.1},
                    "controls":store::records::get_extension(&self.store.pool,&record,"lince.controls").await?,
                    "binding":store::records::get_extension(&self.store.pool,&record,BINDING_NAMESPACE).await?})
            }
            Command::Download {
                record,
                representation,
            } => {
                let record = self.resolve(&record).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&record))
                    .await?;
                let mut resource: Resource = serde_json::from_value(
                    store::records::get_extension(&self.store.pool, &record, RESOURCE_NAMESPACE)
                        .await?
                        .ok_or_else(|| invalid("This Record has no resource reference"))?,
                )?;
                if let Some(index) = representation {
                    resource = resource.representation(index).map_err(invalid)?;
                }
                if actor.is_some()
                    && lince_resources::address(&resource.uri)
                        .is_ok_and(|uri| uri.scheme() == "file")
                {
                    return Err(EngineError::Forbidden(
                        "Local source files require the local owner session".into(),
                    ));
                }
                let library = library()?;
                let entry = tokio::task::spawn_blocking(move || library.fetch(&resource))
                    .await
                    .map_err(invalid)?
                    .map_err(invalid)?;
                json!({"record":record,"local":entry})
            }
            Command::RetainInterval {
                record,
                start_ms,
                end_ms,
                representation,
            } => {
                let record = self.resolve(&record).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&record))
                    .await?;
                let mut resource: Resource = serde_json::from_value(
                    store::records::get_extension(&self.store.pool, &record, RESOURCE_NAMESPACE)
                        .await?
                        .ok_or_else(|| invalid("This Record has no media reference"))?,
                )?;
                if let Some(index) = representation {
                    resource = resource.representation(index).map_err(invalid)?;
                }
                if !resource.media_type.starts_with("video/") {
                    return Err(invalid("Choose a video source"));
                }
                resource.locator.start_ms = Some(start_ms);
                resource.locator.end_ms = Some(end_ms);
                resource.validate().map_err(invalid)?;
                if actor.is_some()
                    && lince_resources::address(&resource.uri)
                        .is_ok_and(|uri| uri.scheme() == "file")
                {
                    return Err(EngineError::Forbidden(
                        "Local source files require the local owner session".into(),
                    ));
                }
                let library = library()?;
                let entry = tokio::task::spawn_blocking(move || {
                    lince_resources::video::retain(&library, &resource)
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                json!({"record":record,"local":entry,"interval":{"start_ms":start_ms,"end_ms":end_ms}})
            }
            Command::Register { record, path } => {
                if actor.is_some() {
                    return Err(EngineError::Forbidden(
                        "Local file registration requires the local owner session".into(),
                    ));
                }
                let record = self.resolve(&record).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&record))
                    .await?;
                let resource: Resource = serde_json::from_value(
                    store::records::get_extension(&self.store.pool, &record, RESOURCE_NAMESPACE)
                        .await?
                        .ok_or_else(|| invalid("This Record has no resource reference"))?,
                )?;
                let library = library()?;
                let entry = tokio::task::spawn_blocking(move || {
                    library.register(
                        &PathBuf::from(path),
                        &resource.uri,
                        &resource.media_type,
                        resource.hash.as_deref(),
                    )
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                json!({"record":record,"local":entry})
            }
            Command::Export {
                uri,
                selected,
                destination,
                media,
            } => {
                if actor.is_some() {
                    return Err(EngineError::Forbidden(
                        "Writing a local export requires the local owner session".into(),
                    ));
                }
                let source = self.source_publication(uri, selected, false, actor).await?;
                let library = library()?;
                let entry = tokio::task::spawn_blocking(move || {
                    source.export(&library, &PathBuf::from(destination), media)
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                json!({"entry":entry})
            }
            Command::Audit { uri, inverses } => {
                if actor.is_some() {
                    return Err(EngineError::Forbidden(
                        "A local link audit requires the local owner session".into(),
                    ));
                }
                if inverses.len() > 128 {
                    return Err(invalid("Choose at most 128 inverse predicate pairs"));
                }
                let uri = lince_resources::address(&uri).map_err(invalid)?;
                let path = uri
                    .to_file_path()
                    .map_err(|_| invalid("Choose a local folder to audit"))?;
                let library = library()?;
                let audit = tokio::task::spawn_blocking(move || {
                    Publication::audit(&library, &path, &inverses)
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                json!({"audit":audit,"records":[],"ready":false})
            }
            Command::Extract { uri, destination } => {
                if actor.is_some() {
                    return Err(EngineError::Forbidden(
                        "Archive extraction requires the local owner session".into(),
                    ));
                }
                let library = library()?;
                let destination = tokio::task::spawn_blocking(move || {
                    library.extract_zip(&uri, &PathBuf::from(destination))
                })
                .await
                .map_err(invalid)?
                .map_err(invalid)?;
                json!({"entry":lince_resources::Reference::path(&destination).map_err(invalid)?.target,"extracted":true})
            }
            Command::Activate { record } => self.activate_source(&record, actor).await?,
        };
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }
}

fn linked_body(body: &str, links: &BTreeMap<String, String>) -> String {
    let mut body = body.to_owned();
    for (name, uri) in links {
        body = body.replace(&format!("[[{name}]]"), &format!("[[{name}|{uri}]]"));
        body = body.replace(&format!("|{name}]]"), &format!("|{uri}]]"));
    }
    body
}
