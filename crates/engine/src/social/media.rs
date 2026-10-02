use super::*;
use std::io::{Cursor, Read};

const MAX_IMAGE_BYTES: usize = 128 * 1024;

pub(super) async fn image_work<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, EngineError> + Send + 'static,
) -> Result<T, EngineError> {
    static DECODERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    let permit = DECODERS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| invalid("Image processing is busy; try again shortly"))?;
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), task)
        .await
        .map_err(|_| invalid("Image processing exceeded the time limit"))?
        .map_err(|_| invalid("Image processing stopped"))?
}

fn decode(bytes: &[u8]) -> Result<image::DynamicImage, EngineError> {
    let format = image::guess_format(bytes).map_err(|_| invalid("Choose a PNG or JPEG image"))?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err(invalid("Choose a PNG or JPEG image"));
    }
    let dimensions = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| invalid("Invalid image dimensions"))?;
    if dimensions.0 == 0
        || dimensions.1 == 0
        || dimensions.0 > 2048
        || dimensions.1 > 2048
        || u64::from(dimensions.0) * u64::from(dimensions.1) > 4_000_000
    {
        return Err(invalid(
            "Choose an image up to 2048×2048 and four million pixels",
        ));
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| invalid("The image is invalid or exceeds decoder limits"))
}

pub(super) fn validate_public_image(bytes: &[u8], hash: &str) -> Result<(u32, u32), EngineError> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES || nucleus::fact::sha256_hex(bytes) != hash
    {
        return Err(invalid(
            "The public image is oversized or its content hash differs",
        ));
    }
    let image = decode(bytes)?;
    Ok((image.width(), image.height()))
}

fn normalize(path: &str) -> Result<Vec<u8>, EngineError> {
    let file =
        std::fs::File::open(path).map_err(|_| invalid("Cannot read the selected local image"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("Cannot inspect the selected local image"))?
        .is_file()
    {
        return Err(invalid("Choose a regular local image file"));
    }
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Cannot read the selected local image"))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("Choose an image file up to 4 MiB"));
    }
    normalize_bytes(&bytes)
}

fn normalize_bytes(bytes: &[u8]) -> Result<Vec<u8>, EngineError> {
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("Choose an image file up to 4 MiB"));
    }
    let image = decode(bytes)?;
    let image = if image.width() > 1024 || image.height() > 512 {
        image.thumbnail(1024, 512)
    } else {
        image
    }
    .to_rgb8();
    for quality in [80, 65, 45] {
        let mut output = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality)
            .encode_image(&image)
            .map_err(|_| invalid("Cannot prepare the public image"))?;
        if output.len() <= MAX_IMAGE_BYTES {
            return Ok(output);
        }
    }
    let image = image::DynamicImage::ImageRgb8(image)
        .thumbnail(512, 256)
        .to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 60)
        .encode_image(&image)
        .map_err(|_| invalid("Cannot prepare the public image"))?;
    if output.len() > MAX_IMAGE_BYTES {
        return Err(invalid("The normalized image exceeds 128 KiB"));
    }
    Ok(output)
}

impl Engine {
    pub(super) async fn social_import_image_data(
        &self,
        encoded: String,
    ) -> Result<Value, EngineError> {
        if encoded.len() > 5_592_408 {
            return Err(invalid("Choose an image file up to 4 MiB"));
        }
        let bytes = B64
            .decode(encoded)
            .map_err(|_| invalid("The selected image upload is malformed"))?;
        let bytes = image_work(move || normalize_bytes(&bytes)).await?;
        self.social_prepare_image_asset(bytes).await
    }

    async fn social_prepare_image_asset(&self, bytes: Vec<u8>) -> Result<Value, EngineError> {
        let hash = nucleus::fact::sha256_hex(&bytes);
        let dimensions = validate_public_image(&bytes, &hash)?;
        let mut tx = self.social_write_tx().await?;
        Self::social_store_asset(&mut tx, &hash, &bytes, dimensions, 64 * 1024 * 1024).await?;
        tx.commit().await?;
        Ok(
            json!({"asset_hash":hash,"width":dimensions.0,"height":dimensions.1,"status":"Public image prepared without source metadata. Add this hash to your profile and review before saving"}),
        )
    }

    pub(super) async fn social_enqueue_images(
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        profile: &Profile,
    ) -> Result<(), EngineError> {
        if profile.state != PostState::Active {
            return Ok(());
        }
        let mut hashes = std::collections::HashSet::new();
        for hash in [&profile.fields.avatar, &profile.fields.banner]
            .into_iter()
            .flatten()
        {
            if !hashes.insert(hash) {
                continue;
            }
            let bytes: Option<Vec<u8>> =
                store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
                    .bind(hash)
                    .fetch_optional(&mut **tx)
                    .await?;
            if let Some(bytes) = bytes {
                let document = PublicImage {
                    profile: profile.clone(),
                    hash: hash.clone(),
                    encoded: B64.encode(bytes),
                };
                let job_hash = document_hash("profile-image", &document)?;
                store::social::enqueue_on(
                    tx,
                    "image",
                    &job_hash,
                    &serde_json::to_string(&document)?,
                    &profile.destinations,
                    profile.expires_at,
                )
                .await?;
            }
        }
        Ok(())
    }

    pub(super) async fn social_load_image(
        &self,
        organ: &str,
        hash: &str,
        services: &[String],
    ) -> Result<Value, EngineError> {
        if services.len() > 8 || hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(
                "Select an image hash and at most eight profile hosts",
            ));
        }
        let body: String = store::sqlx::query_scalar("SELECT body FROM social_document WHERE kind='profile' AND id=? AND state='active' AND expires_at>?")
            .bind(organ).bind(nucleus::execution::now().timestamp()).fetch_optional(&self.store.pool).await?
            .ok_or_else(|| invalid("Load the current public profile before choosing its image"))?;
        let doc: Profile = serde_json::from_str(&body)?;
        if ![doc.fields.avatar.as_deref(), doc.fields.banner.as_deref()].contains(&Some(hash)) {
            return Err(invalid(
                "The selected image is not part of this public profile",
            ));
        }
        let held: Option<Vec<u8>> =
            store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
                .bind(hash)
                .fetch_optional(&self.store.pool)
                .await?;
        if let Some(bytes) = held {
            return Ok(
                json!({"image_hash":hash,"encoded":B64.encode(bytes),"source":"verified local public image cache"}),
            );
        }
        let network = self.social_network()?;
        let mut failures = Vec::new();
        for service in services {
            if !doc.destinations.contains(service) {
                return Err(invalid("Choose a host selected by this public profile"));
            }
            let reply = network
                .request(
                    service,
                    PublicRequest::FetchProfileImage {
                        organ: organ.into(),
                        hash: hash.into(),
                    },
                )
                .await;
            if let Ok(reply) = reply {
                if reply["hash"] != hash || reply["service"] != *service {
                    failures.push(service.clone());
                    continue;
                }
                let encoded = reply["encoded"].as_str().unwrap_or_default();
                if encoded.len() > (MAX_IMAGE_BYTES + 2) / 3 * 4 {
                    failures.push(service.clone());
                    continue;
                }
                let Ok(bytes) = B64.decode(encoded) else {
                    failures.push(service.clone());
                    continue;
                };
                let checked = bytes.clone();
                let expected = hash.to_owned();
                let dimensions =
                    image_work(move || validate_public_image(&checked, &expected)).await?;
                let mut tx = self.social_write_tx().await?;
                Self::social_store_asset(&mut tx, hash, &bytes, dimensions, 64 * 1024 * 1024)
                    .await?;
                tx.commit().await?;
                return Ok(
                    json!({"image_hash":hash,"encoded":encoded,"source":service,"failures":failures}),
                );
            }
            failures.push(service.clone());
        }
        Err(invalid(
            "The selected image is unavailable from these hosts. The text profile remains available",
        ))
    }

    pub(super) async fn social_import_image(
        &self,
        path: String,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        if actor.is_some() {
            return Err(EngineError::Forbidden(
                "Local image files can only be selected on this device's interface".into(),
            ));
        }
        if path.is_empty() || path.len() > 4096 {
            return Err(invalid("Choose a valid local image path"));
        }
        let bytes = image_work(move || normalize(&path)).await?;
        self.social_prepare_image_asset(bytes).await
    }

    pub(super) async fn social_store_asset(
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        hash: &str,
        bytes: &[u8],
        dimensions: (u32, u32),
        budget: u64,
    ) -> Result<(), EngineError> {
        if store::sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM social_public_asset WHERE hash=?)",
        )
        .bind(hash)
        .fetch_one(&mut **tx)
        .await?
        {
            return Ok(());
        }
        let used: i64 = store::sqlx::query_scalar(
            "SELECT COALESCE(SUM(length(bytes)),0) FROM social_public_asset",
        )
        .fetch_one(&mut **tx)
        .await?;
        if (used as u64).saturating_add(bytes.len() as u64) > budget {
            return Err(invalid(
                "Public image storage is full; remove unused images before importing another",
            ));
        }
        store::sqlx::query(
            "INSERT INTO social_public_asset(hash,bytes,width,height,touched_at) VALUES(?,?,?,?,?)",
        )
        .bind(hash)
        .bind(bytes)
        .bind(dimensions.0)
        .bind(dimensions.1)
        .bind(nucleus::execution::now().timestamp())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub(super) async fn social_publish_asset(
        &self,
        node: &str,
        doc: Profile,
        hash: String,
        encoded: String,
        now: i64,
    ) -> Result<Value, EngineError> {
        profile::validate_profile(&doc, now)?;
        if doc.state != PostState::Active
            || !doc.destinations.iter().any(|id| id == node)
            || ![doc.fields.avatar.as_ref(), doc.fields.banner.as_ref()].contains(&Some(&hash))
        {
            return Err(invalid(
                "This image is not selected for the signed profile on this host",
            ));
        }
        if encoded.len() > (MAX_IMAGE_BYTES + 2) / 3 * 4 {
            return Err(invalid("The public image is too large"));
        }
        let receipt_hash = document_hash(
            "profile-image",
            &PublicImage {
                profile: doc.clone(),
                hash: hash.clone(),
                encoded: encoded.clone(),
            },
        )?;
        let bytes = B64
            .decode(encoded)
            .map_err(|_| invalid("Malformed public image"))?;
        let checked_hash = hash.clone();
        let checked_bytes = bytes.clone();
        let dimensions =
            image_work(move || validate_public_image(&checked_bytes, &checked_hash)).await?;
        let settings = self.social_settings().await?;
        let mut tx = self.social_write_tx().await?;
        store::social::anchor_profile_authority_on(&mut tx, &doc.authority, false).await?;
        Self::social_store_asset(
            &mut tx,
            &hash,
            &bytes,
            dimensions,
            settings.storage_bytes / 8,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"accepted":true,"hash":receipt_hash,"asset_hash":hash,"service":node,"expires_at":doc.expires_at}),
        )
    }

    pub(super) async fn social_fetch_asset(
        &self,
        organ: &str,
        hash: &str,
        node: &str,
        now: i64,
    ) -> Result<Value, EngineError> {
        let body: String = store::sqlx::query_scalar("SELECT body FROM social_document WHERE kind='profile' AND id=? AND expires_at>? AND state='active'")
            .bind(organ).bind(now).fetch_optional(&self.store.pool).await?
            .ok_or_else(|| invalid("No current hosted profile references this image"))?;
        let doc: Profile = serde_json::from_str(&body)?;
        if !doc.destinations.iter().any(|id| id == node)
            || ![doc.fields.avatar.as_deref(), doc.fields.banner.as_deref()].contains(&Some(hash))
        {
            return Err(invalid(
                "This public profile does not host the requested image",
            ));
        }
        let bytes: Vec<u8> =
            store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
                .bind(hash)
                .fetch_optional(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("The public image has not reached this host"))?;
        Ok(json!({"hash":hash,"encoded":B64.encode(bytes),"service":node}))
    }
}
