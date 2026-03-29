use {
    crate::domain::lince_package::{LincePackage, parse_lince_package},
    reqwest::Client,
    serde::Deserialize,
    std::{
        collections::BTreeMap,
        sync::Arc,
        time::{Duration, Instant},
    },
    tokio::sync::RwLock,
};

const DNA_RAW_BASE_URL: &str = "https://raw.githubusercontent.com/lince-social/dna/main";
const CATALOG_TTL: Duration = Duration::from_secs(5 * 60);
const PREVIEW_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct DnaHubStore {
    http: Client,
    catalog_cache: Arc<RwLock<Option<TimedValue<DnaSandCatalog>>>>,
    preview_cache: Arc<RwLock<BTreeMap<String, TimedValue<LincePackage>>>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DnaSandCatalog {
    #[serde(default)]
    pub packages: BTreeMap<String, DnaSandCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnaSandCatalogEntry {
    pub title: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct DnaSandSearchMatch {
    pub package_name: String,
    pub title: String,
    pub description: String,
    pub path: String,
    pub channel: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteSandToml {
    name: String,
    channel: String,
}

#[derive(Debug, Clone)]
struct TimedValue<T> {
    value: T,
    fetched_at: Instant,
}

impl<T: Clone> TimedValue<T> {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed() < ttl
    }

    fn cloned_if_fresh(&self, ttl: Duration) -> Option<T> {
        self.is_fresh(ttl).then(|| self.value.clone())
    }
}

impl DnaHubStore {
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = Client::builder().user_agent("lince-web/0.1").build()?;
        Ok(Self {
            http,
            catalog_cache: Arc::new(RwLock::new(None)),
            preview_cache: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    pub async fn catalog(&self) -> Result<DnaSandCatalog, String> {
        if let Some(cached) = self
            .catalog_cache
            .read()
            .await
            .as_ref()
            .and_then(|value| value.cloned_if_fresh(CATALOG_TTL))
        {
            return Ok(cached);
        }

        let raw = self
            .fetch_text("sand/catalog.toml")
            .await
            .map_err(|_| "Falha ao buscar o catalogo do hub.".to_string())?;
        let catalog = toml::from_str::<DnaSandCatalog>(&raw)
            .map_err(|_| "O catalogo remoto de widgets e invalido.".to_string())?;
        let mut cache = self.catalog_cache.write().await;
        *cache = Some(TimedValue {
            value: catalog.clone(),
            fetched_at: Instant::now(),
        });
        Ok(catalog)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<DnaSandSearchMatch>, String> {
        let normalized = normalize_search_query(query);
        if normalized.chars().count() < 2 {
            return Ok(Vec::new());
        }

        let catalog = self.catalog().await?;
        let mut matches = Vec::new();

        for (package_name, entry) in &catalog.packages {
            let Some(score) = search_match_score(package_name, entry, &normalized) else {
                continue;
            };
            let (channel, catalog_name) = parse_catalog_path(&entry.path)?;
            if catalog_name != package_name {
                return Err("O catalogo remoto de widgets e invalido.".to_string());
            }
            matches.push((score, DnaSandSearchMatch {
                package_name: package_name.clone(),
                title: entry.title.clone(),
                description: entry.description.clone(),
                path: entry.path.clone(),
                channel: channel.to_string(),
            }));
        }

        matches.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.title.to_lowercase().cmp(&right.1.title.to_lowercase()))
                .then_with(|| left.1.package_name.cmp(&right.1.package_name))
        });

        Ok(matches.into_iter().map(|(_, item)| item).collect())
    }

    pub async fn preview_package(
        &self,
        channel: &str,
        package_name: &str,
    ) -> Result<LincePackage, String> {
        let normalized_name = validate_package_name(package_name)?;
        let normalized_channel = validate_channel(channel)?;
        let cache_key = format!("{normalized_channel}/{normalized_name}");

        if let Some(cached) = self
            .preview_cache
            .read()
            .await
            .get(&cache_key)
            .and_then(|value| value.cloned_if_fresh(PREVIEW_TTL))
        {
            return Ok(cached);
        }

        let prefix = package_prefix(&normalized_name);
        let base = format!("sand/{normalized_channel}/{prefix}/{normalized_name}");
        let sand_toml = self.fetch_text(&format!("{base}/sand.toml")).await?;
        let sand = toml::from_str::<RemoteSandToml>(&sand_toml)
            .map_err(|_| "O pacote remoto nao possui metadados validos para instalacao.".to_string())?;
        if sand.name != normalized_name || sand.channel != normalized_channel {
            return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
        }

        let html = self
            .fetch_text(&format!("{base}/{normalized_name}_metadata.html"))
            .await?;
        let filename = format!("{normalized_name}.html");
        let package = parse_lince_package(filename, html.as_bytes())
            .map_err(|_| "O pacote remoto nao possui metadados validos para instalacao.".to_string())?;

        let mut cache = self.preview_cache.write().await;
        cache.insert(
            cache_key,
            TimedValue {
                value: package.clone(),
                fetched_at: Instant::now(),
            },
        );
        Ok(package)
    }

    async fn fetch_text(&self, path: &str) -> Result<String, String> {
        let url = format!("{DNA_RAW_BASE_URL}/{path}");
        let response = self.http.get(url).send().await.map_err(|error| {
            tracing::warn!("dna hub request failed for {path}: {error}");
            "Falha ao baixar o pacote selecionado.".to_string()
        })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err("Falha ao baixar o pacote selecionado.".to_string());
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("dna hub rejected {path} with {status}: {body}");
            return Err("Falha ao baixar o pacote selecionado.".to_string());
        }

        response.text().await.map_err(|error| {
            tracing::warn!("dna hub body invalid for {path}: {error}");
            "Falha ao baixar o pacote selecionado.".to_string()
        })
    }
}

fn validate_channel(value: &str) -> Result<String, String> {
    match value.trim() {
        "official" | "community" => Ok(value.trim().to_string()),
        _ => Err("O pacote remoto nao possui metadados validos para instalacao.".to_string()),
    }
}

fn validate_package_name(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 200 {
        return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
    }

    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
    };
    if !first.is_ascii_lowercase() {
        return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
    }

    let mut previous_was_underscore = false;
    for ch in chars {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_';
        if !valid || (ch == '_' && previous_was_underscore) {
            return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
        }
        previous_was_underscore = ch == '_';
    }

    if normalized.ends_with('_') {
        return Err("O pacote remoto nao possui metadados validos para instalacao.".to_string());
    }

    Ok(normalized)
}

fn normalize_search_query(query: &str) -> String {
    let mut out = String::new();
    let mut previous_was_separator = false;

    for ch in query.trim().chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            '_' | '-' | ' ' => Some('_'),
            _ => None,
        };
        let Some(normalized) = normalized else {
            continue;
        };

        if normalized == '_' {
            if out.is_empty() || previous_was_separator {
                continue;
            }
            previous_was_separator = true;
        } else {
            previous_was_separator = false;
        }
        out.push(normalized);
    }

    out.trim_matches('_').to_string()
}

fn search_match_score(
    package_name: &str,
    entry: &DnaSandCatalogEntry,
    query: &str,
) -> Option<u8> {
    let package_name_lower = package_name.to_ascii_lowercase();
    if package_name_lower.starts_with(query) {
        return Some(0);
    }
    if package_name_lower.contains(query) {
        return Some(1);
    }

    let title = entry.title.to_ascii_lowercase();
    if title.contains(query) {
        return Some(2);
    }

    let description = entry.description.to_ascii_lowercase();
    if description.contains(query) {
        return Some(3);
    }

    None
}

fn parse_catalog_path(path: &str) -> Result<(&str, &str), String> {
    let mut parts = path.split('/');
    let Some(channel) = parts.next() else {
        return Err("O catalogo remoto de widgets e invalido.".to_string());
    };
    let Some(_prefix) = parts.next() else {
        return Err("O catalogo remoto de widgets e invalido.".to_string());
    };
    let Some(package_name) = parts.next() else {
        return Err("O catalogo remoto de widgets e invalido.".to_string());
    };
    if parts.next().is_some() || !matches!(channel, "official" | "community") {
        return Err("O catalogo remoto de widgets e invalido.".to_string());
    }
    Ok((channel, package_name))
}

fn package_prefix(name: &str) -> String {
    name.chars().take(2).collect()
}
