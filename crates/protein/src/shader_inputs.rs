use nucleus::shader_effect::bindings::{NumericBinding, NumericField, NumericSource};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use store::{Store, records::RecordRow};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub record: String,
    pub field: NumericField,
}

pub fn key(field: &NumericField) -> String {
    serde_json::to_string(field).unwrap()
}

pub fn validate(requests: &[Request]) -> Result<(), crate::ProteinError> {
    if requests.len() > 128 {
        return Err(store::StoreError::Protocol(
            "Numeric projections support at most 128 requested fields.".into(),
        ));
    }
    for request in requests {
        nucleus::shader_effect::bindings::validate(&[NumericBinding {
            slot: 0,
            label: "Reading".into(),
            source: NumericSource {
                organ: None,
                record: Some(request.record.clone()),
                field: request.field.clone(),
            },
        }])
        .map_err(store::StoreError::Protocol)?;
    }
    Ok(())
}

pub(crate) async fn attach(
    store: &Store,
    rows: &[RecordRow],
    requests: &[Request],
) -> Result<HashMap<String, Value>, crate::ProteinError> {
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
    let mut documents = HashMap::new();
    if !pairs.is_empty() {
        let mut query = store::sqlx::QueryBuilder::new(
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
        for (uid, namespace, raw) in query
            .build_query_as::<(String, String, String)>()
            .fetch_all(&store.pool)
            .await?
        {
            documents.insert((uid, namespace), raw);
        }
    }
    let mut out = HashMap::new();
    for request in requests {
        let Some(record) = readable.get(request.record.as_str()) else {
            continue;
        };
        let value = match &request.field {
            NumericField::Quantity => Some(record.quantity.to_string()),
            NumericField::Extension { namespace, path } => documents
                .get(&(request.record.clone(), namespace.clone()))
                .and_then(|raw| store::records::extension_numeric_at(raw, path).ok())
                .map(|number| number.to_string()),
        };
        let fields = out
            .entry(request.record.clone())
            .or_insert_with(|| Value::Object(Default::default()));
        fields[key(&request.field)] = value.map_or(Value::Null, Value::String);
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
