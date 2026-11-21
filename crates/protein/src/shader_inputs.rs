//! Projects requested numeric fields from readable Records for shader inputs.

use crate::{ProteinError, validation::protocol};
use nucleus::shader_effect::bindings::NumericField;
use protein_authority::query::shader_inputs;
pub use protein_authority::query::shader_inputs::Request;
use serde_json::{Map, Value, to_string};
use std::collections::{BTreeSet, HashMap};
use store::{
    Store,
    records::{ExtensionNumbers, RecordRow},
    sqlx::QueryBuilder,
};

/// Returns the JSON key used to identify a numeric field in a query result.
pub fn key(field: &NumericField) -> String {
    to_string(field).expect("numeric field keys contain only enum tags and strings")
}

/// Checks the request count and each Record UID and numeric field path.
///
/// # Errors
/// Returns a protocol error for an oversized batch or malformed request.
pub fn validate(requests: &[Request]) -> Result<(), ProteinError> {
    shader_inputs::validate(requests).map_err(protocol)
}

pub(crate) async fn attach<Body>(
    store: &Store,
    rows: &[RecordRow<Body>],
    requests: &[Request],
) -> Result<HashMap<String, Value>, ProteinError> {
    if requests.is_empty() {
        return Ok(HashMap::new());
    }
    validate(requests)?;
    let readable: HashMap<_, _> = rows.iter().map(|row| (row.uid.as_str(), row)).collect();
    let pairs: BTreeSet<_> = requests
        .iter()
        .filter(|request| readable.contains_key(request.record.as_str()))
        .filter_map(|request| match &request.field {
            NumericField::Extension { namespace, .. } => {
                Some((request.record.as_str(), namespace.as_str()))
            }
            NumericField::Quantity => None,
        })
        .collect();
    let raw_documents = if pairs.is_empty() {
        Vec::new()
    } else {
        let mut query = QueryBuilder::new(
            "SELECT record_uid, namespace, fds FROM record_extension WHERE length(CAST(fds AS BLOB)) <= 65536 AND (",
        );
        for (index, (record, namespace)) in pairs.iter().enumerate() {
            if index > 0 {
                query.push(" OR ");
            }
            query
                .push("(record_uid = ")
                .push_bind(*record)
                .push(" AND namespace = ")
                .push_bind(*namespace)
                .push(")");
        }
        query.push(")");
        query
            .build_query_as::<(String, String, String)>()
            .fetch_all(&store.pool)
            .await?
    };
    let mut documents: HashMap<&str, HashMap<&str, ExtensionNumbers<'_>>> = HashMap::new();
    for (uid, namespace, raw) in &raw_documents {
        documents
            .entry(uid)
            .or_default()
            .insert(namespace, ExtensionNumbers::new(raw));
    }
    let mut out: HashMap<String, Value> = HashMap::new();
    for request in requests {
        let Some(record) = readable.get(request.record.as_str()) else {
            continue;
        };
        let value = match &request.field {
            NumericField::Quantity => Some(record.quantity.to_string()),
            NumericField::Extension { namespace, path } => documents
                .get_mut(request.record.as_str())
                .and_then(|namespaces| namespaces.get_mut(namespace.as_str()))
                .and_then(|numbers| numbers.get(path).ok())
                .map(|number| number.to_string()),
        };
        let field = key(&request.field);
        let value = value.map_or(Value::Null, Value::String);
        if let Some(Value::Object(fields)) = out.get_mut(request.record.as_str()) {
            fields.insert(field, value);
        } else {
            out.insert(
                request.record.clone(),
                Value::Object(Map::from_iter([(field, value)])),
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_paths_preserve_exact_numbers_and_reject_non_numeric_values() {
        let path = vec!["daily".into(), "protein".into()];
        let raw = r#"{"daily":{"protein":9007199254740993,"note":"private"}}"#;
        assert_eq!(
            store::records::extension_numeric_at(raw, &path)
                .unwrap()
                .to_string(),
            "9007199254740993"
        );
        assert!(
            store::records::extension_numeric_at(raw, &["daily".into(), "note".into()]).is_err()
        );
        assert!(store::records::extension_numeric_at(raw, &[]).is_err());
        assert!(
            validate(&[Request {
                record: "a-slug".into(),
                field: NumericField::Quantity
            }])
            .is_err()
        );
        let request = Request {
            record: nucleus::new_uid("r"),
            field: NumericField::Quantity,
        };
        assert!(validate(&vec![request; 129]).is_err());
    }
}
