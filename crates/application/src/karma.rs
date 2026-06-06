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
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    io::{Error, ErrorKind},
};

pub async fn refresh_karma_cache(services: InjectedServices) -> Result<(), Error> {
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_sync = sync_token_regex();
    let vec_karma = services.repository.karma.get(None).await?;

    let mut karma_by_record: HashMap<u32, Vec<Karma>> = HashMap::new();

    for karma in vec_karma {
        let mut ids =
            extract_karma_record_ids(&regex_record_quantity, &regex_sync, &karma.condition);
        ids.sort_unstable();
        ids.dedup();
        for record_id in ids {
            karma_by_record
                .entry(record_id)
                .or_default()
                .push(karma.clone());
        }
    }

    for vec_karma in karma_by_record.values_mut() {
        vec_karma.sort_by_key(|karma| karma.id);
        vec_karma.dedup_by_key(|karma| karma.id);
    }

    services.karma_cache.replace(karma_by_record);
    Ok(())
}

pub async fn deliver_record_karma(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
) -> Result<(), Error> {
    let mut ordered_record_ids = record_ids.into_iter().collect::<Vec<_>>();
    ordered_record_ids.sort_unstable();
    ordered_record_ids.dedup();

    for record_id in ordered_record_ids {
        let vec_karma = services.karma_cache.karma_for_record(record_id);
        if vec_karma.is_empty() {
            continue;
        }
        karma_deliver(services.clone(), vec_karma).await?;
    }

    Ok(())
}

pub async fn karma_deliver(services: InjectedServices, vec_karma: Vec<Karma>) -> Result<(), Error> {
    let regex_record_quantity = Regex::new(r"rq(\d+)").unwrap();
    let regex_frequency = Regex::new(r"f(\d+)").unwrap();
    let regex_command = Regex::new(r"c(\d+)").unwrap();
    let regex_query = Regex::new(r"sql(\d+)").unwrap();
    let regex_sync = sync_token_regex();
    let regex_sync_exact = sync_token_exact_regex();

    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();
    let mut sync_snapshots: HashMap<SyncToken, SyncSourceTree> = HashMap::new();
    let active_rule_count = services
        .repository
        .karma
        .get_active(None)
        .await
        .map(|rules| rules.len())
        .unwrap_or(vec_karma.len());
    let max_cascade_steps = std::cmp::max(32, active_rule_count * 4);
    let mut queue = VecDeque::from(vec_karma);
    let mut seen_states = HashSet::<(u32, u32, String)>::new();
    let mut steps = 0usize;

    while let Some(karma) = queue.pop_front() {
        if steps >= max_cascade_steps {
            log(LogEntry::Error(
                ErrorKind::Other,
                format!(
                    "Karma cascade stopped after {max_cascade_steps} steps to avoid an infinite loop"
                ),
            ));
            break;
        }
        steps += 1;

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
        condition =
            replace_sync_tokens(&services, &regex_sync, condition, &mut sync_snapshots).await?;

        let expr = format!("({condition}) * 1.0");
        let condition = tokio::task::spawn_blocking(move || {
            let engine = return_engine();
            engine.eval::<f64>(&expr).unwrap_or(0.0)
        })
        .await
        .map_err(Error::other)?;

        let operator = karma.operator.as_str();
        if !((operator == "=" && condition != 0.0) || operator == "=*") {
            continue;
        }

        if let Some(token) = parse_sync_token(&regex_sync_exact, &karma.consequence) {
            if let Err(error) =
                execute_sync_consequence(services.clone(), &token, &mut sync_snapshots).await
            {
                log(LogEntry::Error(
                    error.kind(),
                    format!("sync consequence error on karma id {}: {}", karma.id, error),
                ));
                return Err(Error::new(
                    error.kind(),
                    format!("sync consequence error on karma id {}: {}", karma.id, error),
                ));
            }
            continue;
        }

        if let Some(caps) = regex_record_quantity.captures(&karma.consequence) {
            if let Ok(id) = caps[1].parse::<u32>() {
                if let Err(e) = execute_statement(
                    services.clone(),
                    "UPDATE record SET quantity = ? WHERE id = ?",
                    vec![
                        SqlParameter::Real(condition),
                        SqlParameter::Integer(id as i64),
                    ],
                )
                .await
                {
                    log(LogEntry::Error(
                        e.kind(),
                        format!("record quantity error on karma id {}", karma.id),
                    ));
                } else {
                    let state = (karma.id, id, format!("{condition:.6}"));
                    if !seen_states.insert(state) {
                        log(LogEntry::Error(
                            ErrorKind::Other,
                            format!(
                                "Karma cascade repeated state for karma id {} and record {}",
                                karma.id, id
                            ),
                        ));
                        continue;
                    }
                    for dependent in services.karma_cache.karma_for_record(id) {
                        queue.push_back(dependent);
                    }
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

fn extract_karma_record_ids(
    regex_record_quantity: &Regex,
    regex_sync: &Regex,
    condition: &str,
) -> Vec<u32> {
    let mut record_ids = Vec::new();

    for caps in regex_record_quantity.captures_iter(condition) {
        if let Ok(id) = caps[1].parse::<u32>() {
            record_ids.push(id);
        }
    }

    for caps in regex_sync.captures_iter(condition) {
        let Some(token) = parse_sync_captures(&caps) else {
            continue;
        };
        record_ids.push(token.record_id as u32);
    }

    record_ids
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
    sync_snapshots: &mut HashMap<SyncToken, SyncSourceTree>,
) -> Result<String, Error> {
    let mut output = input.clone();

    for caps in regex.captures_iter(&input) {
        let Some(token) = parse_sync_captures(&caps) else {
            continue;
        };
        let exists = if token.organ_id.is_some() {
            load_sync_source_tree(services, &token, sync_snapshots)
                .await
                .is_ok()
        } else {
            services
                .repository
                .record
                .get_by_id(token.record_id as u32)
                .await
                .is_ok()
        };
        output = output.replace(&caps[0], if exists { "1" } else { "0" });
    }

    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    fn from_human_segment(segment: &str) -> Option<Self> {
        let parts = segment.split('-').collect::<Vec<_>>();
        if !parts
            .iter()
            .all(|part| matches!(*part, "quantity" | "head" | "body"))
        {
            return None;
        }
        Some(Self {
            quantity: parts.contains(&"quantity"),
            head: parts.contains(&"head"),
            body: parts.contains(&"body"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    fn from_human_segment(segment: &str) -> Option<Self> {
        match segment {
            "node" => Some(Self::Node),
            "tree" => Some(Self::Tree),
            "node-and-tree" => Some(Self::NodeAndTree),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SyncToken {
    organ_id: Option<i64>,
    scope: SyncScope,
    fields: SyncFields,
    record_id: i64,
}

fn parse_sync_token(regex: &Regex, input: &str) -> Option<SyncToken> {
    regex
        .captures(input)
        .and_then(|caps| parse_sync_captures(&caps))
}

fn parse_sync_captures(caps: &regex::Captures<'_>) -> Option<SyncToken> {
    if let Some(id) = caps.name("id") {
        return Some(SyncToken {
            organ_id: caps
                .name("organ")
                .and_then(|value| value.as_str().parse::<i64>().ok()),
            scope: SyncScope::from_segment(caps.name("scope")?.as_str())?,
            fields: SyncFields::from_segment(
                caps.name("fields")
                    .map(|value| value.as_str())
                    .unwrap_or(""),
            )?,
            record_id: id.as_str().parse::<i64>().ok()?,
        });
    }

    Some(SyncToken {
        organ_id: caps
            .name("human_organ")
            .and_then(|value| value.as_str().parse::<i64>().ok()),
        scope: SyncScope::from_human_segment(caps.name("human_scope")?.as_str())?,
        fields: SyncFields::from_human_segment(caps.name("human_fields")?.as_str())?,
        record_id: caps.name("human_id")?.as_str().parse::<i64>().ok()?,
    })
}

fn sync_token_regex() -> Regex {
    Regex::new(
        r"(?:sr|sync-record)(?:org(?P<organ>\d+))?(?P<scope>nt|t|n)(?P<fields>q?h?b?)(?P<id>\d+)|sync-record(?:-organ-(?P<human_organ>\d+))?-(?P<human_scope>node-and-tree|node|tree)-(?P<human_fields>quantity-head-body|quantity-head|quantity-body|head-body|quantity|head|body)-(?P<human_id>\d+)",
    )
    .unwrap()
}

fn sync_token_exact_regex() -> Regex {
    Regex::new(
        r"^(?:(?:sr|sync-record)(?:org(?P<organ>\d+))?(?P<scope>nt|t|n)(?P<fields>q?h?b?)(?P<id>\d+)|sync-record(?:-organ-(?P<human_organ>\d+))?-(?P<human_scope>node-and-tree|node|tree)-(?P<human_fields>quantity-head-body|quantity-head|quantity-body|head-body|quantity|head|body)-(?P<human_id>\d+))$",
    )
    .unwrap()
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

#[derive(Debug, Clone)]
struct SyncSourceTree {
    records: Vec<SqlRecordRow>,
    parent_links: Vec<SqlRecordLinkRow>,
    record_ids: BTreeSet<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct SqlOrganRow {
    id: i64,
    base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteRecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteRecordLinkRow {
    id: i64,
    record_id: i64,
    link_type: String,
    target_table: String,
    target_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct TrailSyncExtension {
    #[serde(alias = "trailRootRecordId")]
    trail_root_record_id: i64,
    #[serde(alias = "syncSourceRecordId")]
    sync_source_record_id: i64,
}

async fn load_sync_source_tree(
    services: &InjectedServices,
    token: &SyncToken,
    sync_snapshots: &mut HashMap<SyncToken, SyncSourceTree>,
) -> Result<SyncSourceTree, Error> {
    if let Some(existing) = sync_snapshots.get(token) {
        return Ok(existing.clone());
    }

    let tree = if let Some(organ_id) = token.organ_id {
        fetch_remote_sync_source_tree(services, organ_id, token.record_id).await?
    } else {
        load_local_sync_source_tree(services, token.record_id).await?
    };
    sync_snapshots.insert(*token, tree.clone());
    Ok(tree)
}

async fn load_local_sync_source_tree(
    services: &InjectedServices,
    root_record_id: i64,
) -> Result<SyncSourceTree, Error> {
    let db = &*services.db;
    let records = sqlx::query_as::<_, SqlRecordRow>("SELECT id, quantity, head, body FROM record")
        .fetch_all(db)
        .await
        .map_err(Error::other)?;
    let parent_links = sqlx::query_as::<_, SqlRecordLinkRow>(
        "SELECT id, record_id, link_type, target_table, target_id FROM record_link WHERE link_type = 'parent' AND target_table = 'record'",
    )
    .fetch_all(db)
    .await
    .map_err(Error::other)?;
    let record_ids = collect_tree_record_ids(root_record_id, &parent_links);
    Ok(SyncSourceTree {
        records: records
            .into_iter()
            .filter(|row| record_ids.contains(&row.id))
            .collect(),
        parent_links: parent_links
            .into_iter()
            .filter(|row| {
                record_ids.contains(&row.record_id) && record_ids.contains(&row.target_id)
            })
            .collect(),
        record_ids,
    })
}

async fn fetch_remote_sync_source_tree(
    services: &InjectedServices,
    organ_id: i64,
    root_record_id: i64,
) -> Result<SyncSourceTree, Error> {
    let organ =
        sqlx::query_as::<_, SqlOrganRow>("SELECT id, base_url FROM organ WHERE id = ? LIMIT 1")
            .bind(organ_id)
            .fetch_optional(&*services.db)
            .await
            .map_err(Error::other)?
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Remote organ not found"))?;
    let bearer_token = services
        .remote_organ_auth
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&organ.id)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "Remote organ login missing"))?;
    let base_url = organ.base_url.trim().trim_end_matches('/');
    let client = reqwest::Client::builder()
        .user_agent("lince-karma-sync/0.1")
        .build()
        .map_err(Error::other)?;

    let records = fetch_remote_json::<Vec<RemoteRecordRow>>(
        &client,
        &bearer_token,
        &format!("{base_url}/table/record"),
    )
    .await?;
    let parent_links = fetch_remote_json::<Vec<RemoteRecordLinkRow>>(
        &client,
        &bearer_token,
        &format!("{base_url}/table/record_link"),
    )
    .await?
    .into_iter()
    .filter(|row| row.link_type == "parent" && row.target_table == "record")
    .map(|row| SqlRecordLinkRow {
        id: row.id,
        record_id: row.record_id,
        link_type: row.link_type,
        target_table: row.target_table,
        target_id: row.target_id,
    })
    .collect::<Vec<_>>();
    let record_ids = collect_tree_record_ids(root_record_id, &parent_links);
    Ok(SyncSourceTree {
        records: records
            .into_iter()
            .filter(|row| record_ids.contains(&row.id))
            .map(|row| SqlRecordRow {
                id: row.id,
                quantity: row.quantity,
                head: row.head,
                body: row.body,
            })
            .collect(),
        parent_links: parent_links
            .into_iter()
            .filter(|row| {
                record_ids.contains(&row.record_id) && record_ids.contains(&row.target_id)
            })
            .collect(),
        record_ids,
    })
}

async fn fetch_remote_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    bearer_token: &str,
    url: &str,
) -> Result<T, Error> {
    let response = client
        .get(url)
        .bearer_auth(bearer_token)
        .send()
        .await
        .map_err(Error::other)?;
    if !response.status().is_success() {
        return Err(Error::other(format!(
            "Remote sync request failed with {}",
            response.status()
        )));
    }
    response.json::<T>().await.map_err(Error::other)
}

fn collect_tree_record_ids(
    root_record_id: i64,
    parent_links: &[SqlRecordLinkRow],
) -> BTreeSet<i64> {
    let mut record_ids = BTreeSet::from([root_record_id]);
    let mut children_by_parent = HashMap::<i64, BTreeSet<i64>>::new();
    for row in parent_links {
        children_by_parent
            .entry(row.target_id)
            .or_default()
            .insert(row.record_id);
    }
    let mut queue = VecDeque::from([root_record_id]);
    while let Some(parent_id) = queue.pop_front() {
        for child_id in children_by_parent
            .get(&parent_id)
            .cloned()
            .unwrap_or_default()
        {
            if record_ids.insert(child_id) {
                queue.push_back(child_id);
            }
        }
    }
    record_ids
}

async fn execute_sync_consequence(
    services: InjectedServices,
    token: &SyncToken,
    sync_snapshots: &mut HashMap<SyncToken, SyncSourceTree>,
) -> Result<(), Error> {
    let mut context = load_sync_context(&services, token.record_id, None).await?;
    let remote_source_tree = if token.organ_id.is_some() {
        Some(load_sync_source_tree(&services, token, sync_snapshots).await?)
    } else {
        sync_snapshots
            .iter()
            .find(|(snapshot_token, _)| {
                snapshot_token.organ_id.is_some()
                    && snapshot_token.record_id == context.source_root_record_id
            })
            .map(|(_, tree)| tree.clone())
    };
    if let Some(source_tree) = remote_source_tree {
        context = load_sync_context(&services, token.record_id, Some(source_tree)).await?;
    }
    let source_root_record_id = context.source_root_record_id;
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
            &mut context,
            token.record_id,
            source_root_record_id,
            token.fields,
        )
        .await?;
    }

    if token.scope.includes_tree() {
        sync_descendants(
            services,
            &mut context,
            token.record_id,
            source_root_record_id,
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
    children_by_parent: HashMap<i64, BTreeSet<i64>>,
    trail_root_record_id: i64,
    source_root_record_id: i64,
    copies_by_source_id: BTreeMap<i64, i64>,
    categories_by_record: BTreeMap<i64, Vec<String>>,
    assignees_by_record: BTreeMap<i64, Vec<i64>>,
}

async fn load_sync_context(
    services: &InjectedServices,
    copied_root_id: i64,
    remote_source_tree: Option<SyncSourceTree>,
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

    let records_by_id = records
        .into_iter()
        .map(|row| (row.id, row))
        .collect::<BTreeMap<_, _>>();
    let trail_extension = all_extensions
        .iter()
        .find(|row| row.record_id == copied_root_id && row.namespace == "trail.sync")
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing trail.sync extension"))?;
    let trail_sync =
        serde_json::from_str::<TrailSyncExtension>(&trail_extension.freestyle_data_structure)
            .map_err(Error::other)?;

    let mut parent_links = all_links
        .iter()
        .filter(|row| row.link_type == "parent" && row.target_table == "record")
        .cloned()
        .collect::<Vec<_>>();
    let mut records_by_id = records_by_id;
    if let Some(remote) = remote_source_tree {
        for row in remote.records {
            records_by_id.insert(row.id, row);
        }
        parent_links.retain(|row| !remote.record_ids.contains(&row.record_id));
        parent_links.extend(remote.parent_links);
    }
    let mut children_by_parent = HashMap::<i64, BTreeSet<i64>>::new();
    for row in &parent_links {
        children_by_parent
            .entry(row.target_id)
            .or_default()
            .insert(row.record_id);
    }
    let categories_by_record = build_categories_by_record(&all_extensions);
    let assignees_by_record = build_assignees_by_record(&all_links);
    let copies_by_source_id = all_extensions
        .iter()
        .filter(|row| row.namespace == "trail.sync")
        .filter_map(|row| {
            let parsed =
                serde_json::from_str::<TrailSyncExtension>(&row.freestyle_data_structure).ok()?;
            (parsed.trail_root_record_id == trail_sync.trail_root_record_id)
                .then_some((parsed.sync_source_record_id, row.record_id))
        })
        .collect::<BTreeMap<_, _>>();

    Ok(SyncContext {
        records_by_id,
        parent_links,
        all_links,
        all_extensions,
        children_by_parent,
        trail_root_record_id: trail_sync.trail_root_record_id,
        source_root_record_id: trail_sync.sync_source_record_id,
        copies_by_source_id,
        categories_by_record,
        assignees_by_record,
    })
}

async fn sync_existing_record_fields(
    services: InjectedServices,
    context: &mut SyncContext,
    copied_record_id: i64,
    source_record_id: i64,
    fields: SyncFields,
) -> Result<(), Error> {
    let source = context
        .records_by_id
        .get(&source_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing source record for sync"))?
        .clone();
    let copy = context
        .records_by_id
        .get(&copied_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing copied record for sync"))?
        .clone();

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
    .await?;
    if let Some(copy_row) = context.records_by_id.get_mut(&copied_record_id) {
        if fields.quantity {
            copy_row.quantity = source.quantity;
        }
        if fields.head {
            copy_row.head = source.head.clone();
        }
        if fields.body {
            copy_row.body = source.body.clone();
        }
    }
    Ok(())
}

async fn sync_descendants(
    services: InjectedServices,
    context: &mut SyncContext,
    copied_root_id: i64,
    source_root_id: i64,
    fields: SyncFields,
    root_categories: &[String],
    root_assignees: &[i64],
) -> Result<(), Error> {
    let mut copies_by_source = context.copies_by_source_id.clone();
    copies_by_source.insert(source_root_id, copied_root_id);

    let mut queue = VecDeque::from([(source_root_id, copied_root_id)]);
    let mut visited_sources = BTreeSet::new();
    while let Some((current_source_id, current_copy_id)) = queue.pop_front() {
        if !visited_sources.insert(current_source_id) {
            continue;
        }
        let expected_source_children = context
            .children_by_parent
            .get(&current_source_id)
            .cloned()
            .unwrap_or_default();

        let mut expected_copy_children = BTreeSet::new();

        for child_source_id in expected_source_children.iter().copied() {
            let child_copy_id =
                if let Some(existing_id) = copies_by_source.get(&child_source_id).copied() {
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
                        &mut context.all_extensions,
                        &mut context.categories_by_record,
                        existing_id,
                        root_categories,
                    )
                    .await?;
                    sync_assignees(
                        services.clone(),
                        &mut context.all_links,
                        &mut context.assignees_by_record,
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
    context: &mut SyncContext,
    source_record_id: i64,
    trail_root_record_id: i64,
    fields: SyncFields,
    root_categories: &[String],
    root_assignees: &[i64],
) -> Result<i64, Error> {
    let source = context
        .records_by_id
        .get(&source_record_id)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Missing source record for copied node"))?
        .clone();
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
    context.records_by_id.insert(
        copied_record_id,
        SqlRecordRow {
            id: copied_record_id,
            quantity,
            head: source.head.clone(),
            body: source.body.clone(),
        },
    );

    upsert_extension(
        services.clone(),
        &mut context.all_extensions,
        copied_record_id,
        "trail.sync",
        &json!({
            "trail_root_record_id": trail_root_record_id,
            "sync_source_record_id": source_record_id,
        })
        .to_string(),
    )
    .await?;
    context
        .copies_by_source_id
        .insert(source_record_id, copied_record_id);
    sync_categories(
        services.clone(),
        &mut context.all_extensions,
        &mut context.categories_by_record,
        copied_record_id,
        root_categories,
    )
    .await?;
    sync_assignees(
        services,
        &mut context.all_links,
        &mut context.assignees_by_record,
        copied_record_id,
        root_assignees,
    )
    .await?;
    Ok(copied_record_id)
}

async fn ensure_parent_link(
    services: InjectedServices,
    context: &mut SyncContext,
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
    let inserted = execute_statement(
        services,
        "INSERT INTO record_link(record_id, link_type, target_table, target_id, position, freestyle_data_structure) VALUES (?, 'parent', 'record', ?, NULL, NULL)",
        vec![SqlParameter::Integer(child_id), SqlParameter::Integer(parent_id)],
    )
    .await?;
    let row = SqlRecordLinkRow {
        id: inserted.last_insert_rowid.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "Insert into record_link did not return last_insert_rowid",
            )
        })?,
        record_id: child_id,
        link_type: "parent".into(),
        target_table: "record".into(),
        target_id: parent_id,
    };
    context.parent_links.push(row.clone());
    context.all_links.push(row);
    Ok(())
}

async fn repair_parent_links(
    services: InjectedServices,
    context: &mut SyncContext,
    current_copy_id: i64,
    expected_copy_children: &BTreeSet<i64>,
    trail_root_record_id: i64,
) -> Result<(), Error> {
    let candidate_links = context
        .all_links
        .iter()
        .filter(|row| {
            row.link_type == "parent"
                && row.target_table == "record"
                && row.target_id == current_copy_id
        })
        .cloned()
        .collect::<Vec<_>>();
    for link in candidate_links {
        let belongs_to_same_trail = context
            .all_extensions
            .iter()
            .find(|extension| {
                extension.record_id == link.record_id && extension.namespace == "trail.sync"
            })
            .and_then(|extension| {
                serde_json::from_str::<TrailSyncExtension>(&extension.freestyle_data_structure).ok()
            })
            .is_some_and(|meta| meta.trail_root_record_id == trail_root_record_id);
        if belongs_to_same_trail && !expected_copy_children.contains(&link.record_id) {
            execute_statement(
                services.clone(),
                "DELETE FROM record_link WHERE id = ?",
                vec![SqlParameter::Integer(link.id)],
            )
            .await?;
            context.all_links.retain(|row| row.id != link.id);
            context.parent_links.retain(|row| row.id != link.id);
        }
    }
    Ok(())
}

async fn upsert_extension(
    services: InjectedServices,
    extensions: &mut Vec<SqlRecordExtensionRow>,
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
        .map(|_| {
            if let Some(row) = extensions
                .iter_mut()
                .find(|row| row.record_id == record_id && row.namespace == namespace)
            {
                row.freestyle_data_structure = freestyle_data_structure.to_string();
            }
        })
    } else {
        let inserted = execute_statement(
            services,
            "INSERT INTO record_extension(record_id, namespace, version, freestyle_data_structure) VALUES (?, ?, 1, ?)",
            vec![
                SqlParameter::Integer(record_id),
                SqlParameter::Text(namespace.to_string()),
                SqlParameter::Text(freestyle_data_structure.to_string()),
            ],
        )
        .await?;
        extensions.push(SqlRecordExtensionRow {
            id: inserted.last_insert_rowid.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Insert into record_extension did not return last_insert_rowid",
                )
            })?,
            record_id,
            namespace: namespace.to_string(),
            freestyle_data_structure: freestyle_data_structure.to_string(),
        });
        Ok(())
    }
}

async fn sync_categories(
    services: InjectedServices,
    extensions: &mut Vec<SqlRecordExtensionRow>,
    categories_by_record: &mut BTreeMap<i64, Vec<String>>,
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
    .await?;
    categories_by_record.insert(record_id, normalized);
    Ok(())
}

async fn sync_assignees(
    services: InjectedServices,
    links: &mut Vec<SqlRecordLinkRow>,
    assignees_by_record: &mut BTreeMap<i64, Vec<i64>>,
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
        .cloned()
        .collect::<Vec<_>>();
    let existing_ids = existing
        .iter()
        .map(|row| row.target_id)
        .collect::<BTreeSet<_>>();
    for row in &existing {
        if !desired.contains(&row.target_id) {
            execute_statement(
                services.clone(),
                "DELETE FROM record_link WHERE id = ?",
                vec![SqlParameter::Integer(row.id)],
            )
            .await?;
            links.retain(|candidate| candidate.id != row.id);
        }
    }
    for assignee_id in desired {
        if existing_ids.contains(&assignee_id) {
            continue;
        }
        let inserted = execute_statement(
            services.clone(),
            "INSERT INTO record_link(record_id, link_type, target_table, target_id, position, freestyle_data_structure) VALUES (?, 'assigned_to', 'app_user', ?, NULL, NULL)",
            vec![
                SqlParameter::Integer(record_id),
                SqlParameter::Integer(assignee_id),
            ],
        )
        .await?;
        links.push(SqlRecordLinkRow {
            id: inserted.last_insert_rowid.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Insert into record_link did not return last_insert_rowid",
                )
            })?,
            record_id,
            link_type: "assigned_to".into(),
            target_table: "app_user".into(),
            target_id: assignee_id,
        });
    }
    assignees_by_record.insert(
        record_id,
        links
            .iter()
            .filter(|row| {
                row.record_id == record_id
                    && row.link_type == "assigned_to"
                    && row.target_table == "app_user"
            })
            .map(|row| row.target_id)
            .collect(),
    );
    Ok(())
}

fn build_categories_by_record(extensions: &[SqlRecordExtensionRow]) -> BTreeMap<i64, Vec<String>> {
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

fn build_assignees_by_record(links: &[SqlRecordLinkRow]) -> BTreeMap<i64, Vec<i64>> {
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
