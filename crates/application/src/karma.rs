use domain::clean::{frequency::Frequency, karma::Karma};
use injection::cross_cutting::InjectedServices;
use persistence::write_coordinator::SqlParameter;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use utils::info;
use utils::logging::{LogEntry, log};

use crate::{
    command::{CommandOrigin, karma_execute_command, spawn_command_buffer_session_by_id},
    engine::return_engine,
    frequency::frequency_check,
    query::query_execute,
    write::execute_statement,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io::{Error, ErrorKind},
};

pub async fn karma_deliver(services: InjectedServices, vec_karma: Vec<Karma>) -> Result<(), Error> {
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();
    let regex_sync = Regex::new(r"sr(?P<scope>nt|t|n)(?P<fields>q?h?b?)(?P<id>\d+)").unwrap();
    let regex_sync_exact =
        Regex::new(r"^sr(?P<scope>nt|t|n)(?P<fields>q?h?b?)(?P<id>\d+)$").unwrap();

    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();
    let mut prepared = Vec::with_capacity(vec_karma.len());

    for karma in vec_karma {
        let mut condition = karma.condition.clone();

        condition = replace_record_quantities(&services, &regex_record_quantity, condition).await?;
        condition = replace_frequencies(
            &services,
            &regex_frequency,
            condition,
            &mut frequencies_to_update,
        )
        .await?;
        condition = replace_commands(&services, &regex_command, condition).await?;
        condition = replace_sync_tokens(&services, &regex_sync, condition).await?;

        prepared.push((karma, format!("({condition}) * 1.0")));
    }

    let evaluated = tokio::task::spawn_blocking(move || {
        let engine = return_engine();
        prepared
            .into_iter()
            .map(|(karma, expr)| {
                let value = engine.eval::<f64>(&expr).unwrap_or(0.0);
                (karma, value)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(Error::other)?;

    for (karma, condition) in evaluated {
        let operator = karma.operator.as_str();
        if !((operator == "=" && condition != 0.0) || operator == "=*") {
            continue;
        }

        if let Some(token) = parse_sync_token(&regex_sync_exact, &karma.consequence) {
            if let Err(error) = execute_sync_consequence(services.clone(), &token).await {
                log(LogEntry::Error(
                    error.kind(),
                    format!("sync consequence error on karma id {}: {}", karma.id, error),
                ));
            }
            continue;
        }

        if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                if let Err(e) =
                    crate::write::set_record_quantity(services.clone(), id, condition).await
                {
                    log(LogEntry::Error(
                        e.kind(),
                        format!("record quantity error on karma id {}", karma.id),
                    ));
                }
            }
        }

        if let Some(caps) = regex_command.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                info!("Running command {}", id);
                let services = services.clone();
                let karma_id = karma.id;
                tokio::spawn(async move {
                    if let Err(e) = spawn_command_buffer_session_by_id(
                        services,
                        id,
                        CommandOrigin::Consequence(karma_id),
                    )
                    .await
                    {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!("Command returned None for karma id {}: {}", karma_id, e),
                        ));
                    }
                });
            }
        }

        if let Some(caps) = regex_query.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                if let Err(e) = query_execute(services.clone(), id).await {
                    log(LogEntry::Error(
                        ErrorKind::Other,
                        format!("Query error on karma id {}: {e}", karma.id),
                    ));
                }
            }
        }
    }

    for (_, frequency) in frequencies_to_update {
        let _ = crate::write::update_frequency(services.clone(), frequency).await;
    }

    Ok(())
}

async fn replace_record_quantities(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let value = services
            .repository
            .record
            .get_by_id(id)
            .await
            .map(|r| r.quantity)
            .unwrap_or(0.0);

        output = output.replace(&caps[0], &value.to_string());
    }

    Ok(output)
}

async fn replace_frequencies(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
    frequencies_to_update: &mut HashMap<u32, Frequency>,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let (replacement, freq_opt) = frequency_check(services.clone(), id).await;

        if let Some(f) = freq_opt {
            frequencies_to_update.insert(f.id, f);
        }

        output = output.replace(&caps[0], &replacement.to_string());
    }

    Ok(output)
}

async fn replace_commands(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let id = caps[1].parse::<u32>().unwrap();
        let value = karma_execute_command(services.clone(), id)
            .await
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string());

        output = output.replace(&caps[0], &value);
    }

    Ok(output)
}

async fn replace_sync_tokens(
    services: &InjectedServices,
    regex: &Regex,
    input: String,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let Some(token) = parse_sync_captures(&caps) else {
            continue;
        };
        let exists = services
            .repository
            .record
            .get_by_id(token.record_id as u32)
            .await
            .is_ok();
        output = output.replace(&caps[0], if exists { "1" } else { "0" });
    }

    Ok(output)
}

#[derive(Debug, Clone, Copy)]
struct SyncFields {
    quantity: bool,
    head: bool,
    body: bool,
}

impl SyncFields {
    fn from_segment(segment: &str) -> Option<Self> {
        if !segment
            .chars()
            .all(|ch| ch == 'q' || ch == 'h' || ch == 'b')
        {
            return None;
        }
        let quantity = segment.contains('q');
        let head = segment.contains('h');
        let body = segment.contains('b');
        if segment.is_empty() {
            Some(Self {
                quantity: true,
                head: true,
                body: true,
            })
        } else {
            Some(Self {
                quantity,
                head,
                body,
            })
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SyncScope {
    Node,
    Tree,
    NodeAndTree,
}

impl SyncScope {
    fn from_segment(segment: &str) -> Option<Self> {
        match segment {
            "n" => Some(Self::Node),
            "t" => Some(Self::Tree),
            "nt" => Some(Self::NodeAndTree),
            _ => None,
        }
    }

    fn includes_node(self) -> bool {
        matches!(self, Self::Node | Self::NodeAndTree)
    }

    fn includes_tree(self) -> bool {
        matches!(self, Self::Tree | Self::NodeAndTree)
    }
}

#[derive(Debug, Clone, Copy)]
struct SyncToken {
    scope: SyncScope,
    fields: SyncFields,
    record_id: i64,
}

fn parse_sync_token(regex: &Regex, input: &str) -> Option<SyncToken> {
    regex.captures(input).and_then(|caps| parse_sync_captures(&caps))
}

fn parse_sync_captures(caps: &regex::Captures<'_>) -> Option<SyncToken> {
    Some(SyncToken {
        scope: SyncScope::from_segment(caps.name("scope")?.as_str())?,
        fields: SyncFields::from_segment(caps.name("fields").map(|value| value.as_str()).unwrap_or(""))?,
        record_id: caps.name("id")?.as_str().parse::<i64>().ok()?,
    })
}

#[derive(Debug, Clone, FromRow)]
struct SqlRecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct SqlRecordLinkRow {
    id: i64,
    record_id: i64,
    link_type: String,
    target_table: String,
    target_id: i64,
}

#[derive(Debug, Clone, FromRow)]
struct SqlRecordExtensionRow {
    id: i64,
    record_id: i64,
    namespace: String,
    freestyle_data_structure: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TrailSyncExtension {
    trail_root_record_id: i64,
    sync_source_record_id: i64,
}

async fn execute_sync_consequence(
    services: InjectedServices,
    token: &SyncToken,
) -> Result<(), Error> {
    let context = load_sync_context(&services, token.record_id).await?;
    let root_categories = context
        .categories_by_record
        .get(&context.trail_root_record_id)
        .cloned()
        .unwrap_or_default();
    let root_assignees = context
        .assignees_by_record
        .get(&context.trail_root_record_id)
        .cloned()
        .unwrap_or_default();

    if token.scope.includes_node() {
        sync_existing_record_fields(
            services.clone(),
            &context,
            token.record_id,
            context.source_root_record_id,
            token.fields,
        )
        .await?;
    }

    if token.scope.includes_tree() {
        sync_descendants(
            services,
            &context,
            token.record_id,
            context.source_root_record_id,
            token.fields,
            &root_categories,
            &root_assignees,
        )
        .await?;
    }

    Ok(())
}

struct SyncContext {
    records_by_id: BTreeMap<i64, SqlRecordRow>,
    parent_links: Vec<SqlRecordLinkRow>,
    all_links: Vec<SqlRecordLinkRow>,
    all_extensions: Vec<SqlRecordExtensionRow>,
    trail_root_record_id: i64,
    source_root_record_id: i64,
    copies_by_source_id: BTreeMap<i64, i64>,
    categories_by_record: BTreeMap<i64, Vec<String>>,
    assignees_by_record: BTreeMap<i64, Vec<i64>>,
}

async fn load_sync_context(
    services: &InjectedServices,
    copied_root_id: i64,
) -> Result<SyncContext, Error> {
    let db = &*services.db;
    let records = sqlx::query_as::<_, SqlRecordRow>("SELECT id, quantity, head, body FROM record")
        .fetch_all(db)
        .await
        .map_err(Error::other)?;
    let all_links = sqlx::query_as::<_, SqlRecordLinkRow>(
        "SELECT id, record_id, link_type, target_table, target_id FROM record_link",
    )
    .fetch_all(db)
    .await
    .map_err(Error::other)?;
    let all_extensions = sqlx::query_as::<_, SqlRecordExtensionRow>(
        "SELECT id, record_id, namespace, freestyle_data_structure FROM record_extension",
    )
    .fetch_all(db)
    .await
    .map_err(Error::other)?;

    let records_by_id = records.into_iter().map(|row| (row.id, row)).collect::<BTreeMap<_, _>>();
    let trail_extension = all_extensions
        .iter()
        .find(|row| row.record_id == copied_root_id && row.namespace == "trail.sync")
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing trail.sync extension"))?;
    let trail_sync = serde_json::from_str::<TrailSyncExtension>(&trail_extension.freestyle_data_structure)
        .map_err(Error::other)?;

    let parent_links = all_links
        .iter()
        .filter(|row| row.link_type == "parent" && row.target_table == "record")
        .cloned()
        .collect::<Vec<_>>();
    let categories_by_record = build_categories_by_record(&all_extensions);
    let assignees_by_record = build_assignees_by_record(&all_links);
    let copies_by_source_id = all_extensions
        .iter()
        .filter(|row| row.namespace == "trail.sync")
        .filter_map(|row| {
            let parsed = serde_json::from_str::<TrailSyncExtension>(&row.freestyle_data_structure).ok()?;
            (parsed.trail_root_record_id == trail_sync.trail_root_record_id)
                .then_some((parsed.sync_source_record_id, row.record_id))
        })
        .collect::<BTreeMap<_, _>>();

    Ok(SyncContext {
        records_by_id,
        parent_links,
        all_links,
        all_extensions,
        trail_root_record_id: trail_sync.trail_root_record_id,
        source_root_record_id: trail_sync.sync_source_record_id,
        copies_by_source_id,
        categories_by_record,
        assignees_by_record,
    })
}

async fn sync_existing_record_fields(
    services: InjectedServices,
    context: &SyncContext,
    copied_record_id: i64,
    source_record_id: i64,
    fields: SyncFields,
) -> Result<(), Error> {
    let source = context
        .records_by_id
        .get(&source_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing source record for sync"))?;
    let copy = context
        .records_by_id
        .get(&copied_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing copied record for sync"))?;

    let mut assignments = Vec::new();
    let mut params = Vec::new();

    if fields.quantity && copy.quantity != source.quantity {
        assignments.push("quantity = ?");
        params.push(SqlParameter::Real(source.quantity));
    }
    if fields.head && copy.head != source.head {
        assignments.push("head = ?");
        params.push(match &source.head {
            Some(value) => SqlParameter::Text(value.clone()),
            None => SqlParameter::Null,
        });
    }
    if fields.body && copy.body != source.body {
        assignments.push("body = ?");
        params.push(match &source.body {
            Some(value) => SqlParameter::Text(value.clone()),
            None => SqlParameter::Null,
        });
    }

    if assignments.is_empty() {
        return Ok(());
    }

    params.push(SqlParameter::Integer(copied_record_id));
    execute_statement(
        services,
        format!("UPDATE record SET {} WHERE id = ?", assignments.join(", ")),
        params,
    )
    .await
    .map(|_| ())
}

async fn sync_descendants(
    services: InjectedServices,
    context: &SyncContext,
    copied_root_id: i64,
    source_root_id: i64,
    fields: SyncFields,
    root_categories: &[String],
    root_assignees: &[i64],
) -> Result<(), Error> {
    let mut copies_by_source = context.copies_by_source_id.clone();
    copies_by_source.insert(source_root_id, copied_root_id);

    let mut queue = VecDeque::from([(source_root_id, copied_root_id)]);
    while let Some((current_source_id, current_copy_id)) = queue.pop_front() {
        let expected_source_children = context
            .parent_links
            .iter()
            .filter(|row| row.target_id == current_source_id)
            .map(|row| row.record_id)
            .collect::<Vec<_>>();

        let mut expected_copy_children = BTreeSet::new();

        for child_source_id in expected_source_children.iter().copied() {
            let child_copy_id = if let Some(existing_id) = copies_by_source.get(&child_source_id).copied() {
                sync_existing_record_fields(
                    services.clone(),
                    context,
                    existing_id,
                    child_source_id,
                    fields,
                )
                .await?;
                sync_categories(
                    services.clone(),
                    &context.all_extensions,
                    existing_id,
                    root_categories,
                )
                .await?;
                sync_assignees(
                    services.clone(),
                    &context.all_links,
                    existing_id,
                    root_assignees,
                )
                .await?;
                existing_id
            } else {
                let created_id = create_copied_record(
                    services.clone(),
                    context,
                    child_source_id,
                    context.trail_root_record_id,
                    fields,
                    root_categories,
                    root_assignees,
                )
                .await?;
                copies_by_source.insert(child_source_id, created_id);
                created_id
            };

            ensure_parent_link(services.clone(), context, child_copy_id, current_copy_id).await?;
            expected_copy_children.insert(child_copy_id);
            queue.push_back((child_source_id, child_copy_id));
        }

        repair_parent_links(
            services.clone(),
            context,
            current_copy_id,
            &expected_copy_children,
            context.trail_root_record_id,
        )
        .await?;
    }

    Ok(())
}

async fn create_copied_record(
    services: InjectedServices,
    context: &SyncContext,
    source_record_id: i64,
    trail_root_record_id: i64,
    fields: SyncFields,
    root_categories: &[String],
    root_assignees: &[i64],
) -> Result<i64, Error> {
    let source = context
        .records_by_id
        .get(&source_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing source record for copied node"))?;
    let quantity = if fields.quantity {
        source.quantity
    } else {
        0.0
    };
    let inserted = execute_statement(
        services.clone(),
        "INSERT INTO record(quantity, head, body) VALUES (?, ?, ?)",
        vec![
            SqlParameter::Real(quantity),
            match &source.head {
                Some(value) => SqlParameter::Text(value.clone()),
                None => SqlParameter::Null,
            },
            match &source.body {
                Some(value) => SqlParameter::Text(value.clone()),
                None => SqlParameter::Null,
            },
        ],
    )
    .await?;
    let copied_record_id = inserted.last_insert_rowid.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "Insert into record did not return last_insert_rowid",
        )
    })?;

    upsert_extension(
        services.clone(),
        &context.all_extensions,
        copied_record_id,
        "trail.sync",
        &json!({
            "trail_root_record_id": trail_root_record_id,
            "sync_source_record_id": source_record_id,
        })
        .to_string(),
    )
    .await?;
    sync_categories(services.clone(), &context.all_extensions, copied_record_id, root_categories).await?;
    sync_assignees(services, &context.all_links, copied_record_id, root_assignees).await?;
    Ok(copied_record_id)
}

async fn ensure_parent_link(
    services: InjectedServices,
    context: &SyncContext,
    child_id: i64,
    parent_id: i64,
) -> Result<(), Error> {
    let exists = context.all_links.iter().any(|row| {
        row.record_id == child_id
            && row.link_type == "parent"
            && row.target_table == "record"
            && row.target_id == parent_id
    });
    if exists {
        return Ok(());
    }
    execute_statement(
        services,
        "INSERT INTO record_link(record_id, link_type, target_table, target_id, position, freestyle_data_structure) VALUES (?, 'parent', 'record', ?, NULL, NULL)",
        vec![SqlParameter::Integer(child_id), SqlParameter::Integer(parent_id)],
    )
    .await
    .map(|_| ())
}

async fn repair_parent_links(
    services: InjectedServices,
    context: &SyncContext,
    current_copy_id: i64,
    expected_copy_children: &BTreeSet<i64>,
    trail_root_record_id: i64,
) -> Result<(), Error> {
    for link in context.all_links.iter().filter(|row| {
        row.link_type == "parent" && row.target_table == "record" && row.target_id == current_copy_id
    }) {
        let belongs_to_same_trail = context
            .all_extensions
            .iter()
            .find(|extension| extension.record_id == link.record_id && extension.namespace == "trail.sync")
            .and_then(|extension| serde_json::from_str::<TrailSyncExtension>(&extension.freestyle_data_structure).ok())
            .is_some_and(|meta| meta.trail_root_record_id == trail_root_record_id);
        if belongs_to_same_trail && !expected_copy_children.contains(&link.record_id) {
            execute_statement(
                services.clone(),
                "DELETE FROM record_link WHERE id = ?",
                vec![SqlParameter::Integer(link.id)],
            )
            .await?;
        }
    }
    Ok(())
}

async fn upsert_extension(
    services: InjectedServices,
    extensions: &[SqlRecordExtensionRow],
    record_id: i64,
    namespace: &str,
    freestyle_data_structure: &str,
) -> Result<(), Error> {
    if let Some(existing) = extensions
        .iter()
        .find(|row| row.record_id == record_id && row.namespace == namespace)
    {
        execute_statement(
            services,
            "UPDATE record_extension SET version = 1, freestyle_data_structure = ? WHERE id = ?",
            vec![
                SqlParameter::Text(freestyle_data_structure.to_string()),
                SqlParameter::Integer(existing.id),
            ],
        )
        .await
        .map(|_| ())
    } else {
        execute_statement(
            services,
            "INSERT INTO record_extension(record_id, namespace, version, freestyle_data_structure) VALUES (?, ?, 1, ?)",
            vec![
                SqlParameter::Integer(record_id),
                SqlParameter::Text(namespace.to_string()),
                SqlParameter::Text(freestyle_data_structure.to_string()),
            ],
        )
        .await
        .map(|_| ())
    }
}

async fn sync_categories(
    services: InjectedServices,
    extensions: &[SqlRecordExtensionRow],
    record_id: i64,
    categories: &[String],
) -> Result<(), Error> {
    let normalized = normalize_categories(categories.to_vec());
    if normalized.is_empty() {
        return Ok(());
    }
    upsert_extension(
        services,
        extensions,
        record_id,
        "task.categories",
        &json!({ "categories": normalized }).to_string(),
    )
    .await
}

async fn sync_assignees(
    services: InjectedServices,
    links: &[SqlRecordLinkRow],
    record_id: i64,
    assignee_ids: &[i64],
) -> Result<(), Error> {
    let desired = assignee_ids
        .iter()
        .copied()
        .filter(|value| *value > 0)
        .collect::<BTreeSet<_>>();
    let existing = links
        .iter()
        .filter(|row| {
            row.record_id == record_id
                && row.link_type == "assigned_to"
                && row.target_table == "app_user"
        })
        .collect::<Vec<_>>();
    for row in &existing {
        if !desired.contains(&row.target_id) {
            execute_statement(
                services.clone(),
                "DELETE FROM record_link WHERE id = ?",
                vec![SqlParameter::Integer(row.id)],
            )
            .await?;
        }
    }
    for assignee_id in desired {
        if existing.iter().any(|row| row.target_id == assignee_id) {
            continue;
        }
        execute_statement(
            services.clone(),
            "INSERT INTO record_link(record_id, link_type, target_table, target_id, position, freestyle_data_structure) VALUES (?, 'assigned_to', 'app_user', ?, NULL, NULL)",
            vec![
                SqlParameter::Integer(record_id),
                SqlParameter::Integer(assignee_id),
            ],
        )
        .await?;
    }
    Ok(())
}

fn build_categories_by_record(
    extensions: &[SqlRecordExtensionRow],
) -> BTreeMap<i64, Vec<String>> {
    extensions
        .iter()
        .filter(|row| row.namespace == "task.categories")
        .filter_map(|row| {
            let categories = serde_json::from_str::<Value>(&row.freestyle_data_structure)
                .ok()?
                .get("categories")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
            Some((row.record_id, categories))
        })
        .collect()
}

fn build_assignees_by_record(
    links: &[SqlRecordLinkRow],
) -> BTreeMap<i64, Vec<i64>> {
    let mut map = BTreeMap::<i64, Vec<i64>>::new();
    for link in links {
        if link.link_type == "assigned_to" && link.target_table == "app_user" {
            map.entry(link.record_id).or_default().push(link.target_id);
        }
    }
    map
}

fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for category in categories {
        let trimmed = category.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_lowercase();
        if seen.insert(lowered) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}
