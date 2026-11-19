use crate::{schema::*, settings::Settings};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub struct Nomad {
    client: Client,
    settings: Settings,
}

impl Nomad {
    pub fn new(settings: Settings) -> Result<Self, String> {
        settings.validate()?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(12))
            .user_agent("Lince-NOMAD/0.1")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client, settings })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        // Only configured NOMAD endpoints are requested. Catalog download URLs
        // and paths returned by the server are source data, never fetch targets.
        let mut response = self
            .client
            .get(self.settings.url(path)?)
            .send()
            .await
            .map_err(|e| format!("NOMAD {path}: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("NOMAD {path}: HTTP {}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(format!("NOMAD {path}: response exceeds 4 MiB"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("NOMAD {path}: {e}"))?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(format!("NOMAD {path}: response exceeds 4 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("NOMAD {path}: invalid response: {e}"))
    }

    async fn optional<T: DeserializeOwned + Default>(
        &self,
        enabled: bool,
        path: &str,
    ) -> Result<T, String> {
        if enabled {
            self.get(path).await
        } else {
            Ok(T::default())
        }
    }

    pub async fn snapshot(&self) -> Result<Snapshot, String> {
        // Concurrent reads keep the entire request below the plugin host's 45s
        // timeout even when an optional backend is unavailable. A failed enabled
        // source aborts the snapshot; it never erases the previous export.
        let (categories, archives, map_collections, maps, documents, wikipedia) = tokio::try_join!(
            self.get::<Categories>("api/zim/curated-categories"),
            self.get::<Files>("api/zim/list"),
            async {
                if self.settings.include_maps {
                    self.get::<Collections>("api/maps/curated-collections")
                        .await
                        .map(Collections::into_vec)
                } else {
                    Ok(Vec::new())
                }
            },
            self.optional::<Files>(self.settings.include_maps, "api/maps/regions"),
            self.optional::<Documents>(self.settings.include_documents, "api/rag/files"),
            self.optional::<Wikipedia>(self.settings.include_wikipedia, "api/zim/wikipedia"),
        )?;
        if archives.next.is_some() || maps.next.is_some() {
            return Err(
                "This NOMAD paginates file listings; refusing an incomplete snapshot".into(),
            );
        }
        Ok(Snapshot {
            categories: categories.into_vec(),
            archives: archives.files,
            map_collections,
            maps: maps.files,
            documents: documents.files,
            wikipedia: wikipedia.options,
        })
    }
}
