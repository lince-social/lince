use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftImage {
    pub(super) hash: String,
    pub(super) encoded: String,
}

pub(super) struct PreparedImage {
    pub(super) hash: String,
    pub(super) bytes: Vec<u8>,
    pub(super) dimensions: (u32, u32),
}

pub(super) fn omit_encoded_images(draft: &mut Value) {
    for image in draft["images"].as_array_mut().into_iter().flatten() {
        if let Some(image) = image.as_object_mut() {
            image.remove("encoded");
        }
    }
}

pub(super) async fn validate_images(
    fields: &ProfileFields,
    images: &[DraftImage],
) -> Result<Vec<PreparedImage>, EngineError> {
    if images.len() > 2 {
        return Err(invalid(
            "A profile draft can carry only its two selected images",
        ));
    }
    let mut prepared = Vec::new();
    let mut hashes = std::collections::HashSet::new();
    for image in images {
        if ![fields.avatar.as_deref(), fields.banner.as_deref()]
            .contains(&Some(image.hash.as_str()))
            || !hashes.insert(&image.hash)
            || image.encoded.len() > (128 * 1024 + 2) / 3 * 4
        {
            return Err(invalid(
                "The draft image is duplicated, unselected or oversized",
            ));
        }
        let bytes = B64
            .decode(&image.encoded)
            .map_err(|_| invalid("Invalid prepared draft image encoding"))?;
        let hash = image.hash.clone();
        prepared.push(
            media::image_work(move || {
                let dimensions = media::validate_public_image(&bytes, &hash)?;
                Ok(PreparedImage {
                    hash,
                    bytes,
                    dimensions,
                })
            })
            .await?,
        );
    }
    Ok(prepared)
}

impl Engine {
    pub(super) async fn social_profile_draft_images(
        &self,
        fields: &ProfileFields,
    ) -> Result<Vec<DraftImage>, EngineError> {
        let mut images = Vec::new();
        let mut hashes = std::collections::HashSet::new();
        for hash in [&fields.avatar, &fields.banner].into_iter().flatten() {
            if !hashes.insert(hash) {
                continue;
            }
            let bytes: Option<Vec<u8>> =
                store::sqlx::query_scalar("SELECT bytes FROM social_public_asset WHERE hash=?")
                    .bind(hash)
                    .fetch_optional(&self.store.pool)
                    .await?;
            if let Some(bytes) = bytes {
                if bytes.len() > 128 * 1024 {
                    return Err(invalid("The selected prepared image exceeds 128 KiB"));
                }
                images.push(DraftImage {
                    hash: hash.clone(),
                    encoded: B64.encode(bytes),
                });
            }
        }
        validate_images(fields, &images).await?;
        Ok(images)
    }
}
