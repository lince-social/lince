use super::{
    Diagnostic, ProjectedAssertion, ProjectedDocument, ProjectedFrequency, ProjectedImport,
    ProjectedRecord, ProjectedRule, ast, checked_slug, decode_description_boundaries, diagnostic,
    extension, required, typed_uid,
};
use ast::{Declaration, Document, FrequencyField, RecordField, RuleField};

pub fn project(document: &Document) -> Result<ProjectedDocument, Diagnostic> {
    let mut output = ProjectedDocument {
        imports: Vec::new(),
        records: Vec::new(),
        frequencies: Vec::new(),
        rules: Vec::new(),
        extensions: Vec::new(),
    };
    visit(document, Some(&mut output))?;
    Ok(output)
}

pub fn validate_projection(document: &Document) -> Result<(), Diagnostic> {
    visit(document, None)
}

fn visit(
    document: &Document,
    mut output: Option<&mut ProjectedDocument>,
) -> Result<(), Diagnostic> {
    for declaration in &document.declarations {
        match declaration {
            Declaration::Import(value) => {
                if let Some(output) = output.as_mut() {
                    output.imports.push(ProjectedImport {
                        target: if value.target.is_uri() {
                            value.target.value().to_owned()
                        } else {
                            format!("@{}", value.target.value())
                        },
                        locations: value
                            .locations
                            .as_ref()
                            .map(|locations| {
                                locations
                                    .values
                                    .iter()
                                    .map(|value| value.value.0.clone())
                                    .collect()
                            })
                            .unwrap_or_default(),
                    });
                }
            }
            Declaration::Statement(_) => {
                return Err(diagnostic(
                    "Script statements require `lince lingua FILE`; import and sync do not execute scripts",
                ));
            }
            Declaration::Extension(value) => {
                let extension = extension::project(value)?;
                if let Some(output) = output.as_mut() {
                    output.extensions.push(extension);
                }
            }
            Declaration::Record(record) => {
                let mut assertions = output.as_ref().map(|_| Vec::new());
                let mut identity_count = 0usize;
                for item in &record.header.rest {
                    if matches!(&item.field, RecordField::Identity(_)) {
                        identity_count += 1;
                    }
                    let Some(assertions) = assertions.as_mut() else {
                        continue;
                    };
                    match &item.field {
                        RecordField::Identity(value) => {
                            assertions.push(ProjectedAssertion {
                                predicate: value.predicate.0.clone(),
                                identity: true,
                                object_uri: None,
                                object_slug: None,
                                object_uid: None,
                                quantity: None,
                                unit: None,
                            });
                        }
                        RecordField::Assertion(value) => {
                            let (object_slug, quantity, unit) = match &value.tail {
                                Some(ast::AssertionTail::Link(link)) => (
                                    (!link.target.is_uri()).then(|| link.target.value().to_owned()),
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
                                object_uri: match &value.tail {
                                    Some(ast::AssertionTail::Link(link))
                                        if link.target.is_uri() =>
                                    {
                                        Some(link.target.value().to_owned())
                                    }
                                    _ => None,
                                },
                                object_slug,
                                object_uid: None,
                                quantity,
                                unit,
                            });
                        }
                    }
                }
                let title = if output.is_some() || record.header.subject.slug().is_none() {
                    record.title.value()
                } else {
                    String::new()
                };
                let label = record.header.subject.slug().unwrap_or(&title);
                if identity_count > 1 {
                    return Err(diagnostic(format!(
                        "`{label}` declares more than one identity assertion"
                    )));
                }
                let uid = typed_uid(required(record.opening.uid(), label, "uid")?, "r", label)?;
                let slug = record
                    .header
                    .subject
                    .slug()
                    .map(|slug| checked_slug(slug, label))
                    .transpose()?;
                if let Some(output) = output.as_mut() {
                    let quantity = record.header.subject.quantity();
                    output.records.push(ProjectedRecord {
                        uid: uid.into(),
                        slug: slug.map(str::to_owned),
                        head: title,
                        body: record
                            .description
                            .as_ref()
                            .map_or_else(String::new, |v| decode_description_boundaries(&v.0)),
                        quantity: Some((
                            quantity.value.0.clone(),
                            quantity.unit.as_ref().map(|unit| unit.name.0.clone()),
                        )),
                        assertions: assertions.unwrap(),
                    });
                }
            }
            Declaration::Frequency(frequency) => {
                let mut head = None;
                let mut quantity = 1;
                let mut every = None;
                let mut timezone = None;
                let mut next_at = None;
                let mut definition = None;
                for field in &frequency.fields {
                    match field {
                        FrequencyField::Title(value) => head = Some(value.value.0.as_str()),
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
                                value.unit.0.as_str(),
                            ))
                        }
                        FrequencyField::Timezone(value) => timezone = Some(value.value.0.as_str()),
                        FrequencyField::NextAt(value) => next_at = Some(value.value.0.as_str()),
                        FrequencyField::Definition(value) => {
                            definition = Some(value.value.0.as_str())
                        }
                    }
                }
                let slug = checked_slug(&frequency.opening.0, &frequency.opening.0)?;
                let uid = typed_uid(
                    required(
                        frequency.uid.as_ref().map(|uid| uid.0.as_str()),
                        &frequency.opening.0,
                        "uid",
                    )?,
                    "freq",
                    &frequency.opening.0,
                )?;
                let every = if definition.is_some() {
                    every.unwrap_or_default()
                } else {
                    required(every, &frequency.opening.0, "every")?
                };
                let timezone = if definition.is_some() {
                    timezone.unwrap_or_default()
                } else {
                    required(timezone, &frequency.opening.0, "timezone")?
                };
                let next_at = if definition.is_some() {
                    next_at.unwrap_or_default()
                } else {
                    required(next_at, &frequency.opening.0, "next_at")?
                };
                if let Some(output) = output.as_mut() {
                    output.frequencies.push(ProjectedFrequency {
                        uid: uid.into(),
                        head: head.unwrap_or(slug).into(),
                        slug: slug.into(),
                        quantity,
                        every: (every.0, every.1.into()),
                        timezone: timezone.into(),
                        next_at: next_at.into(),
                        definition: definition.map(str::to_owned),
                    });
                }
            }
            Declaration::Karma(karma) => {
                for rule in &karma.rules.rules {
                    let mut quantity = 1;
                    let mut frequency_slug = None;
                    let mut record_slug = None;
                    let mut condition = None;
                    let mut gate = None;
                    let mut carry = None;
                    let mut consequences = None;
                    let mut note = None;
                    let mut cadence = None;
                    let mut anchor_at = None;
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
                                frequency_slug = Some(value.value.value());
                            }
                            RuleField::Record(value) => {
                                record_slug = Some(value.value.value());
                            }
                            RuleField::Condition(value) => condition = Some(value.value.0.as_str()),
                            RuleField::Gate(value) => gate = Some(value.value.0.as_str()),
                            RuleField::Carry(value) => carry = Some(value.value.0.as_str()),
                            RuleField::Consequences(value) => {
                                consequences = Some(value.value.0.as_str())
                            }
                            RuleField::Note(value) => note = Some(value.value.0.as_str()),
                            RuleField::Cadence(value) => cadence = Some(value.value.0.as_str()),
                            RuleField::AnchorAt(value) => anchor_at = Some(value.value.0.as_str()),
                        }
                    }
                    let uid = typed_uid(
                        required(
                            rule.uid.as_ref().map(|uid| uid.0.as_str()),
                            &rule.name.0,
                            "uid",
                        )?,
                        "rule",
                        &rule.name.0,
                    )?;
                    let slug = checked_slug(&rule.name.0, &rule.name.0)?;
                    let record_slug = required(record_slug, &rule.name.0, "record")?;
                    let consequences = required(consequences, &rule.name.0, "consequences")?;
                    if let Some(output) = output.as_mut() {
                        output.rules.push(ProjectedRule {
                            uid: uid.into(),
                            slug: slug.into(),
                            quantity,
                            frequency_slug: frequency_slug.unwrap_or_default().into(),
                            frequency_uid: None,
                            record_slug: record_slug.into(),
                            record_uid: None,
                            condition: condition.map(str::to_owned),
                            gate: gate.map(str::to_owned),
                            carry: carry.map(str::to_owned),
                            consequences_json: consequences.into(),
                            note: note.map(str::to_owned),
                            cadence: cadence.map(str::to_owned),
                            anchor_at: anchor_at.map(str::to_owned),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
