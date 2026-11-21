// non-lean-yet
// Scope: struct Shared, struct Element, struct Index, fn tail.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 139, 156.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   137, 212.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (211).

use super::{Rule, SharedField, Suggestion};
use nucleus::karma::rule_field::RuleFieldKind;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, Weak},
};

struct Shared {
    rule: usize,
    field: usize,
    folded: String,
}

struct Element {
    value: String,
    folded: String,
}

struct Index {
    kind: RuleFieldKind,
    fields: Vec<Shared>,
    elements: Vec<Element>,
}

impl Index {
    fn new(kind: RuleFieldKind, rules: &[Rule], records: &[Value], frequencies: &[Value]) -> Self {
        let _prepare = nucleus::performance::Span::new("karma-suggestion-source-preparation");
        let fields: Vec<_> = rules
            .iter()
            .enumerate()
            .flat_map(|(rule, value)| {
                value
                    .fields
                    .iter()
                    .enumerate()
                    .filter_map(move |(field, value)| {
                        (value.kind == kind).then(|| Shared {
                            rule,
                            field,
                            folded: value.source.to_lowercase(),
                        })
                    })
            })
            .collect();
        let mut elements: Vec<String> = match kind {
        RuleFieldKind::Condition => [
            "+",
            "-",
            "*",
            "/",
            "(",
            ")",
            "1",
            "quantity(",
            "value(",
            "sum(",
            "sum_pos(",
            "sum_neg(",
            "signal(",
            "query_command(",
            "freq(",
            "extension(",
        ]
        .map(str::to_owned)
        .into(),
        RuleFieldKind::Threshold => ["!=0", "always", ">0", ">=1", "<0", "<=0", "==0"]
            .map(str::to_owned)
            .into(),
        RuleFieldKind::Consequence => [
            ": activate-fiote",
            ": command(\"\")",
            ": run(@command)",
            ": show({\"kind\":\"record\",\"mode\":\"full\"})",
            ": show({\"kind\":\"record\",\"mode\":\"description\"})",
            ": show({\"kind\":\"record\",\"mode\":\"call\"})",
            ": show({\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"thread\",\"person\":\"me\",\"media\":\"audio\"}})",
            ": show({\"kind\":\"text\",\"text\":\"Reminder\"})",
            ": show({\"kind\":\"karma\",\"search\":\"\"})",
            ": show({\"kind\":\"frequency\",\"search\":\"\"})",
        ].map(str::to_owned).into(),
    };
        if kind != RuleFieldKind::Threshold {
            elements.extend(
                records
                    .iter()
                    .filter_map(|record| record["slug"].as_str().map(|slug| format!("@{slug}"))),
            );
        }
        if kind == RuleFieldKind::Condition {
            elements.extend(records.iter().flat_map(|record| {
                let slug = record["slug"]
                    .as_str()
                    .or_else(|| record["uid"].as_str())
                    .unwrap_or_default();
                record["numeric_extensions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(move |property| {
                        let namespace =
                            serde_json::to_string(property["namespace"].as_str()?).ok()?;
                        let name = serde_json::to_string(property["property"].as_str()?).ok()?;
                        Some(format!("extension(@{slug}, {namespace}, {name})"))
                    })
            }));
            elements.extend(frequencies.iter().filter_map(|frequency| {
                frequency["slug"]
                    .as_str()
                    .map(|slug| format!("freq(@{slug})"))
            }));
        }
        elements.sort();
        elements.dedup();
        let elements: Vec<_> = elements
            .into_iter()
            .map(|value| {
                let folded = value.to_lowercase();
                Element { value, folded }
            })
            .collect();
        nucleus::performance::add("karma-suggestion-fields-prepared", fields.len() as u64);
        nucleus::performance::add("karma-suggestion-elements-prepared", elements.len() as u64);
        Self {
            kind,
            fields,
            elements,
        }
    }

    fn select(&self, text: &str, rules: &[Rule]) -> Vec<Suggestion> {
        let _query = nucleus::performance::Span::new("karma-suggestion-query");
        let query = text.trim().to_lowercase();
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        for shared in &self.fields {
            let field: &SharedField = &rules[shared.rule].fields[shared.field];
            if shared.folded.contains(&query) && seen.insert(field.uid.as_str()) {
                found.push(Suggestion::Shared(field.clone()));
                if found.len() == 12 {
                    break;
                }
            }
        }
        let tail = tail(text);
        let needle = if self.kind == RuleFieldKind::Threshold {
            &query
        } else {
            &tail
        };
        found.extend(
            self.elements
                .iter()
                .filter(|element| element.folded.contains(needle))
                .take(16)
                .map(|element| Suggestion::Element(element.value.clone())),
        );
        nucleus::performance::add("karma-suggestions-selected", found.len() as u64);
        found
    }
}

pub fn tail(text: &str) -> String {
    text.rsplit(|character: char| {
        character.is_whitespace() || matches!(character, '(' | ')' | '*' | '/' | '+' | '=' | ',')
    })
    .next()
    .unwrap_or("")
    .to_lowercase()
}

pub fn suggestions(
    kind: RuleFieldKind,
    text: &str,
    rules: &[Rule],
    records: &[Value],
    frequencies: &[Value],
) -> Vec<Suggestion> {
    Index::new(kind, rules, records, frequencies).select(text, rules)
}

#[derive(Default)]
pub struct Suggestions {
    rules: Weak<Vec<Rule>>,
    records: Weak<Vec<Value>>,
    frequencies: Weak<Vec<Value>>,
    kinds: [Option<Index>; 3],
}

impl Suggestions {
    pub fn select(
        &mut self,
        kind: RuleFieldKind,
        text: &str,
        rules: &Arc<Vec<Rule>>,
        records: &Arc<Vec<Value>>,
        frequencies: &Arc<Vec<Value>>,
    ) -> Vec<Suggestion> {
        if self.rules.as_ptr() != Arc::as_ptr(rules)
            || self.records.as_ptr() != Arc::as_ptr(records)
            || self.frequencies.as_ptr() != Arc::as_ptr(frequencies)
        {
            self.rules = Arc::downgrade(rules);
            self.records = Arc::downgrade(records);
            self.frequencies = Arc::downgrade(frequencies);
            self.kinds = Default::default();
        }
        let slot = RuleFieldKind::ALL
            .iter()
            .position(|value| *value == kind)
            .unwrap();
        self.kinds[slot]
            .get_or_insert_with(|| Index::new(kind, rules, records, frequencies))
            .select(text, rules)
    }
}
