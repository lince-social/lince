use crate::{
    app::{self, Mobile, button, input, label},
    navigation::Page,
};
use bevy::prelude::*;
use engine::actions::Action;
use nucleus::knowledge::{Command, UpdateField};
use serde_json::{Value, json};

pub struct State {
    sources: Vec<Value>,
    selected: Vec<String>,
    results: Vec<Value>,
    item: Value,
    output: Value,
    preview: Value,
    review: Value,
    saved: Option<String>,
    job: Option<String>,
    next_poll: std::time::Instant,
    fields: Vec<UpdateField>,
    page: usize,
    statement: Option<Value>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sources: vec![],
            selected: vec!["test".into()],
            results: vec![],
            item: Value::Null,
            output: Value::Null,
            preview: Value::Null,
            review: Value::Null,
            saved: None,
            job: None,
            next_poll: std::time::Instant::now(),
            fields: vec![],
            page: 0,
            statement: None,
        }
    }
}

impl State {
    pub(crate) fn stop(&mut self) {
        self.job = None;
    }
}

#[derive(Clone)]
pub enum Intent {
    Sources,
    Select(String),
    Search(u32),
    Read(String, String),
    Inspect(String),
    Cancel,
    Clear,
    Preview,
    Save,
    Export,
    Import,
    Install,
    Review,
    Field(UpdateField),
    Accept(bool),
    Page(bool),
    Statement(Option<usize>),
    Context,
    Wording,
    InspectMeaning(String),
}

fn act(world: &mut World, request: Command) -> Result<(), String> {
    app::act(world, Action::Knowledge { request }, None)
}

pub fn apply(world: &mut World, intent: Intent) -> Result<(), String> {
    let draft = |key: &str| {
        world.resource::<Mobile>().draft(
            "knowledge",
            key,
            if key == "context" { "General" } else { "" },
        )
    };
    let state = &world.resource::<Mobile>().knowledge;
    let item_id = || {
        state.item["id"]
            .as_str()
            .ok_or("Read source content first")
            .map(str::to_owned)
    };
    let source = || {
        state.item["source"]
            .as_str()
            .ok_or("Read source content first")
            .map(str::to_owned)
    };
    let organ = || {
        world
            .resource::<Mobile>()
            .rows
            .get("records")
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["kind"] == "organ" && row["slug"] == "local-organ")
            })
            .and_then(|row| row["uid"].as_str())
            .map(str::to_owned)
            .or_else(|| world.resource::<Mobile>().organ_uid())
            .ok_or("Destination Organ unavailable")
    };
    let saved = || state.saved.clone().ok_or("Choose a saved knowledge Record");
    let request = match intent {
        Intent::Statement(index) => {
            let claim = index
                .and_then(|index| state.item["claims"].get(index))
                .cloned();
            let mut mobile = world.resource_mut::<Mobile>();
            mobile.knowledge.statement = claim;
            mobile.knowledge.page = 0;
            return Ok(());
        }
        Intent::Select(source) => {
            let mut state = world.resource_mut::<Mobile>();
            if state.knowledge.selected.contains(&source) {
                state.knowledge.selected.retain(|known| known != &source);
            } else if state.knowledge.selected.len() < 8 {
                state.knowledge.selected.push(source);
            }
            return Ok(());
        }
        Intent::Field(field) => {
            let mut state = world.resource_mut::<Mobile>();
            if state.knowledge.fields.contains(&field) {
                state.knowledge.fields.retain(|known| known != &field);
            } else {
                state.knowledge.fields.push(field);
            }
            return Ok(());
        }
        Intent::Page(next) => {
            let mut state = world.resource_mut::<Mobile>();
            state.knowledge.page = if next {
                state.knowledge.page.saturating_add(1)
            } else {
                state.knowledge.page.saturating_sub(1)
            };
            return Ok(());
        }
        Intent::Sources => Command::Sources,
        Intent::Search(offset) => Command::Search {
            request_id: nucleus::new_uid("kr"),
            sources: state.selected.clone(),
            text: draft("search"),
            offset,
        },
        Intent::Read(source, item) => Command::Read {
            request_id: nucleus::new_uid("kr"),
            source,
            item,
        },
        Intent::Inspect(record) => Command::InspectSaved { record },
        Intent::Cancel => Command::Cancel {
            request_id: state.job.clone().ok_or("No source request is running")?,
        },
        Intent::Clear => Command::ClearCache { source: None },
        Intent::Preview => Command::PreviewSave {
            source: source()?,
            item: item_id()?,
            organ: organ()?,
            selection: Default::default(),
        },
        Intent::Save => Command::Save {
            preview: state.preview["preview_uid"]
                .as_str()
                .ok_or("Review the save preview first")?
                .into(),
        },
        Intent::Export => Command::Export { record: saved()? },
        Intent::Import => Command::Import {
            package: draft("package"),
            organ: organ()?,
        },
        Intent::Install => Command::InstallArchive {
            package: draft("package"),
        },
        Intent::Review => Command::ReviewUpdate { record: saved()? },
        Intent::Accept(fork) => Command::ApplyUpdate {
            review: state.review["review_uid"]
                .as_str()
                .ok_or("Review source changes first")?
                .into(),
            fields: state.fields.clone(),
            fork,
        },
        Intent::Context => {
            return app::act(
                world,
                Action::Vocabulary {
                    request: nucleus::vocabulary::Command::SetTerminologyContext {
                        context: draft("context"),
                    },
                },
                None,
            );
        }
        Intent::Wording => {
            return app::act(
                world,
                Action::Vocabulary {
                    request: nucleus::vocabulary::Command::UseWording {
                        concept: draft("concept"),
                        wording: draft("wording"),
                    },
                },
                None,
            );
        }
        Intent::InspectMeaning(concept) => {
            return app::act(
                world,
                Action::Vocabulary {
                    request: nucleus::vocabulary::Command::InspectMeaning { concept, offset: 0 },
                },
                None,
            );
        }
    };
    act(world, request)
}

pub fn receive(world: &mut World, data: Option<Value>) {
    let Some(data) = data else { return };
    let mut mobile = world.resource_mut::<Mobile>();
    let state = &mut mobile.knowledge;
    if let Some(sources) = data["sources"].as_array() {
        state.sources = sources.clone();
    }
    if let Some(job) = data["request_id"].as_str() {
        state.job = (data["complete"] != true).then(|| job.into());
        state.next_poll = std::time::Instant::now() + std::time::Duration::from_millis(500);
    }
    if let Some(results) = data["result"]["sources"].as_array() {
        state.results = results.clone();
    }
    if let Some(item) = data["result"]
        .get("item")
        .or_else(|| data.get("item"))
        .filter(|item| !item.is_null())
    {
        state.item = item.clone();
        state.statement = None;
        state.page = 0;
    }
    if data["preview_uid"].is_string() {
        state.preview = data.clone();
    }
    if data["review_uid"].is_string() {
        state.review = data.clone();
    }
    if let Some(uid) = data["record_uid"]
        .as_str()
        .or_else(|| data["record"]["uid"].as_str())
    {
        state.saved = Some(uid.into());
    }
    if data["cleared"] == true {
        state.item = Value::Null;
        state.results.clear();
        state.job = None;
        state.statement = None;
        state.preview = Value::Null;
        state.review = Value::Null;
    }
    state.output = data.clone();
    mobile.status = data["message"]
        .as_str()
        .unwrap_or(match data["status"].as_str() {
            Some("loading") => "Loading source content…",
            Some("cancelled") => "Source loading cancelled",
            _ => "Knowledge ready",
        })
        .into();
    if data["package"].is_object() {
        mobile
            .drafts
            .insert("knowledge/package".into(), data["package"].to_string());
    }
    mobile.dirty = true;
}

pub fn poll(world: &mut World) {
    let mobile = world.resource::<Mobile>();
    let Some(job) = mobile.knowledge.job.clone() else {
        return;
    };
    if !app::action_idle(world) || mobile.knowledge.next_poll > std::time::Instant::now() {
        return;
    }
    let request = if mobile.navigation.current == Page::Knowledge {
        Command::Poll { request_id: job }
    } else {
        Command::Cancel { request_id: job }
    };
    if let Err(error) = act(world, request) {
        let mut mobile = world.resource_mut::<Mobile>();
        mobile.knowledge.job = None;
        mobile.status = error;
    }
}

fn control(world: &mut World, parent: Entity, title: &str, intent: Intent) {
    button(world, parent, title, app::Intent::Knowledge(intent));
}

pub fn render(world: &mut World, parent: Entity) {
    input(
        world,
        parent,
        "knowledge",
        "search",
        "Search local Records and selected sources",
        "",
        false,
    );
    control(world, parent, "List sources", Intent::Sources);
    let state = &world.resource::<Mobile>().knowledge;
    let sources = state.sources.clone();
    let selected = state.selected.clone();
    let results = state.results.clone();
    let item = state.item.clone();
    let preview = state.preview.clone();
    let review = state.review.clone();
    let output = state.output.clone();
    let fields = state.fields.clone();
    let page = state.page;
    let statement = state.statement.clone();
    for source in sources {
        if let Some(id) = source["id"].as_str() {
            control(
                world,
                parent,
                &format!(
                    "{}{}",
                    if selected.iter().any(|known| known == id) {
                        "✓ "
                    } else {
                        ""
                    },
                    source["name"].as_str().unwrap_or(id)
                ),
                Intent::Select(id.into()),
            );
        }
    }
    for (title, intent) in [
        ("Search sources", Intent::Search(0)),
        ("Cancel loading", Intent::Cancel),
        ("Clear temporary content", Intent::Clear),
    ] {
        control(world, parent, title, intent);
    }
    let search = world
        .resource::<Mobile>()
        .draft("knowledge", "search", "")
        .to_lowercase();
    let records = world
        .resource::<Mobile>()
        .rows
        .get("records")
        .cloned()
        .unwrap_or_default();
    label(world, parent, "Local Records", 20.0);
    for record in records
        .iter()
        .filter(|record| {
            record["head"]
                .as_str()
                .is_some_and(|head| head.to_lowercase().contains(&search))
        })
        .take(20)
    {
        if let Some(uid) = record["uid"].as_str() {
            control(
                world,
                parent,
                record["head"].as_str().unwrap_or(uid),
                Intent::Inspect(uid.into()),
            );
        }
    }
    for result in results {
        label(
            world,
            parent,
            &format!(
                "{} · {}",
                result["source"],
                if result["complete"] == true {
                    "results complete"
                } else {
                    "more results available"
                }
            ),
            16.0,
        );
        for entry in result["items"].as_array().into_iter().flatten().take(20) {
            if let (Some(source), Some(id)) = (entry["source"].as_str(), entry["id"].as_str()) {
                control(
                    world,
                    parent,
                    entry["title"].as_str().unwrap_or(id),
                    Intent::Read(source.into(), id.into()),
                );
            }
        }
        if let Some(offset) = result["next_offset"].as_u64() {
            control(world, parent, "More results", Intent::Search(offset as u32));
        }
    }
    if let Some(title) = item["title"].as_str() {
        label(world, parent, title, 22.0);
        label(
            world,
            parent,
            &format!(
                "{} · revision {}\n{}",
                item["source"], item["revision"], item["uri"]
            ),
            14.0,
        );
        let full_statement = statement
            .as_ref()
            .map(|value| serde_json::to_string_pretty(value).unwrap_or_default());
        let content = full_statement
            .as_deref()
            .unwrap_or_else(|| item["content"].as_str().unwrap_or(""));
        if full_statement.is_some() {
            control(
                world,
                parent,
                "Return to source content",
                Intent::Statement(None),
            );
        }
        let (text, more) = lince_interface::text::reading_page(content, page, 8000);
        for paragraph in text.split_inclusive(['\n', '\u{2028}', '\u{2029}']) {
            let paragraph = paragraph.trim_end_matches(['\r', '\n', '\u{2028}', '\u{2029}']);
            label(
                world,
                parent,
                if paragraph.is_empty() { " " } else { paragraph },
                16.0,
            );
        }
        label(
            world,
            parent,
            &format!("License: {}\nCredits: {}", item["license"], item["credits"]),
            14.0,
        );
        for dependency in item["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .take(128)
        {
            if let (Some(source), Some(id)) =
                (dependency["source"].as_str(), dependency["id"].as_str())
            {
                control(
                    world,
                    parent,
                    &format!("Read related: {}", dependency["title"]),
                    Intent::Read(source.into(), id.into()),
                );
            }
        }
        for claim in item["claims"]
            .as_array()
            .into_iter()
            .flatten()
            .skip(page * 20)
            .take(20)
        {
            label(
                world,
                parent,
                &claim.to_string().chars().take(8000).collect::<String>(),
                14.0,
            );
        }
        for (index, claim) in item["claims"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .skip(page * 20)
            .take(20)
        {
            control(
                world,
                parent,
                &format!("Read complete statement {}", claim["id"]),
                Intent::Statement(Some(index)),
            );
        }
        if page > 0 {
            control(world, parent, "Previous content", Intent::Page(false));
        }
        if more
            || item["claims"]
                .as_array()
                .is_some_and(|claims| claims.len() > (page + 1) * 20)
        {
            control(world, parent, "More content", Intent::Page(true));
        }
    }
    for (title, intent) in [
        ("Preview saving into this Organ", Intent::Preview),
        ("Save reviewed content", Intent::Save),
        ("Export with dependencies", Intent::Export),
        ("Review source update", Intent::Review),
    ] {
        control(world, parent, title, intent);
    }
    if preview.is_object() {
        label(world,parent,&json!({"bytes":preview["preview"]["estimated_bytes"],"missing":preview["preview"]["missing"],"can_save":preview["preview"]["can_save"]}).to_string(),14.0);
    }
    if review.is_object() {
        label(
            world,
            parent,
            &review["review"]
                .to_string()
                .chars()
                .take(16000)
                .collect::<String>(),
            14.0,
        );
    }
    for field in [
        UpdateField::Title,
        UpdateField::Content,
        UpdateField::Claims,
        UpdateField::Attribution,
        UpdateField::Assets,
    ] {
        control(
            world,
            parent,
            &format!(
                "{}{field:?}",
                if fields.contains(&field) { "✓ " } else { "" }
            ),
            Intent::Field(field),
        );
    }
    control(
        world,
        parent,
        "Accept selected changes",
        Intent::Accept(false),
    );
    control(
        world,
        parent,
        "Create separate local copy",
        Intent::Accept(true),
    );
    let package = input(
        world,
        parent,
        "knowledge",
        "package",
        "Offline archive or portable package",
        "",
        true,
    );
    world
        .get_mut::<bevy::text::EditableText>(package)
        .unwrap()
        .max_characters = None;
    control(world, parent, "Install offline archive", Intent::Install);
    control(world, parent, "Import portable knowledge", Intent::Import);
    label(world, parent, "Vocabulary wording", 20.0);
    input(
        world,
        parent,
        "knowledge",
        "context",
        "My terminology context",
        "General",
        false,
    );
    control(world, parent, "Use context", Intent::Context);
    input(
        world,
        parent,
        "knowledge",
        "concept",
        "Stable Concept identity",
        "",
        false,
    );
    input(
        world,
        parent,
        "knowledge",
        "wording",
        "My wording",
        "",
        false,
    );
    control(world, parent, "Use my wording", Intent::Wording);
    let concepts = world
        .resource::<Mobile>()
        .rows
        .get("knowledge_concepts")
        .cloned()
        .unwrap_or_default();
    for concept in concepts.iter().take(20) {
        if let Some(uid) = concept["uid"].as_str() {
            control(
                world,
                parent,
                concept["name"].as_str().unwrap_or(uid),
                Intent::InspectMeaning(uid.into()),
            );
        }
    }
    if output["meaning"].is_object() {
        label(
            world,
            parent,
            &output["meaning"]
                .to_string()
                .chars()
                .take(16000)
                .collect::<String>(),
            14.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_replies_keep_saved_reference_and_full_multilingual_content() {
        let mut world = World::new();
        world.insert_resource(Mobile::new("unused-test-directory".into()));
        receive(
            &mut world,
            Some(
                json!({"item":{"source":"test","id":"1","title":"العربية 日本語","content":"café"},"record":{"uid":"r_saved"},"saved":true,"offline":true}),
            ),
        );
        assert_eq!(
            world.resource::<Mobile>().knowledge.saved.as_deref(),
            Some("r_saved")
        );
        assert_eq!(
            world.resource::<Mobile>().knowledge.item["title"],
            "العربية 日本語"
        );
        receive(
            &mut world,
            Some(json!({"request_id":"kr_1","status":"cancelled","complete":true})),
        );
        assert!(world.resource::<Mobile>().knowledge.job.is_none());
    }

    #[test]
    fn narrow_mobile_page_keeps_touch_and_keyboard_controls_and_bounds_reader_text() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<lince_interface::theme::Typography>();
        world.insert_resource(Mobile::new("unused-test-directory".into()));
        world.resource_mut::<Mobile>().knowledge.item = json!({"title":"العربية 日本語","content":"café ".repeat(100000),"source":"test","revision":"1"});
        let parent = world
            .spawn(Node {
                width: px(320),
                height: px(640),
                ..default()
            })
            .id();
        render(&mut world, parent);
        for (node, _) in world
            .query::<(&Node, &crate::app::ButtonIntent)>()
            .iter(&world)
        {
            assert!(node.min_height == px(48));
        }
        assert!(
            world
                .query_filtered::<Entity, With<bevy::input_focus::tab_navigation::TabIndex>>()
                .iter(&world)
                .count()
                > 20
        );
        let texts = world
            .query::<&Text>()
            .iter(&world)
            .map(|text| text.0.clone())
            .collect::<Vec<_>>();
        assert!(texts.iter().any(|text| text.contains("العربية 日本語")));
        assert!(texts.iter().all(|text| text.chars().count() <= 16000));
        assert!(world.entities().count_spawned() < 300);
    }
}
