use crate::{Diagnostic, ProjectedRecord};
use std::collections::{BTreeMap, BTreeSet};

pub struct Requirement<'a> {
    pub slug: &'a str,
    pub parent: Option<&'a str>,
}

pub fn validate(
    source: &str,
    required: &[Requirement<'_>],
) -> Result<Vec<ProjectedRecord>, Vec<Diagnostic>> {
    let error = |message: String| Diagnostic {
        path: None,
        message,
    };
    let (identified, _) = crate::ensure_uids(source).map_err(|error| vec![error])?;
    let document = crate::parse(&identified).map_err(|error| vec![error])?;
    let projected = crate::project(&document).map_err(|error| vec![error])?;
    let records = projected.records;
    let mut errors = Vec::new();
    let mut uids = BTreeSet::new();
    let mut slugs = BTreeMap::new();
    for record in &records {
        if !uids.insert(&record.uid) {
            errors.push(error(format!(
                "Duplicate Record UID {} ({})",
                record.uid, record.head
            )));
        }
        if let Some(slug) = &record.slug
            && slugs.insert(slug.as_str(), record).is_some()
        {
            errors.push(error(format!("Duplicate slug @{slug}")));
        }
    }
    let mut sibling_orders = BTreeMap::new();
    for requirement in required {
        let Some(record) = slugs.get(requirement.slug) else {
            errors.push(error(format!(
                "Missing required Instinct Record @{}",
                requirement.slug
            )));
            continue;
        };
        if record.body.trim().is_empty() {
            errors.push(error(format!(
                "@{} needs an explanation or instruction",
                requirement.slug
            )));
        }
        if !record
            .assertions
            .iter()
            .any(|assertion| assertion.predicate == "instinct" && !assertion.identity)
        {
            errors.push(error(format!("@{} needs #instinct", requirement.slug)));
        }
        if let Some(parent) = requirement.parent
            && !record.assertions.iter().any(|assertion| {
                assertion.predicate == "part-of" && assertion.object_slug.as_deref() == Some(parent)
            })
        {
            errors.push(error(format!(
                "@{} needs #part-of @{parent}",
                requirement.slug
            )));
        }
        if let Some(parent) = requirement.parent
            && slugs.get(parent).is_some_and(|record| {
                record
                    .assertions
                    .iter()
                    .any(|assertion| assertion.identity && assertion.predicate == "chapter")
            })
        {
            let order = record
                .assertions
                .iter()
                .find(|assertion| {
                    assertion.predicate == "part-of"
                        && assertion.object_slug.as_deref() == Some(parent)
                })
                .and_then(|assertion| assertion.quantity.as_deref())
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0);
            match order {
                Some(order) => {
                    if let Some(previous) = sibling_orders.insert((parent, order), requirement.slug)
                        && previous != requirement.slug
                    {
                        errors.push(error(format!(
                            "@{} and @{previous} have the same order in @{parent}",
                            requirement.slug
                        )));
                    }
                }
                None => errors.push(error(format!(
                    "@{} needs a positive whole-number chapter order in @{parent}",
                    requirement.slug
                ))),
            }
        }
    }
    for record in &records {
        for assertion in &record.assertions {
            if let Some(slug) = &assertion.object_slug
                && !slugs.contains_key(slug.as_str())
            {
                errors.push(error(format!("{} references missing @{slug}", record.head)));
            }
            if let Some(uid) = &assertion.object_uid
                && !uids.contains(uid)
            {
                errors.push(error(format!(
                    "{} references missing UID {uid}",
                    record.head
                )));
            }
            if assertion.predicate == "part-of"
                && assertion.quantity.as_deref().is_some_and(|amount| {
                    amount
                        .parse::<f64>()
                        .ok()
                        .is_none_or(|value| !value.is_finite() || value < 0.0)
                })
            {
                errors.push(error(format!("{} has invalid chapter order", record.head)));
            }
        }
        let mut seen = BTreeSet::new();
        let mut current = Some(record);
        while let Some(owner) = current {
            if !seen.insert(&owner.uid) {
                errors.push(error(format!("{} has a chapter cycle", record.head)));
                break;
            }
            let parents: Vec<_> = owner
                .assertions
                .iter()
                .filter(|assertion| assertion.predicate == "part-of")
                .collect();
            if parents.len() > 1 {
                errors.push(error(format!(
                    "{} has more than one chapter parent",
                    owner.head
                )));
                break;
            }
            current = parents
                .first()
                .and_then(|parent| parent.object_slug.as_deref())
                .and_then(|slug| slugs.get(slug))
                .copied();
        }
    }
    if errors.is_empty() {
        Ok(records)
    } else {
        Err(errors)
    }
}
