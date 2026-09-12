pub mod grammar;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ast::{Declaration, Document};
use grammar::grammar as ast;
use sha2::{Digest, Sha256};

pub const EXTENSION: &str = "lingua";
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedDocument {
    pub records: Vec<ProjectedRecord>,
    pub frequencies: Vec<ProjectedFrequency>,
    pub rules: Vec<ProjectedRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRecord {
    pub uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: Option<(String, Option<String>)>,
    pub assertions: Vec<ProjectedAssertion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedAssertion {
    pub predicate: String,
    pub identity: bool,
    pub object_slug: Option<String>,
    pub object_uid: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedFrequency {
    pub uid: String,
    pub slug: String,
    pub head: String,
    pub quantity: i64,
    pub every: (u32, String),
    pub timezone: String,
    pub next_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRule {
    pub uid: String,
    pub slug: String,
    pub quantity: i64,
    pub frequency_slug: String,
    pub frequency_uid: Option<String>,
    pub record_slug: String,
    pub record_uid: Option<String>,
    pub condition: Option<String>,
    pub gate: Option<String>,
    pub carry: Option<String>,
    pub consequences_json: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: Option<PathBuf>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.path {
            write!(formatter, "{}: {}", path.display(), self.message)
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl std::error::Error for Diagnostic {}

pub fn parse(source: &str) -> Result<Document, Diagnostic> {
    let document = ast::parse(source).map_err(|error| Diagnostic {
        path: None,
        message: format!("{error:?}"),
    })?;
    validate_record_boundaries(&document)?;
    Ok(document)
}

fn validate_record_boundaries(document: &Document) -> Result<(), Diagnostic> {
    for declaration in &document.declarations {
        let Declaration::Record(record) = declaration else {
            continue;
        };
        let title = record.title.value();
        match (record.opening.uid(), record.closing.uid()) {
            (None, None) => {}
            (Some(opening), Some(closing)) if opening == closing => {}
            (Some(opening), Some(closing)) => {
                return Err(diagnostic(format!(
                    "Record `{}` opens with uid `{opening}` but closes with `{closing}`",
                    title
                )));
            }
            (Some(uid), None) => {
                return Err(diagnostic(format!(
                    "Record `{}` opens with uid `{uid}` but its closing boundary has none",
                    title
                )));
            }
            (None, Some(uid)) => {
                return Err(diagnostic(format!(
                    "Record `{}` closes with uid `{uid}` but its opening boundary has none",
                    title
                )));
            }
        }
    }
    Ok(())
}

pub fn parse_path(path: &Path) -> Result<Document, Diagnostic> {
    let source = fs::read_to_string(path).map_err(|error| Diagnostic {
        path: Some(path.to_path_buf()),
        message: error.to_string(),
    })?;
    parse(&source).map_err(|mut error| {
        error.path = Some(path.to_path_buf());
        error
    })
}

pub fn format(document: &Document) -> String {
    let mut output = String::new();
    for (index, declaration) in document.declarations.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        match declaration {
            Declaration::Record(record) => format_record(&mut output, record),
            Declaration::Frequency(frequency) => format_frequency(&mut output, frequency),
            Declaration::Karma(karma) => format_karma(&mut output, karma),
        }
    }
    output
}

pub fn canonicalize(source: &str) -> Result<String, Diagnostic> {
    parse(source).map(|document| format(&document))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintedUid {
    pub kind: &'static str,
    pub name: String,
    pub uid: String,
}

pub fn ensure_uids(source: &str) -> Result<(String, Vec<MintedUid>), Diagnostic> {
    let mut document = parse(source)?;
    let mut missing = Vec::new();
    let mut ordinal = 0usize;
    for declaration in &mut document.declarations {
        match declaration {
            Declaration::Record(record) if record.opening.uid().is_none() => {
                let name = record
                    .header
                    .subject
                    .slug()
                    .map(str::to_string)
                    .unwrap_or_else(|| record.title.value());
                let identity = minted(source, "Record", "r", &name, ordinal);
                ordinal += 1;
                record.opening = ast::RecordOpening::identified(identity.uid.clone());
                record.closing = ast::RecordClosing::identified(identity.uid.clone());
                missing.push(identity);
            }
            Declaration::Frequency(frequency) if frequency.uid.is_none() => {
                let identity = minted(source, "Frequency", "freq", &frequency.opening.0, ordinal);
                ordinal += 1;
                frequency.uid = Some(ast::Uid::new(identity.uid.clone()));
                missing.push(identity);
            }
            Declaration::Karma(karma) => {
                for rule in &mut karma.rules.rules {
                    if rule.uid.is_none() {
                        let identity = minted(source, "Rule", "rule", &rule.name.0, ordinal);
                        ordinal += 1;
                        rule.uid = Some(ast::Uid::new(identity.uid.clone()));
                        missing.push(identity);
                    }
                }
            }
            _ => {}
        }
    }
    Ok((format(&document), missing))
}

pub fn bind_reference_uids(
    source: &str,
    identities: &BTreeMap<String, String>,
) -> Result<String, Diagnostic> {
    use ast::{RecordField, RuleField};
    let document = parse(source)?;
    for declaration in &document.declarations {
        match declaration {
            Declaration::Record(record) => {
                for item in &record.header.rest {
                    let RecordField::Assertion(assertion) = &item.field else {
                        continue;
                    };
                    if let Some(ast::AssertionTail::Link(link)) = &assertion.tail {
                        require_reference(&link.target, identities)?;
                    }
                }
            }
            Declaration::Karma(karma) => {
                for rule in &karma.rules.rules {
                    for field in &rule.fields {
                        match field {
                            RuleField::Frequency(field) => {
                                require_reference(&field.value, identities)?
                            }
                            RuleField::Record(field) => {
                                require_reference(&field.value, identities)?
                            }
                            _ => {}
                        }
                    }
                }
            }
            Declaration::Frequency(_) => {}
        }
    }
    Ok(format(&document))
}

fn require_reference(
    reference: &ast::Reference,
    identities: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    identities
        .get(&reference.slug.0)
        .ok_or_else(|| {
            diagnostic(format!(
                "reference slug `{}` does not resolve",
                reference.slug.0
            ))
        })
        .map(|_| ())
}

pub fn resolve_project_references(
    project: &mut ProjectedDocument,
    identities: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    for record in &mut project.records {
        for assertion in &mut record.assertions {
            if let Some(slug) = &assertion.object_slug {
                assertion.object_uid = Some(
                    identities
                        .get(slug)
                        .ok_or_else(|| {
                            diagnostic(format!("reference slug `{slug}` does not resolve"))
                        })?
                        .clone(),
                );
            }
        }
    }
    for rule in &mut project.rules {
        rule.frequency_uid = Some(
            identities
                .get(&rule.frequency_slug)
                .ok_or_else(|| {
                    diagnostic(format!(
                        "reference slug `{}` does not resolve",
                        rule.frequency_slug
                    ))
                })?
                .clone(),
        );
        rule.record_uid = Some(
            identities
                .get(&rule.record_slug)
                .ok_or_else(|| {
                    diagnostic(format!(
                        "reference slug `{}` does not resolve",
                        rule.record_slug
                    ))
                })?
                .clone(),
        );
    }
    Ok(())
}

fn minted(source: &str, kind: &'static str, prefix: &str, name: &str, ordinal: usize) -> MintedUid {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let digest = Sha256::digest(format!("{kind}\0{name}\0{ordinal}\0{source}").as_bytes());
    let mut body = String::with_capacity(26);
    for index in 0..26 {
        let bit = index * 5;
        let byte = bit / 8;
        let offset = bit % 8;
        let pair = (u16::from(digest[byte]) << 8) | u16::from(digest[byte + 1]);
        let value = ((pair >> (11 - offset)) & 0x1f) as usize;
        body.push(ALPHABET[value] as char);
    }
    MintedUid {
        kind,
        name: name.to_string(),
        uid: format!("{prefix}_{body}"),
    }
}

pub fn project(document: &Document) -> Result<ProjectedDocument, Diagnostic> {
    use ast::{FrequencyField, RecordField, RuleField};
    let mut output = ProjectedDocument {
        records: Vec::new(),
        frequencies: Vec::new(),
        rules: Vec::new(),
    };
    for declaration in &document.declarations {
        match declaration {
            Declaration::Record(record) => {
                let mut assertions = Vec::new();
                let mut identity_count = 0usize;
                for item in &record.header.rest {
                    match &item.field {
                        RecordField::Identity(value) => {
                            identity_count += 1;
                            assertions.push(ProjectedAssertion {
                                predicate: value.predicate.0.clone(),
                                identity: true,
                                object_slug: None,
                                object_uid: None,
                                quantity: None,
                                unit: None,
                            });
                        }
                        RecordField::Assertion(value) => {
                            let (object_slug, quantity, unit) = match &value.tail {
                                Some(ast::AssertionTail::Link(link)) => (
                                    Some(link.target.slug.0.clone()),
                                    link.amount.as_ref().map(|v| v.quantity.value.0.clone()),
                                    link.amount
                                        .as_ref()
                                        .and_then(|v| v.quantity.unit.as_ref())
                                        .map(|u| u.name.0.clone()),
                                ),
                                Some(ast::AssertionTail::Amount(amount)) => (
                                    None,
                                    Some(amount.quantity.value.0.clone()),
                                    amount.quantity.unit.as_ref().map(|u| u.name.0.clone()),
                                ),
                                None => (None, None, None),
                            };
                            assertions.push(ProjectedAssertion {
                                predicate: value.predicate.0.clone(),
                                identity: false,
                                object_slug,
                                object_uid: None,
                                quantity,
                                unit,
                            });
                        }
                    }
                }
                let title = record.title.value();
                let label = record.header.subject.slug().unwrap_or(&title);
                if identity_count > 1 {
                    return Err(diagnostic(format!(
                        "`{label}` declares more than one identity assertion"
                    )));
                }
                let quantity = record.header.subject.quantity();
                output.records.push(ProjectedRecord {
                    uid: typed_uid(
                        required(record.opening.uid().map(str::to_string), label, "uid")?,
                        "r",
                        label,
                    )?,
                    slug: record
                        .header
                        .subject
                        .slug()
                        .map(|slug| checked_slug(slug.to_string(), label))
                        .transpose()?,
                    head: title,
                    body: record
                        .description
                        .as_ref()
                        .map_or_else(String::new, |v| decode_description_boundaries(&v.0)),
                    quantity: Some((
                        quantity.value.0.clone(),
                        quantity.unit.as_ref().map(|unit| unit.name.0.clone()),
                    )),
                    assertions,
                });
            }
            Declaration::Frequency(frequency) => {
                let mut head = None;
                let mut quantity = 1;
                let mut every = None;
                let mut timezone = None;
                let mut next_at = None;
                for field in &frequency.fields {
                    match field {
                        FrequencyField::Title(value) => head = Some(value.value.0.clone()),
                        FrequencyField::Quantity(value) => {
                            quantity = value.value.0.parse().map_err(|_| {
                                diagnostic(format!(
                                    "{} has a non-integer quantity",
                                    frequency.opening.0
                                ))
                            })?
                        }
                        FrequencyField::Every(value) => {
                            every = Some((
                                value.count.0.parse().map_err(|_| {
                                    diagnostic(format!(
                                        "{} has a non-integer interval",
                                        frequency.opening.0
                                    ))
                                })?,
                                value.unit.0.clone(),
                            ))
                        }
                        FrequencyField::Timezone(value) => timezone = Some(value.value.0.clone()),
                        FrequencyField::NextAt(value) => next_at = Some(value.value.0.clone()),
                    }
                }
                let slug = checked_slug(frequency.opening.0.clone(), &frequency.opening.0)?;
                output.frequencies.push(ProjectedFrequency {
                    uid: typed_uid(
                        required(
                            frequency.uid.as_ref().map(ast::Uid::value),
                            &frequency.opening.0,
                            "uid",
                        )?,
                        "freq",
                        &frequency.opening.0,
                    )?,
                    head: head.unwrap_or_else(|| slug.clone()),
                    slug,
                    quantity,
                    every: required(every, &frequency.opening.0, "every")?,
                    timezone: required(timezone, &frequency.opening.0, "timezone")?,
                    next_at: required(next_at, &frequency.opening.0, "next_at")?,
                });
            }
            Declaration::Karma(karma) => {
                for rule in &karma.rules.rules {
                    let mut quantity = 1;
                    let mut frequency_slug = None;
                    let mut frequency_uid = None;
                    let mut record_slug = None;
                    let mut record_uid = None;
                    let mut condition = None;
                    let mut gate = None;
                    let mut carry = None;
                    let mut consequences = None;
                    let mut note = None;
                    for field in &rule.fields {
                        match field {
                            RuleField::Quantity(value) => {
                                quantity = value.value.0.parse().map_err(|_| {
                                    diagnostic(format!(
                                        "{} has a non-integer quantity",
                                        rule.name.0
                                    ))
                                })?
                            }
                            RuleField::Frequency(value) => {
                                frequency_slug = Some(value.value.slug.0.clone());
                                frequency_uid = None
                            }
                            RuleField::Record(value) => {
                                record_slug = Some(value.value.slug.0.clone());
                                record_uid = None
                            }
                            RuleField::Condition(value) => condition = Some(value.value.0.clone()),
                            RuleField::Gate(value) => gate = Some(value.value.0.clone()),
                            RuleField::Carry(value) => carry = Some(value.value.0.clone()),
                            RuleField::Consequences(value) => {
                                consequences = Some(value.value.0.clone())
                            }
                            RuleField::Note(value) => note = Some(value.value.0.clone()),
                        }
                    }
                    output.rules.push(ProjectedRule {
                        uid: typed_uid(
                            required(rule.uid.as_ref().map(ast::Uid::value), &rule.name.0, "uid")?,
                            "rule",
                            &rule.name.0,
                        )?,
                        slug: checked_slug(rule.name.0.clone(), &rule.name.0)?,
                        quantity,
                        frequency_slug: required(frequency_slug, &rule.name.0, "frequency")?,
                        frequency_uid,
                        record_slug: required(record_slug, &rule.name.0, "record")?,
                        record_uid,
                        condition,
                        gate,
                        carry,
                        consequences_json: required(consequences, &rule.name.0, "consequences")?,
                        note,
                    });
                }
            }
        }
    }
    Ok(output)
}

fn required<T>(value: Option<T>, declaration: &str, field: &str) -> Result<T, Diagnostic> {
    value.ok_or_else(|| diagnostic(format!("`{declaration}` needs `{field}`")))
}

fn typed_uid(value: String, prefix: &str, declaration: &str) -> Result<String, Diagnostic> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let valid = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .is_some_and(|body| body.len() == 26 && body.bytes().all(|byte| ALPHABET.contains(&byte)));
    if valid {
        Ok(value)
    } else {
        Err(diagnostic(format!(
            "`{declaration}` needs a `{prefix}_` uid with 26 Crockford characters"
        )))
    }
}

fn checked_slug(value: String, declaration: &str) -> Result<String, Diagnostic> {
    let valid = !value.is_empty()
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
    if valid {
        Ok(value)
    } else {
        Err(diagnostic(format!(
            "`{declaration}` has invalid slug `{value}`"
        )))
    }
}

fn diagnostic(message: String) -> Diagnostic {
    Diagnostic {
        path: None,
        message,
    }
}

pub fn set_frequency_next_at(
    source: &str,
    uid: &str,
    expected: &str,
    next: &str,
) -> Result<String, Diagnostic> {
    use ast::FrequencyField;
    let mut document = parse(source)?;
    for declaration in &mut document.declarations {
        let Declaration::Frequency(frequency) = declaration else {
            continue;
        };
        if frequency.uid.as_ref().map(ast::Uid::value).as_deref() != Some(uid) {
            continue;
        }
        let current = frequency
            .fields
            .iter_mut()
            .find_map(|field| match field {
                FrequencyField::NextAt(value) => Some(&mut value.value.0),
                _ => None,
            })
            .ok_or_else(|| diagnostic(format!("Frequency `{uid}` has no next_at")))?;
        if current != expected {
            return Err(diagnostic(format!(
                "Frequency `{uid}` next_at changed from expected `{expected}` to `{current}`"
            )));
        }
        *current = next.to_string();
        return Ok(format(&document));
    }
    Err(diagnostic(format!("no Frequency has uid `{uid}`")))
}

pub fn set_frequency_runtime(
    source: &str,
    uid: &str,
    head: &str,
    quantity: i64,
    every: (u32, &str),
    timezone: &str,
    next_at: &str,
) -> Result<String, Diagnostic> {
    use ast::FrequencyField;
    let mut document = parse(source)?;
    for declaration in &mut document.declarations {
        let Declaration::Frequency(frequency) = declaration else {
            continue;
        };
        if frequency.uid.as_ref().map(ast::Uid::value).as_deref() != Some(uid) {
            continue;
        }
        let mut found_quantity = false;
        let mut found_next = false;
        for field in &mut frequency.fields {
            match field {
                FrequencyField::Title(value) => value.value.0 = head.to_string(),
                FrequencyField::Quantity(value) => {
                    value.value.0 = quantity.to_string();
                    found_quantity = true;
                }
                FrequencyField::NextAt(value) => {
                    value.value.0 = next_at.to_string();
                    found_next = true;
                }
                FrequencyField::Every(value) => {
                    value.count.0 = every.0.to_string();
                    value.unit.0 = every.1.to_string();
                }
                FrequencyField::Timezone(value) => value.value.0 = timezone.to_string(),
            }
        }
        if !found_quantity || !found_next {
            return Err(diagnostic(format!(
                "Frequency `{uid}` lacks runtime fields"
            )));
        }
        return Ok(format(&document));
    }
    Err(diagnostic(format!("no Frequency has uid `{uid}`")))
}

#[allow(clippy::too_many_arguments)]
pub fn set_rule_runtime(
    source: &str,
    uid: &str,
    quantity: i64,
    frequency_slug: &str,
    frequency_uid: &str,
    record_slug: &str,
    record_uid: &str,
    condition: Option<(&str, &str, &str)>,
    consequences: &str,
    note: Option<&str>,
) -> Result<String, Diagnostic> {
    use ast::RuleField;
    let mut document = parse(source)?;
    for declaration in &mut document.declarations {
        let Declaration::Karma(karma) = declaration else {
            continue;
        };
        for rule in &mut karma.rules.rules {
            if rule.uid.as_ref().map(ast::Uid::value).as_deref() != Some(uid) {
                continue;
            }
            rule.fields.clear();
            rule.fields.push(RuleField::quantity(quantity));
            let _ = (frequency_uid, record_uid);
            rule.fields
                .push(RuleField::frequency(frequency_slug.to_string()));
            rule.fields.push(RuleField::record(record_slug.to_string()));
            if let Some((source, gate, carry)) = condition {
                rule.fields.push(RuleField::condition(source.to_string()));
                rule.fields.push(RuleField::gate(gate.to_string()));
                rule.fields.push(RuleField::carry(carry.to_string()));
            }
            rule.fields
                .push(RuleField::consequences(consequences.to_string()));
            if let Some(note) = note {
                rule.fields.push(RuleField::note(note.to_string()));
            }
            return Ok(format(&document));
        }
    }
    Err(diagnostic(format!("no Rule has uid `{uid}`")))
}

pub fn set_record_runtime(
    source: &str,
    uid: &str,
    head: &str,
    body: &str,
    quantity: &str,
    assertions: &[ProjectedAssertion],
) -> Result<String, Diagnostic> {
    use ast::RecordField;
    let mut document = parse(source)?;
    for declaration in &mut document.declarations {
        let Declaration::Record(record) = declaration else {
            continue;
        };
        if record.opening.uid() != Some(uid) {
            continue;
        }
        record.title.set(head);
        record.description =
            (!body.is_empty()).then(|| ast::Description(encode_description_boundaries(body)));
        record.header.subject.quantity_mut().value.0 = quantity.to_string();
        let mut comments = HashMap::<String, VecDeque<ast::Comment>>::new();
        for item in &record.header.rest {
            if let Some(comment) = &item.comment {
                comments
                    .entry(record_field_key(&item.field))
                    .or_default()
                    .push_back(comment.clone());
            }
        }
        record.header.rest.clear();
        for assertion in assertions {
            let field = if assertion.identity {
                RecordField::identity(assertion.predicate.clone())
            } else if let Some(slug) = &assertion.object_slug {
                RecordField::link(
                    assertion.predicate.clone(),
                    slug.clone(),
                    assertion.quantity.clone(),
                    assertion.unit.clone(),
                )
            } else if let Some(quantity) = &assertion.quantity {
                RecordField::amount(
                    assertion.predicate.clone(),
                    quantity.clone(),
                    assertion.unit.clone(),
                )
            } else {
                RecordField::assertion(assertion.predicate.clone())
            };
            let key = record_field_key(&field);
            let mut item = ast::RecordHeaderRest::new(field);
            item.comment = comments.get_mut(&key).and_then(VecDeque::pop_front);
            record.header.rest.push(item);
        }
        return Ok(format(&document));
    }
    Err(diagnostic(format!("no Record has uid `{uid}`")))
}

fn record_field_key(field: &ast::RecordField) -> String {
    use ast::RecordField;
    match field {
        RecordField::Identity(value) => format!("identity:{}", value.predicate.0),
        RecordField::Assertion(value) => {
            let tail = match &value.tail {
                Some(ast::AssertionTail::Link(link)) => format!("@{}", link.target.slug.0),
                Some(ast::AssertionTail::Amount(_)) => "amount".to_string(),
                None => "assertion".to_string(),
            };
            format!("{}:{tail}", value.predicate.0)
        }
    }
}

pub fn check_project(root: &Path) -> Result<Vec<PathBuf>, Vec<Diagnostic>> {
    let paths = lingua_paths(root).map_err(|error| vec![error])?;
    let mut documents = Vec::with_capacity(paths.len());
    let mut errors = Vec::new();
    for path in &paths {
        match parse_path(path) {
            Ok(document) => documents.push((path, document)),
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut uids = BTreeMap::<String, &Path>::new();
    let mut slugs = BTreeMap::<String, &Path>::new();
    let mut references = Vec::<(String, &Path)>::new();
    for (path, document) in &documents {
        validate_document(
            document,
            path,
            &mut uids,
            &mut slugs,
            &mut references,
            &mut errors,
        );
    }
    for (slug, path) in references {
        if !slugs.contains_key(&slug) {
            errors.push(Diagnostic {
                path: Some(path.to_path_buf()),
                message: format!("reference slug `{slug}` does not resolve"),
            });
        }
    }
    if errors.is_empty() {
        Ok(paths)
    } else {
        Err(errors)
    }
}

pub fn lingua_paths(root: &Path) -> Result<Vec<PathBuf>, Diagnostic> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_paths(root: &Path, path: &Path, output: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    for entry in fs::read_dir(path).map_err(|error| Diagnostic {
        path: Some(path.to_path_buf()),
        message: error.to_string(),
    })? {
        let entry = entry.map_err(|error| Diagnostic {
            path: Some(path.to_path_buf()),
            message: error.to_string(),
        })?;
        let child = entry.path();
        if child.is_dir() {
            if child
                .strip_prefix(root)
                .ok()
                .and_then(|value| value.components().next())
                == Some(std::path::Component::Normal("state".as_ref()))
            {
                continue;
            }
            collect_paths(root, &child, output)?;
        } else if child.extension().and_then(|value| value.to_str()) == Some(EXTENSION) {
            output.push(child);
        }
    }
    Ok(())
}

fn insert_unique<'a>(
    values: &mut BTreeMap<String, &'a Path>,
    value: &str,
    path: &'a Path,
    kind: &str,
    errors: &mut Vec<Diagnostic>,
) {
    if let Some(first) = values.insert(value.to_string(), path) {
        errors.push(Diagnostic {
            path: Some(path.to_path_buf()),
            message: format!(
                "duplicate {kind} `{value}` (first declared in {})",
                first.display()
            ),
        });
    }
}

fn validate_document<'a>(
    document: &ast::Document,
    path: &'a Path,
    uids: &mut BTreeMap<String, &'a Path>,
    slugs: &mut BTreeMap<String, &'a Path>,
    references: &mut Vec<(String, &'a Path)>,
    errors: &mut Vec<Diagnostic>,
) {
    use ast::{FrequencyField, RecordField, RuleField};
    for declaration in &document.declarations {
        match declaration {
            Declaration::Record(record) => {
                let title = record.title.value();
                let label = record.header.subject.slug().unwrap_or(&title);
                let identity_count = record
                    .header
                    .rest
                    .iter()
                    .filter(|item| matches!(&item.field, RecordField::Identity(_)))
                    .count();
                if identity_count > 1 {
                    errors.push(Diagnostic {
                        path: Some(path.to_path_buf()),
                        message: format!("`{label}` declares more than one identity assertion"),
                    });
                }
                if let Some(slug) = record.header.subject.slug() {
                    insert_unique(slugs, slug, path, "slug", errors);
                    if let Err(error) = checked_slug(slug.to_string(), label) {
                        errors.push(Diagnostic {
                            path: Some(path.to_path_buf()),
                            message: error.message,
                        });
                    }
                }
                if title.is_empty() {
                    errors.push(Diagnostic {
                        path: Some(path.to_path_buf()),
                        message: "a Record title cannot be empty".to_string(),
                    });
                }
                if let Some(uid) = record.opening.uid() {
                    insert_unique(uids, uid, path, "uid", errors);
                    if let Err(error) = typed_uid(uid.to_string(), "r", label) {
                        errors.push(Diagnostic {
                            path: Some(path.to_path_buf()),
                            message: error.message,
                        });
                    }
                }
                for item in &record.header.rest {
                    if let RecordField::Assertion(field) = &item.field
                        && let Some(ast::AssertionTail::Link(link)) = &field.tail
                    {
                        references.push((link.target.slug.0.clone(), path));
                    }
                }
            }
            Declaration::Frequency(frequency) => {
                insert_unique(slugs, &frequency.opening.0, path, "slug", errors);
                if let Err(error) = checked_slug(frequency.opening.0.clone(), &frequency.opening.0)
                {
                    errors.push(Diagnostic {
                        path: Some(path.to_path_buf()),
                        message: error.message,
                    });
                }
                let mut seen = HashSet::new();
                if let Some(uid) = &frequency.uid {
                    insert_unique(uids, &uid.value(), path, "uid", errors);
                    validate_typed_uid(path, &frequency.opening.0, uid, "freq", errors);
                }
                for field in &frequency.fields {
                    let key = match field {
                        FrequencyField::Title(_) => "title",
                        FrequencyField::Quantity(field) => {
                            validate_switch(path, &frequency.opening.0, &field.value.0, errors);
                            "quantity"
                        }
                        FrequencyField::Every(_) => "every",
                        FrequencyField::Timezone(_) => "timezone",
                        FrequencyField::NextAt(_) => "next_at",
                    };
                    duplicate_field(path, &frequency.opening.0, key, &mut seen, errors);
                }
                require_fields(
                    path,
                    &frequency.opening.0,
                    &seen,
                    &["quantity", "every", "timezone", "next_at"],
                    errors,
                );
            }
            Declaration::Karma(karma) => {
                for rule in &karma.rules.rules {
                    insert_unique(slugs, &rule.name.0, path, "slug", errors);
                    if let Err(error) = checked_slug(rule.name.0.clone(), &rule.name.0) {
                        errors.push(Diagnostic {
                            path: Some(path.to_path_buf()),
                            message: error.message,
                        });
                    }
                    let mut seen = HashSet::new();
                    if let Some(uid) = &rule.uid {
                        insert_unique(uids, &uid.value(), path, "uid", errors);
                        validate_typed_uid(path, &rule.name.0, uid, "rule", errors);
                    }
                    for field in &rule.fields {
                        let key = match field {
                            RuleField::Quantity(field) => {
                                validate_switch(path, &rule.name.0, &field.value.0, errors);
                                "quantity"
                            }
                            RuleField::Frequency(field) => {
                                references.push((field.value.slug.0.clone(), path));
                                "frequency"
                            }
                            RuleField::Record(field) => {
                                references.push((field.value.slug.0.clone(), path));
                                "record"
                            }
                            RuleField::Condition(_) => "condition",
                            RuleField::Gate(_) => "gate",
                            RuleField::Carry(_) => "carry",
                            RuleField::Consequences(_) => "consequences",
                            RuleField::Note(_) => "note",
                        };
                        duplicate_field(path, &rule.name.0, key, &mut seen, errors);
                    }
                    require_fields(
                        path,
                        &rule.name.0,
                        &seen,
                        &["quantity", "frequency", "record", "consequences"],
                        errors,
                    );
                }
            }
        }
    }
}

fn validate_typed_uid(
    path: &Path,
    declaration: &str,
    uid: &ast::Uid,
    prefix: &str,
    errors: &mut Vec<Diagnostic>,
) {
    if let Err(error) = typed_uid(uid.value(), prefix, declaration) {
        errors.push(Diagnostic {
            path: Some(path.to_path_buf()),
            message: error.message,
        });
    }
}

fn duplicate_field(
    path: &Path,
    declaration: &str,
    field: &'static str,
    seen: &mut HashSet<&'static str>,
    errors: &mut Vec<Diagnostic>,
) {
    if !seen.insert(field) {
        errors.push(Diagnostic {
            path: Some(path.to_path_buf()),
            message: format!("`{declaration}` declares `{field}` more than once"),
        });
    }
}

fn require_fields(
    path: &Path,
    declaration: &str,
    seen: &HashSet<&str>,
    required: &[&str],
    errors: &mut Vec<Diagnostic>,
) {
    for field in required {
        if !seen.contains(field) {
            errors.push(Diagnostic {
                path: Some(path.to_path_buf()),
                message: format!("`{declaration}` needs `{field}`"),
            });
        }
    }
}

fn validate_switch(path: &Path, declaration: &str, quantity: &str, errors: &mut Vec<Diagnostic>) {
    if quantity != "0" && quantity != "1" {
        errors.push(Diagnostic {
            path: Some(path.to_path_buf()),
            message: format!("`{declaration}` quantity is a switch and must be 0 or 1"),
        });
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("strings serialize")
}
fn body(value: &str) -> String {
    format!("\"\"\"{value}\"\"\"")
}
fn quantity(value: &ast::Quantity) -> String {
    format!(
        "{}{}",
        value.value.0,
        value
            .unit
            .as_ref()
            .map_or(String::new(), |unit| format!(" @{}", unit.name.0))
    )
}
fn reference(value: &ast::Reference) -> String {
    format!("@{}", value.slug.0)
}

fn record_boundary_line(line: &str) -> Option<(usize, &str)> {
    let value = line.strip_prefix("} ")?;
    let leading_quotes = value.bytes().take_while(|byte| *byte == b'"').count();
    let trailing_quotes = value.bytes().rev().take_while(|byte| *byte == b'"').count();
    if leading_quotes != trailing_quotes || value.len() < leading_quotes * 2 {
        return None;
    }
    let uid = &value[leading_quotes..value.len() - trailing_quotes];
    typed_uid(uid.to_string(), "r", "description boundary").ok()?;
    Some((leading_quotes, uid))
}

fn rewrite_description_boundaries(value: &str, delta: i8) -> String {
    value
        .split('\n')
        .map(|line| {
            let Some((quotes, uid)) = record_boundary_line(line) else {
                return line.to_string();
            };
            let quotes = if delta > 0 {
                quotes + 1
            } else if quotes > 0 {
                quotes - 1
            } else {
                return line.to_string();
            };
            format!("}} {}{uid}{}", "\"".repeat(quotes), "\"".repeat(quotes))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn encode_description_boundaries(value: &str) -> String {
    rewrite_description_boundaries(value, 1)
}

fn decode_description_boundaries(value: &str) -> String {
    rewrite_description_boundaries(value, -1)
}

fn record_field(field: &ast::RecordField) -> String {
    use ast::RecordField::*;
    match field {
        Identity(value) => format!("is #{}", value.predicate.0),
        Assertion(value) => match &value.tail {
            Some(ast::AssertionTail::Link(link)) => format!(
                "#{} {}{}",
                value.predicate.0,
                reference(&link.target),
                link.amount.as_ref().map_or(String::new(), |amount| {
                    format!(": {}", quantity(&amount.quantity))
                })
            ),
            Some(ast::AssertionTail::Amount(amount)) => {
                format!("#{}: {}", value.predicate.0, quantity(&amount.quantity))
            }
            None => format!("#{}", value.predicate.0),
        },
    }
}

fn format_record(output: &mut String, record: &ast::Record) {
    let subject = match &record.header.subject {
        ast::RecordSubject::Slugged(value) => {
            format!("@{}: {}", value.slug.0, quantity(&value.quantity))
        }
        ast::RecordSubject::Anonymous(value) => quantity(value),
    };
    let fields: Vec<String> = record
        .header
        .rest
        .iter()
        .map(|item| record_field(&item.field))
        .collect();
    let comments = record.header.rest.iter().any(|item| item.comment.is_some());
    let title = record.title.value();
    let opening_uid = record.opening.uid();
    let mut compact = format!("{title} ({subject}");
    for field in &fields {
        write!(compact, ", {field}").unwrap();
    }
    compact.push_str(") {");
    if let Some(uid) = opening_uid {
        write!(compact, " {uid}").unwrap();
    }
    if !comments && compact.chars().count() <= 150 {
        writeln!(output, "{compact}").unwrap();
    } else {
        writeln!(output, "{title} (").unwrap();
        write!(output, "    {subject}").unwrap();
        for (index, item) in record.header.rest.iter().enumerate() {
            output.push(',');
            if let Some(comment) = &item.comment {
                write!(output, " {}", comment.0).unwrap();
            }
            writeln!(output).unwrap();
            write!(output, "    {}", fields[index]).unwrap();
        }
        write!(output, "\n) {{").unwrap();
        if let Some(uid) = opening_uid {
            write!(output, " {uid}").unwrap();
        }
        output.push('\n');
    }
    if let Some(description) = &record.description {
        let logical = decode_description_boundaries(&description.0);
        let encoded = encode_description_boundaries(&logical);
        output.push_str(encoded.trim_end_matches('\n'));
        if !description.0.is_empty() {
            output.push('\n');
        }
    }
    output.push('}');
    if let Some(uid) = record.closing.uid() {
        write!(output, " {uid}").unwrap();
    }
    output.push('\n');
}

fn format_frequency(output: &mut String, frequency: &ast::Frequency) {
    use ast::FrequencyField::*;
    writeln!(output, "Frequency {} {{", frequency.opening.0).unwrap();
    for field in &frequency.fields {
        match field {
            Title(v) => writeln!(output, "    title {}", quoted(&v.value.0)),
            Quantity(v) => writeln!(output, "    quantity {}", v.value.0),
            Every(v) => writeln!(output, "    every {} {}", v.count.0, v.unit.0),
            Timezone(v) => writeln!(output, "    timezone {}", quoted(&v.value.0)),
            NextAt(v) => writeln!(output, "    next_at {}", v.value.0),
        }
        .unwrap();
    }
    output.push('}');
    if let Some(uid) = &frequency.uid {
        write!(output, " ^{}", uid.value()).unwrap();
    }
    output.push('\n');
}

fn format_karma(output: &mut String, karma: &ast::Karma) {
    use ast::RuleField::*;
    writeln!(output, "Karma {} {{\n    Rules {{", karma.opening.0).unwrap();
    for rule in &karma.rules.rules {
        writeln!(output, "        Rule {} {{", rule.name.0).unwrap();
        for field in &rule.fields {
            match field {
                Quantity(v) => writeln!(output, "            quantity {}", v.value.0),
                Frequency(v) => writeln!(output, "            frequency {}", reference(&v.value)),
                Record(v) => writeln!(output, "            record {}", reference(&v.value)),
                Condition(v) => writeln!(output, "            condition {}", body(&v.value.0)),
                Gate(v) => writeln!(output, "            gate {}", v.value.0),
                Carry(v) => writeln!(output, "            carry {}", v.value.0),
                Consequences(v) => {
                    writeln!(output, "            consequences {}", body(&v.value.0))
                }
                Note(v) => writeln!(output, "            note {}", quoted(&v.value.0)),
            }
            .unwrap();
        }
        output.push_str("        }");
        if let Some(uid) = &rule.uid {
            write!(output, " ^{}", uid.value()).unwrap();
        }
        output.push('\n');
    }
    output.push_str("    }\n}\n");
}

pub fn hash(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Commitment {
    pub version: u32,
    pub generation: u64,
    pub relative_path: PathBuf,
    pub disk_hash: String,
    pub projection_hash: String,
    pub declarations: BTreeMap<String, String>,
    pub baseline: Vec<u8>,
}

impl Commitment {
    pub fn encode(&self) -> Result<Vec<u8>, Diagnostic> {
        postcard::to_allocvec(self).map_err(|error| Diagnostic {
            path: None,
            message: error.to_string(),
        })
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Diagnostic> {
        postcard::from_bytes(bytes).map_err(|error| Diagnostic {
            path: None,
            message: error.to_string(),
        })
    }
}

pub fn state_paths(sync_dir: &Path, relative_file: &Path) -> StatePaths {
    let key = hash(relative_file.as_os_str().as_encoded_bytes());
    let proc = sync_dir.join("state").join("proc");
    StatePaths {
        commitment: proc.join("commitment").join(format!("{key}.state")),
        pending: proc.join("pending").join(format!("{key}.state")),
        conflict: proc.join("conflicts").join(format!("{key}.state")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePaths {
    pub commitment: PathBuf,
    pub pending: PathBuf,
    pub conflict: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"Rent (@rent: 1, is #task) { r_00000000000000000000000001
Pay it.
} r_00000000000000000000000001
Frequency monthly {
 title "Monthly"
 quantity 1
 every 1 month
 timezone "America/Sao_Paulo"
 next_at 2026-09-01T09:00:00-03:00
} ^freq_00000000000000000000000002
Karma home {
 Rules {
  Rule pay-rent {
   quantity 1
   frequency @monthly
   record @rent
   consequences """[{"kind":"set-concept","concept":"paid"}]"""
  } ^rule_00000000000000000000000003
 }
}
"#;

    #[test]
    fn hashes_use_fixed_width_lowercase_sha256() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nested_schema_round_trips_canonically() {
        let canonical = canonicalize(SOURCE).expect("parses");
        assert_eq!(canonicalize(&canonical).expect("parses again"), canonical);
        assert!(canonical.contains("Karma home {\n    Rules {\n        Rule pay-rent"));
    }

    #[test]
    fn state_is_below_the_reserved_machine_tree() {
        let paths = state_paths(Path::new("/sync"), Path::new("rent.lingua"));
        assert!(paths.commitment.starts_with("/sync/state/proc/commitment"));
        assert!(paths.pending.starts_with("/sync/state/proc/pending"));
        assert!(paths.conflict.starts_with("/sync/state/proc/conflicts"));
    }

    #[test]
    fn missing_declaration_uids_are_minted_once_and_formatted() {
        let source = r#"A note (0) {
A note.
}
"#;
        let (first, minted) = ensure_uids(source).expect("mint uid");
        assert_eq!(minted.len(), 1);
        assert!(first.contains(&format!("{{ {}", minted[0].uid)));
        assert!(first.contains(&format!("}} {}", minted[0].uid)));
        let (second, minted_again) = ensure_uids(&first).expect("read durable uid");
        assert!(minted_again.is_empty());
        assert_eq!(second, first);
    }

    #[test]
    fn same_titled_slugless_records_receive_distinct_uids() {
        let source = "Note (0) {\nFirst.\n}\nNote (0) {\nSecond.\n}\n";
        let (_, minted) = ensure_uids(source).expect("mint identities");
        assert_eq!(minted.len(), 2);
        assert_ne!(minted[0].uid, minted[1].uid);
    }

    #[test]
    fn database_projection_can_rewrite_complete_runtime_declarations() {
        let source = set_frequency_runtime(
            SOURCE,
            "freq_00000000000000000000000002",
            "Every week",
            0,
            (1, "week"),
            "UTC",
            "2026-09-08T09:00:00Z",
        )
        .expect("rewrite frequency");
        let source = set_rule_runtime(
            &source,
            "rule_00000000000000000000000003",
            0,
            "weekly",
            "freq_00000000000000000000000002",
            "rent",
            "r_00000000000000000000000001",
            Some(("@rent", "nonzero", "value")),
            "[]",
            Some("disabled while testing"),
        )
        .expect("rewrite rule");
        let project = project(&parse(&source).expect("parse rewritten source"))
            .expect("project rewritten source");

        assert_eq!(project.frequencies[0].slug, "monthly");
        assert_eq!(project.frequencies[0].quantity, 0);
        assert_eq!(project.frequencies[0].every, (1, "week".to_string()));
        assert_eq!(project.rules[0].slug, "pay-rent");
        assert_eq!(project.rules[0].frequency_slug, "weekly");
        assert_eq!(project.rules[0].condition.as_deref(), Some("@rent"));
        assert_eq!(
            project.rules[0].note.as_deref(),
            Some("disabled while testing")
        );
    }

    #[test]
    fn slug_only_references_are_bound_after_lince_mints_declaration_uids() {
        let source = r#"Counter (@counter: 0) {
Counter.
}
Frequency fast {
    quantity 1
    every 5 seconds
    timezone "UTC"
    next_at 2026-09-01T09:00:00Z
}
Karma demo {
    Rules {
        Rule toggle {
            quantity 1
            frequency @fast
            record @counter
            consequences """[{"kind":"set-quantity"}]"""
        }
    }
}
"#;
        let (with_uids, minted) = ensure_uids(source).expect("mint declaration identities");
        let identities = minted
            .iter()
            .map(|identity| (identity.name.clone(), identity.uid.clone()))
            .collect();
        let bound = bind_reference_uids(&with_uids, &identities).expect("bind references");
        let mut projected = project(&parse(&bound).expect("parse bound source")).expect("project");
        resolve_project_references(&mut projected, &identities).expect("resolve projected ids");

        assert_eq!(minted.len(), 3);
        assert_eq!(
            projected.rules[0].frequency_uid.as_deref(),
            Some(projected.frequencies[0].uid.as_str())
        );
        assert_eq!(
            projected.rules[0].record_uid.as_deref(),
            Some(projected.records[0].uid.as_str())
        );
    }

    #[test]
    fn slugless_and_keyword_named_records_are_unambiguous() {
        let source = "Karma (0) {\nA Record named Karma.\n}\nRule (@rule-note: 0) {\nA Record named Rule.\n}\n";
        let (source, _) = ensure_uids(source).expect("mint identities");
        let project = project(&parse(&source).expect("parse")).expect("project");
        assert_eq!(project.records.len(), 2);
        assert_eq!(project.records[0].slug, None);
        assert_eq!(project.records[1].slug.as_deref(), Some("rule-note"));
    }

    #[test]
    fn metadata_comments_survive_formatting() {
        let source = "Commented (@commented: 0, // why it exists\n#done) {\nText.\n}\n";
        let canonical = canonicalize(source).expect("parse");
        assert!(canonical.contains("// why it exists"));
        assert_eq!(canonicalize(&canonical).expect("reparse"), canonical);
    }

    #[test]
    fn metadata_comments_survive_runtime_writeback() {
        let source = "Commented (@commented: 0, // completion is independent\n#done) { r_00000000000000000000000001\nText.\n} r_00000000000000000000000001\n";
        let projected = project(&parse(source).expect("parse")).expect("project");
        let rewritten = set_record_runtime(
            source,
            "r_00000000000000000000000001",
            "Commented",
            "Text.",
            "1",
            &projected.records[0].assertions,
        )
        .expect("rewrite");
        assert!(rewritten.contains("// completion is independent"));
        assert!(rewritten.contains("@commented: 1"));
    }

    #[test]
    fn a_record_has_at_most_one_identity_assertion() {
        let source = "Ambiguous (@ambiguous: 0, is #task, is #chapter) { r_00000000000000000000000001\nText.\n} r_00000000000000000000000001\n";
        let error = project(&parse(source).expect("parse")).expect_err("reject identities");
        assert!(error.message.contains("more than one identity"));
    }

    #[test]
    fn record_uid_must_match_on_both_description_boundaries() {
        let source =
            "Mismatch (0) { r_00000000000000000000000001\nText.\n} r_00000000000000000000000002\n";
        let error = parse(source).expect_err("reject mismatched boundary");
        assert!(error.message.contains("opens with uid"));
    }

    #[test]
    fn boundary_shaped_description_lines_are_escaped_reversibly() {
        let uid = "r_00000000000000000000000001";
        let source =
            format!("Boundary (@boundary: 0) {{ {uid}\n}} \"{uid}\"\n}} \"\"{uid}\"\"\n}} {uid}\n");
        let document = parse(&source).expect("parse escaped boundaries");
        let projected = project(&document).expect("project logical description");
        assert_eq!(
            projected.records[0].body,
            format!("}} {uid}\n}} \"{uid}\"\n")
        );
        assert_eq!(format(&document), source);

        let rewritten = set_record_runtime(
            &source,
            uid,
            "Boundary",
            &format!("}} {uid}\n}} \"{uid}\""),
            "0",
            &[],
        )
        .expect("escape logical runtime body");
        assert!(rewritten.contains(&format!("}} \"{uid}\"\n}} \"\"{uid}\"\"")));
    }
}
