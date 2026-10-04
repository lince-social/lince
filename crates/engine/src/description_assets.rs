use crate::{Engine, error::EngineError};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use nucleus::description_asset::{Kind, MAX_BYTES, Request, Response};
use sha2::{Digest, Sha256};

pub fn description_is_locked(body: &str) -> bool {
    utils::vault::is_locked(body)
}

impl Engine {
    pub async fn description_asset(
        &self,
        actor: Option<&str>,
        request: Request,
    ) -> Result<Response, EngineError> {
        let write = matches!(request, Request::Put { .. });
        self.access_scope(write, self.description_asset_inner(actor, request)).await
    }

    async fn description_asset_inner(
        &self,
        actor: Option<&str>,
        request: Request,
    ) -> Result<Response, EngineError> {
        let record = match &request {
            Request::Put { record, .. } | Request::Get { record, .. } => record,
        };
        if !nucleus::valid_uid(record, "r") || !self.may_read_record(actor, record).await? {
            return Err(EngineError::Forbidden(
                "Description asset is unavailable.".into(),
            ));
        }
        let query: protein::Protein = serde_json::from_value(serde_json::json!({
            "source":"record", "where":[{"uid_eq":record}], "fields":["uid","body"], "limit":1
        }))
        .map_err(|error| EngineError::Consequence(error.to_string()))?;
        let rows = protein::execute_for(&self.store, &query, actor).await?;
        if rows
            .first()
            .and_then(|row| row["body"].as_str())
            .is_none_or(utils::vault::is_locked)
        {
            return Err(EngineError::Forbidden(
                "Description asset is unavailable.".into(),
            ));
        }
        match request {
            Request::Put {
                record,
                kind,
                data_base64,
            } => {
                self.require_permission(actor, "record:update").await?;
                self.authorize_action(&crate::actions::Action::EditRecordText { target: record.clone(), head: None, body: None }, actor).await?;
                if !self
                    .record_text_permissions(actor, &record)
                    .await?
                    .contains(&protein::authority::Property::Body)
                {
                    return Err(EngineError::Forbidden("Description is read only.".into()));
                }
                if data_base64.len() > MAX_BYTES.div_ceil(3) * 4 {
                    return Err(EngineError::Consequence("Asset exceeds 4 MiB.".into()));
                }
                let bytes = STANDARD
                    .decode(data_base64)
                    .map_err(|_| EngineError::Consequence("Invalid asset encoding.".into()))?;
                validate(kind, &bytes).map_err(EngineError::Consequence)?;
                let asset = Sha256::digest([kind.name().as_bytes(), bytes.as_slice()].concat())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
                self.require_permission_on(&mut tx, actor, "record:update").await?;
                let checkpoint = self.record_checkpoint_on(&mut tx, actor, vec![record.clone()], protein::authority::Operation::Update).await?;
                let used: i64 = store::sqlx::query_scalar("SELECT coalesce(sum(length(bytes)),0) FROM description_assets WHERE record_uid = ?")
                    .bind(&record).fetch_one(&mut *tx).await?;
                let exists: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM description_assets WHERE record_uid = ? AND asset = ?)")
                    .bind(&record).bind(&asset).fetch_one(&mut *tx).await?;
                if !exists && used + bytes.len() as i64 > 64 * 1024 * 1024 {
                    return Err(EngineError::Consequence(
                        "This Record's assets exceed 64 MiB.".into(),
                    ));
                }
                store::sqlx::query("INSERT OR IGNORE INTO description_assets(record_uid,asset,kind,bytes) VALUES(?,?,?,?)")
                    .bind(&record).bind(&asset).bind(kind.name()).bind(bytes).execute(&mut *tx).await?;
                if let Some(checkpoint) = checkpoint { checkpoint.finish(&mut tx, std::collections::BTreeSet::from([protein::authority::Property::Body])).await?; }
                tx.commit().await?;
                Ok(Response::Stored { asset })
            }
            Request::Get { record, asset } => {
                if nucleus::description_asset::parse_reference(
                    &nucleus::description_asset::reference(&record, &asset),
                )
                .is_none()
                {
                    return Err(EngineError::Consequence("Invalid asset reference.".into()));
                }
                let row: Option<(String, Vec<u8>)> = store::sqlx::query_as(
                    "SELECT kind, bytes FROM description_assets WHERE record_uid = ? AND asset = ?",
                )
                .bind(record)
                .bind(asset)
                .fetch_optional(&self.store.pool)
                .await?;
                let (kind, bytes) = row.ok_or_else(|| {
                    EngineError::Consequence("Description asset is unavailable.".into())
                })?;
                let kind = match kind.as_str() {
                    "drawing" => Kind::Drawing,
                    "png" => Kind::Png,
                    "webp" => Kind::Webp,
                    _ => return Err(EngineError::Consequence("Unknown asset format.".into())),
                };
                Ok(Response::Data {
                    kind,
                    data_base64: STANDARD.encode(bytes),
                })
            }
        }
    }
}

fn validate(kind: Kind, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("Asset exceeds its size limit.".into());
    }
    if kind == Kind::Drawing {
        let drawing: nucleus::drawing::Drawing =
            serde_json::from_slice(bytes).map_err(|_| "Invalid native drawing.".to_string())?;
        return drawing.validate();
    }
    let expected = if kind == Kind::Png {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::WebP
    };
    if image::guess_format(bytes).ok() != Some(expected) {
        return Err("Asset format does not match its contents.".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|error| error.to_string())?;
    Ok(())
}
