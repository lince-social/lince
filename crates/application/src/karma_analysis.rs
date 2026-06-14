use crate::engine::return_engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaOrchestraRuleInput {
    pub karma_id: u32,
    pub karma_name: String,
    pub karma_quantity: i32,
    pub karma_parallel: bool,
    pub karma_timeout_seconds: f64,
    pub active: bool,
    pub condition_id: u32,
    pub condition_name: String,
    pub condition_quantity: i32,
    pub condition_code: String,
    pub operator: String,
    pub consequence_id: u32,
    pub consequence_name: String,
    pub consequence_quantity: i32,
    pub consequence_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordToken {
    pub id: u32,
    pub quantity: f64,
    pub head: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferToken {
    pub id: u32,
    pub quantity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedToken {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaTokenCatalog {
    pub records: BTreeMap<u32, RecordToken>,
    pub transfers: BTreeMap<u32, TransferToken>,
    pub commands: BTreeMap<u32, NamedToken>,
    pub queries: BTreeMap<u32, NamedToken>,
    pub frequencies: BTreeMap<u32, NamedToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaExpressionDisplay {
    pub code: String,
    pub human: String,
    pub value: KarmaDisplayValue,
    pub references: Vec<KarmaTokenReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaDisplayValue {
    pub text: String,
    pub numeric: Option<f64>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaTokenReference {
    pub token: String,
    pub kind: String,
    pub id: u32,
    pub human: String,
    pub numeric: Option<f64>,
    pub display_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaOrchestraSnapshot {
    pub view_id: u32,
    pub view_name: String,
    pub karma_rows: Vec<KarmaOrchestraRuleInput>,
    pub nodes: Vec<KarmaOrchestraNode>,
    pub links: Vec<KarmaOrchestraLink>,
    pub loops: Vec<KarmaOrchestraLoop>,
    pub check: KarmaCheckReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaOrchestraNode {
    pub id: String,
    pub entity_id: u32,
    pub kind: String,
    pub shape: String,
    pub label: KarmaExpressionDisplay,
    pub rule_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaOrchestraLink {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub active: bool,
    pub rule_ids: Vec<u32>,
    pub potentially_unreachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaOrchestraLoop {
    pub id: String,
    pub node_ids: Vec<String>,
    pub link_ids: Vec<String>,
    pub potentially_unreachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaCheckReport {
    pub has_loops: bool,
    pub loop_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct EvaluatedRule {
    rule: KarmaOrchestraRuleInput,
    condition: KarmaExpressionDisplay,
    consequence: KarmaExpressionDisplay,
    enabled: bool,
    triggers: bool,
}

pub fn build_karma_orchestra_snapshot(
    view_id: u32,
    view_name: impl Into<String>,
    rules: Vec<KarmaOrchestraRuleInput>,
    catalog: KarmaTokenCatalog,
) -> KarmaOrchestraSnapshot {
    let mut evaluated = rules
        .iter()
        .cloned()
        .map(|rule| evaluate_rule(rule, &catalog))
        .collect::<Vec<_>>();
    evaluated.sort_by_key(|entry| entry.rule.karma_id);

    let mut nodes = build_nodes(&evaluated);
    let mut links = build_direct_links(&evaluated);
    links.extend(build_fulfillment_links(&evaluated, &catalog));
    let loops = detect_loops(&links);
    let bridge_ids = links
        .iter()
        .filter(|link| link.kind == "fulfillment")
        .map(|link| link.source.clone())
        .collect::<HashSet<_>>();
    for node in &mut nodes {
        if bridge_ids.contains(&node.id) {
            node.kind = "bridge".into();
            node.shape = "bridge".into();
        }
    }

    let warnings = loops
        .iter()
        .map(|item| format!("Potential Karma loop: {}", item.node_ids.join(" -> ")))
        .collect::<Vec<_>>();

    KarmaOrchestraSnapshot {
        view_id,
        view_name: view_name.into(),
        karma_rows: evaluated
            .into_iter()
            .map(|entry| {
                let mut rule = entry.rule;
                rule.active = entry.triggers;
                rule
            })
            .collect(),
        nodes,
        links,
        check: KarmaCheckReport {
            has_loops: !loops.is_empty(),
            loop_count: loops.len(),
            warnings,
        },
        loops,
    }
}

pub fn record_ids_in_expression(expression: &str) -> BTreeSet<u32> {
    token_regex("rq")
        .captures_iter(expression)
        .filter_map(|caps| caps.get(1)?.as_str().parse::<u32>().ok())
        .collect()
}

pub fn first_record_quantity_token(expression: &str) -> Option<u32> {
    token_regex("rq")
        .captures(expression)
        .and_then(|caps| caps.get(1)?.as_str().parse::<u32>().ok())
}

pub fn transfer_ids_in_expression(expression: &str) -> BTreeSet<u32> {
    transfer_quantity_regex()
        .captures_iter(expression)
        .filter_map(|caps| transfer_quantity_id(&caps))
        .collect()
}

pub fn first_transfer_quantity_token(expression: &str) -> Option<u32> {
    transfer_quantity_regex()
        .captures(expression)
        .and_then(|caps| transfer_quantity_id(&caps))
}

fn evaluate_rule(rule: KarmaOrchestraRuleInput, catalog: &KarmaTokenCatalog) -> EvaluatedRule {
    let condition = expression_display(&rule.condition_code, catalog, true);
    let condition_numeric = condition.value.numeric.unwrap_or(0.0);
    let enabled =
        rule.karma_quantity > 0 && rule.condition_quantity > 0 && rule.consequence_quantity > 0;
    let triggers = enabled
        && match rule.operator.as_str() {
            "=" => condition.value.complete && condition_numeric != 0.0,
            "=*" => true,
            _ => false,
        };
    let consequence_numeric = !record_ids_in_expression(&rule.consequence_code).is_empty()
        || !transfer_ids_in_expression(&rule.consequence_code).is_empty();
    let mut consequence = expression_display(&rule.consequence_code, catalog, consequence_numeric);
    if let Some(record_id) = first_record_quantity_token(&rule.consequence_code) {
        if consequence.human == rule.consequence_code {
            consequence.human = catalog
                .records
                .get(&record_id)
                .map(|record| record.head.clone())
                .unwrap_or_else(|| format!("Record #{record_id}"));
        }
    } else if let Some(transfer_id) = first_transfer_quantity_token(&rule.consequence_code)
        && consequence.human == rule.consequence_code
    {
        consequence.human = format!("Transfer #{transfer_id}");
    }

    EvaluatedRule {
        rule,
        condition,
        consequence,
        enabled,
        triggers,
    }
}

pub fn expression_display(
    code: &str,
    catalog: &KarmaTokenCatalog,
    numeric_quantities: bool,
) -> KarmaExpressionDisplay {
    let mut human = code.to_string();
    let mut symbolic = code.to_string();
    let mut references = Vec::new();

    for caps in token_regex("rq").captures_iter(code) {
        let token = caps[0].to_string();
        let id = caps[1].parse::<u32>().unwrap_or_default();
        let record = catalog.records.get(&id);
        let quantity = record.map(|record| record.quantity).unwrap_or(0.0);
        let name = record
            .map(|record| record.head.clone())
            .unwrap_or_else(|| format!("Record #{id}"));
        human = human.replace(&token, &name);
        if numeric_quantities {
            symbolic = symbolic.replace(&token, &quantity.to_string());
        }
        references.push(KarmaTokenReference {
            token,
            kind: "record_quantity".into(),
            id,
            human: name.clone(),
            numeric: Some(quantity),
            display_only: !numeric_quantities,
        });
    }

    for caps in transfer_quantity_regex().captures_iter(code) {
        let token = caps[0].to_string();
        let id = transfer_quantity_id(&caps).unwrap_or_default();
        let quantity = catalog
            .transfers
            .get(&id)
            .map(|transfer| transfer.quantity)
            .unwrap_or(0.0);
        let name = format!("Transfer #{id}");
        human = human.replace(&token, &name);
        if numeric_quantities {
            symbolic = symbolic.replace(&token, &quantity.to_string());
        }
        references.push(KarmaTokenReference {
            token,
            kind: "transfer_quantity".into(),
            id,
            human: name,
            numeric: Some(quantity),
            display_only: !numeric_quantities,
        });
    }

    replace_named_tokens(
        code,
        &mut human,
        &mut references,
        &token_regex("c"),
        "command",
        "Nameless Command",
        &catalog.commands,
    );
    replace_named_tokens(
        code,
        &mut human,
        &mut references,
        &token_regex("sql"),
        "query",
        "Nameless Query",
        &catalog.queries,
    );
    replace_named_tokens(
        code,
        &mut human,
        &mut references,
        &token_regex("f"),
        "frequency",
        "Nameless Frequency",
        &catalog.frequencies,
    );
    replace_sync_tokens(code, &mut human, &mut references, catalog);

    KarmaExpressionDisplay {
        code: code.into(),
        human,
        value: evaluate_symbolic(&symbolic),
        references,
    }
}

fn transfer_quantity_regex() -> Regex {
    Regex::new(r"(?:tq(\d+)|transfer-quantity-(\d+))").unwrap()
}

fn transfer_quantity_id(caps: &regex::Captures<'_>) -> Option<u32> {
    caps.get(1)
        .or_else(|| caps.get(2))
        .and_then(|value| value.as_str().parse::<u32>().ok())
}

fn replace_named_tokens(
    code: &str,
    human: &mut String,
    references: &mut Vec<KarmaTokenReference>,
    regex: &Regex,
    kind: &str,
    fallback: &str,
    catalog: &BTreeMap<u32, NamedToken>,
) {
    for caps in regex.captures_iter(code) {
        let token = caps[0].to_string();
        let id = caps[1].parse::<u32>().unwrap_or_default();
        let name = catalog
            .get(&id)
            .map(|entry| entry.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string());
        *human = human.replace(&token, &name);
        references.push(KarmaTokenReference {
            token,
            kind: kind.into(),
            id,
            human: name,
            numeric: None,
            display_only: true,
        });
    }
}

fn replace_sync_tokens(
    code: &str,
    human: &mut String,
    references: &mut Vec<KarmaTokenReference>,
    catalog: &KarmaTokenCatalog,
) {
    let regex = Regex::new(
        r"(?:sr|sync-record)(?P<scope>nt|t|n)(?P<fields>q?h?b?)(?P<id>\d+)|sync-record-(?P<human_scope>node-and-tree|node|tree)-(?P<human_fields>quantity-head-body|quantity-head|quantity-body|head-body|quantity|head|body)-(?P<human_id>\d+)",
    )
    .unwrap();
    for caps in regex.captures_iter(code) {
        let token = caps[0].to_string();
        let id = caps
            .name("id")
            .or_else(|| caps.name("human_id"))
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or_default();
        let scope = caps
            .name("scope")
            .or_else(|| caps.name("human_scope"))
            .map(|value| value.as_str())
            .unwrap_or("t");
        let fields = caps
            .name("fields")
            .or_else(|| caps.name("human_fields"))
            .map(|value| value.as_str())
            .unwrap_or("");
        let root = catalog
            .records
            .get(&id)
            .map(|record| record.head.clone())
            .filter(|head| !head.trim().is_empty())
            .unwrap_or_else(|| format!("Record #{id}"));
        let label = format!(
            "Sync {} {} root {root} (#{id})",
            sync_scope_label(scope),
            sync_fields_label(fields)
        );
        *human = human.replace(&token, &label);
        references.push(KarmaTokenReference {
            token,
            kind: "sync".into(),
            id,
            human: label,
            numeric: None,
            display_only: true,
        });
    }
}

fn sync_fields_label(fields: &str) -> String {
    if fields.is_empty() {
        return "quantity/head/body".into();
    }
    if fields.contains('-') {
        return fields.replace('-', "/");
    }
    let mut labels = Vec::new();
    if fields.contains('q') {
        labels.push("quantity");
    }
    if fields.contains('h') {
        labels.push("head");
    }
    if fields.contains('b') {
        labels.push("body");
    }
    labels.join("/")
}

fn sync_scope_label(scope: &str) -> &str {
    match scope {
        "n" | "node" => "node",
        "t" | "tree" => "tree",
        "nt" | "node-and-tree" => "node and tree",
        _ => scope,
    }
}

fn evaluate_symbolic(symbolic: &str) -> KarmaDisplayValue {
    if contains_unresolved_token(symbolic) {
        let text = reduce_numeric_islands(symbolic);
        return KarmaDisplayValue {
            text,
            numeric: None,
            complete: false,
        };
    }
    let expr = format!("({symbolic}) * 1.0");
    let numeric = return_engine().eval::<f64>(&expr).ok();
    KarmaDisplayValue {
        text: numeric
            .map(format_number)
            .unwrap_or_else(|| symbolic.to_string()),
        numeric,
        complete: numeric.is_some(),
    }
}

fn contains_unresolved_token(value: &str) -> bool {
    value.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn reduce_numeric_islands(input: &str) -> String {
    let regex = Regex::new(r"-?\d+(?:\.\d+)?(?:\s*[+\-*/]\s*-?\d+(?:\.\d+)?)+").unwrap();
    let mut output = input.to_string();
    for _ in 0..16 {
        let Some(matched) = regex.find(&output).map(|m| m.as_str().to_string()) else {
            break;
        };
        let expr = format!("({matched}) * 1.0");
        let Ok(value) = return_engine().eval::<f64>(&expr) else {
            break;
        };
        output = output.replacen(&matched, &format_number(value), 1);
    }
    output
}

fn format_number(value: f64) -> String {
    if (value.round() - value).abs() < 0.000001 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn build_nodes(evaluated: &[EvaluatedRule]) -> Vec<KarmaOrchestraNode> {
    let mut by_id = BTreeMap::<String, KarmaOrchestraNode>::new();
    for entry in evaluated {
        let condition_id = condition_node_id(entry.rule.condition_id);
        by_id
            .entry(condition_id.clone())
            .and_modify(|node| node.rule_ids.push(entry.rule.karma_id))
            .or_insert_with(|| KarmaOrchestraNode {
                id: condition_id,
                entity_id: entry.rule.condition_id,
                kind: "condition".into(),
                shape: "triangle".into(),
                label: entry.condition.clone(),
                rule_ids: vec![entry.rule.karma_id],
            });

        let consequence_id = consequence_node_id(entry.rule.consequence_id);
        by_id
            .entry(consequence_id.clone())
            .and_modify(|node| node.rule_ids.push(entry.rule.karma_id))
            .or_insert_with(|| KarmaOrchestraNode {
                id: consequence_id,
                entity_id: entry.rule.consequence_id,
                kind: "consequence".into(),
                shape: "circle".into(),
                label: entry.consequence.clone(),
                rule_ids: vec![entry.rule.karma_id],
            });
    }
    by_id.into_values().collect()
}

fn build_direct_links(evaluated: &[EvaluatedRule]) -> Vec<KarmaOrchestraLink> {
    evaluated
        .iter()
        .filter(|entry| entry.enabled && entry.triggers)
        .map(|entry| {
            let source = condition_node_id(entry.rule.condition_id);
            let target = consequence_node_id(entry.rule.consequence_id);
            KarmaOrchestraLink {
                id: format!(
                    "direct:{}:{}",
                    entry.rule.karma_id, entry.rule.consequence_id
                ),
                source,
                target,
                kind: "direct".into(),
                active: true,
                rule_ids: vec![entry.rule.karma_id],
                potentially_unreachable: false,
            }
        })
        .collect()
}

fn build_fulfillment_links(
    evaluated: &[EvaluatedRule],
    catalog: &KarmaTokenCatalog,
) -> Vec<KarmaOrchestraLink> {
    let mut links = BTreeMap::<String, KarmaOrchestraLink>::new();

    for source in evaluated
        .iter()
        .filter(|entry| entry.enabled && entry.triggers)
    {
        let changed_quantity =
            if let Some(record_id) = first_record_quantity_token(&source.rule.consequence_code) {
                QuantityTarget::Record(record_id)
            } else if let Some(transfer_id) =
                first_transfer_quantity_token(&source.rule.consequence_code)
            {
                QuantityTarget::Transfer(transfer_id)
            } else {
                continue;
            };
        let Some(new_quantity) = source.condition.value.numeric else {
            continue;
        };
        let mut next_catalog = catalog.clone();
        match changed_quantity {
            QuantityTarget::Record(record_id) => {
                next_catalog
                    .records
                    .entry(record_id)
                    .and_modify(|record| record.quantity = new_quantity)
                    .or_insert(RecordToken {
                        id: record_id,
                        quantity: new_quantity,
                        head: format!("Record #{record_id}"),
                    });
            }
            QuantityTarget::Transfer(transfer_id) => {
                next_catalog
                    .transfers
                    .entry(transfer_id)
                    .and_modify(|transfer| transfer.quantity = new_quantity)
                    .or_insert(TransferToken {
                        id: transfer_id,
                        quantity: new_quantity,
                    });
            }
        }

        for target in evaluated {
            if !target.enabled
                || !quantity_target_in_expression(&changed_quantity, &target.rule.condition_code)
            {
                continue;
            }
            let next_condition =
                expression_display(&target.rule.condition_code, &next_catalog, true);
            let next_numeric = next_condition.value.numeric.unwrap_or(0.0);
            let next_triggers = match target.rule.operator.as_str() {
                "=" => next_condition.value.complete && next_numeric != 0.0,
                "=*" => true,
                _ => false,
            };
            if !next_triggers {
                continue;
            }
            let source_id = consequence_node_id(source.rule.consequence_id);
            let target_id = condition_node_id(target.rule.condition_id);
            let id = format!(
                "fulfillment:{}:{}",
                source.rule.consequence_id, target.rule.condition_id
            );
            links
                .entry(id.clone())
                .or_insert_with(|| KarmaOrchestraLink {
                    id,
                    source: source_id,
                    target: target_id,
                    kind: "fulfillment".into(),
                    active: true,
                    rule_ids: vec![source.rule.karma_id, target.rule.karma_id],
                    potentially_unreachable: false,
                });
        }
    }

    links.into_values().collect()
}

#[derive(Debug, Clone, Copy)]
enum QuantityTarget {
    Record(u32),
    Transfer(u32),
}

fn quantity_target_in_expression(target: &QuantityTarget, expression: &str) -> bool {
    match target {
        QuantityTarget::Record(id) => record_ids_in_expression(expression).contains(id),
        QuantityTarget::Transfer(id) => transfer_ids_in_expression(expression).contains(id),
    }
}

fn detect_loops(links: &[KarmaOrchestraLink]) -> Vec<KarmaOrchestraLoop> {
    let mut adjacency = HashMap::<String, Vec<(String, String)>>::new();
    for link in links.iter().filter(|link| link.active) {
        adjacency
            .entry(link.source.clone())
            .or_default()
            .push((link.target.clone(), link.id.clone()));
    }

    let mut loops = Vec::new();
    let mut seen = HashSet::<String>::new();
    for start in adjacency.keys() {
        let mut path_nodes = vec![start.clone()];
        let mut path_links = Vec::new();
        dfs_cycles(
            start,
            start,
            &adjacency,
            &mut path_nodes,
            &mut path_links,
            &mut seen,
            &mut loops,
        );
    }
    loops
}

fn dfs_cycles(
    start: &str,
    current: &str,
    adjacency: &HashMap<String, Vec<(String, String)>>,
    path_nodes: &mut Vec<String>,
    path_links: &mut Vec<String>,
    seen: &mut HashSet<String>,
    loops: &mut Vec<KarmaOrchestraLoop>,
) {
    if path_nodes.len() > 16 || loops.len() > 64 {
        return;
    }
    let Some(nexts) = adjacency.get(current) else {
        return;
    };
    for (next, link_id) in nexts {
        if next == start {
            let mut ids = path_nodes.clone();
            ids.push(start.to_string());
            let mut loop_links = path_links.clone();
            loop_links.push(link_id.clone());
            let mut key_parts = ids.clone();
            key_parts.sort();
            key_parts.dedup();
            let key = key_parts.join("|");
            if seen.insert(key) {
                loops.push(KarmaOrchestraLoop {
                    id: format!("loop:{}", loops.len() + 1),
                    node_ids: ids,
                    link_ids: loop_links,
                    potentially_unreachable: false,
                });
            }
            continue;
        }
        if path_nodes.contains(next) {
            continue;
        }
        path_nodes.push(next.clone());
        path_links.push(link_id.clone());
        dfs_cycles(start, next, adjacency, path_nodes, path_links, seen, loops);
        path_links.pop();
        path_nodes.pop();
    }
}

fn condition_node_id(id: u32) -> String {
    format!("condition:{id}")
}

fn consequence_node_id(id: u32) -> String {
    format!("consequence:{id}")
}

fn token_regex(prefix: &str) -> Regex {
    Regex::new(&format!(r"{prefix}(\d+)")).expect("valid karma token regex")
}
