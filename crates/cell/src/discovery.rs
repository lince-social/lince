use std::io::{Error, Result};
use store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Discovery {
    pub reach: engine::wire::Reach,
    pub local: bool,
}

pub async fn config(store: &Store, organ_uid: &str) -> Result<Option<serde_json::Value>> {
    if let Some(fields) = store::cells::config(&store.pool, "lince.discovery")
        .await
        .map_err(Error::other)?
    {
        return Ok(Some(fields));
    }
    store::records::get_extension(&store.pool, organ_uid, "lince.discovery")
        .await
        .map_err(Error::other)
}

fn settings(fields: Option<&serde_json::Value>, internet_allowed: bool) -> Result<Discovery> {
    if fields.is_some_and(|fields| !fields.is_object()) {
        return Err(Error::other("Discovery settings must be an object"));
    }
    let boolean = |name: &str, default: bool| -> Result<bool> {
        match fields.and_then(|fields| fields.get(name)) {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| Error::other(format!("Invalid discovery setting: {name}"))),
            None => Ok(default),
        }
    };
    let internet = boolean("internet", true)? && internet_allowed;
    let direct = boolean("direct", false)?;
    let mut local = boolean("local", false)?;
    if let Some(until) = fields.and_then(|fields| fields.get("local_until")) {
        let until = until
            .as_str()
            .ok_or_else(|| Error::other("Invalid local discovery expiry"))?;
        let until = chrono::DateTime::parse_from_rfc3339(until).map_err(Error::other)?;
        local &= until.with_timezone(&chrono::Utc) > chrono::Utc::now();
    }
    Ok(Discovery {
        reach: if !internet {
            engine::wire::Reach::Local
        } else if direct {
            engine::wire::Reach::Internet
        } else {
            engine::wire::Reach::Relay
        },
        local,
    })
}

pub async fn for_organ(store: &Store, organ_uid: &str) -> Result<Discovery> {
    let fields = config(store, organ_uid).await?;
    let internet = !matches!(
        std::env::var("LINCE_DISCOVERY_INTERNET").as_deref(),
        Ok("0") | Ok("false") | Ok("no")
    );
    settings(fields.as_ref(), internet)
}

pub async fn of(store: &Store) -> Result<Discovery> {
    let organ = store::organs::local(&store.pool)
        .await
        .map_err(Error::other)?
        .ok_or_else(|| {
            Error::other("Cannot read discovery settings: the local Organ is missing")
        })?;
    for_organ(store, &organ.uid).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invalid_discovery_settings_never_enable_network_access() {
        assert_eq!(
            settings(None, false).unwrap().reach,
            engine::wire::Reach::Local
        );
        assert_eq!(
            settings(Some(&json!({"internet": false})), true)
                .unwrap()
                .reach,
            engine::wire::Reach::Local
        );
        for fields in [
            json!({"internet": "false"}),
            json!({"local_until": "invalid"}),
            json!({"local": 1}),
            json!([]),
        ] {
            assert!(settings(Some(&fields), true).is_err());
        }
    }

    #[tokio::test]
    async fn database_failure_and_missing_organ_are_errors() {
        let store = Store::open_memory().await.unwrap();
        assert!(of(&store).await.is_ok());
        let organ = store::organs::local(&store.pool).await.unwrap().unwrap();
        store::sqlx::query("UPDATE record SET slug = NULL WHERE uid = ?")
            .bind(&organ.uid)
            .execute(&store.pool)
            .await
            .unwrap();
        assert!(of(&store).await.is_err());
        store.pool.close().await;
        assert!(config(&store, "missing").await.is_err());
    }
}
