use crate::Coordinate;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub places: PathBuf,
    pub geocoder: String,
    pub router: String,
    pub credits: String,
    pub online: bool,
    pub minimum_interval_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            places: PathBuf::new(),
            geocoder: String::new(),
            router: String::new(),
            credits: String::new(),
            online: false,
            minimum_interval_ms: 1000,
        }
    }
}

impl Settings {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(65_537)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 65_536 {
            return Err("Provider settings exceed 64 KiB".into());
        }
        let value: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn save_atomic(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        crate::source::save_json_atomic(self, path)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1000..=60000).contains(&self.minimum_interval_ms) {
            return Err("Choose a provider interval between 1000 and 60000 milliseconds".into());
        }
        for endpoint in [&self.geocoder, &self.router] {
            if endpoint.is_empty() {
                continue;
            }
            let url =
                reqwest::Url::parse(endpoint).map_err(|_| "Enter an absolute provider URL")?;
            let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
            if !(url.scheme() == "https" || url.scheme() == "http" && local)
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "Use HTTPS, or HTTP on localhost, without credentials, query or fragment"
                        .into(),
                );
            }
        }
        if self.credits.len() > 4096 {
            return Err("Keep provider credits within 4096 bytes".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub name: String,
    pub coordinate: Coordinate,
    pub credits: String,
}
impl Place {
    pub fn validate(&self) -> Result<(), String> {
        if !self.coordinate.valid()
            || self.name.trim().is_empty()
            || self.name.len() > 4096
            || self.credits.len() > 4096
        {
            return Err("Provider returned an invalid place".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Driving,
    Cycling,
    Walking,
}
impl Mode {
    fn profile(self) -> &'static str {
        match self {
            Self::Driving => "driving",
            Self::Cycling => "cycling",
            Self::Walking => "walking",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub points: Vec<Coordinate>,
    pub distance_metres: f64,
    pub duration_seconds: f64,
    pub calculated_at: i64,
    pub mode: Mode,
    pub credits: String,
}

impl Route {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=32768).contains(&self.points.len())
            || self.points.iter().any(|point| !point.valid())
            || !self.distance_metres.is_finite()
            || self.distance_metres < 0.0
            || !self.duration_seconds.is_finite()
            || self.duration_seconds < 0.0
            || self.calculated_at <= 0
            || self.credits.len() > 4096
        {
            return Err("Route geometry, estimate or provenance is invalid".into());
        }
        Ok(())
    }
}

#[async_trait]
pub trait Geocoder: Send + Sync {
    async fn search(&self, query: &str) -> Result<Vec<Place>, String>;
    async fn reverse(&self, point: Coordinate) -> Result<Place, String>;
}
#[async_trait]
pub trait Router: Send + Sync {
    async fn route(
        &self,
        points: &[Coordinate],
        mode: Mode,
        calculated_at: i64,
    ) -> Result<Route, String>;
}

pub struct Provider {
    settings: Settings,
    client: reqwest::Client,
    next_request: tokio::sync::Mutex<Option<Instant>>,
}
impl Provider {
    pub fn new(settings: Settings) -> Result<Self, String> {
        settings.validate()?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Lince/0.8 map navigation")
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            settings,
            client,
            next_request: tokio::sync::Mutex::new(None),
        })
    }
    async fn json(&self, url: reqwest::Url) -> Result<Value, String> {
        if !self.settings.online {
            return Err("Enable provider requests explicitly in Map Settings".into());
        }
        {
            let mut next = self.next_request.lock().await;
            if next.is_some_and(|next| next > Instant::now()) {
                return Err(
                    "Wait for the configured interval before another provider request".into(),
                );
            }
            *next = Some(Instant::now() + Duration::from_millis(self.settings.minimum_interval_ms));
        }
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "Map provider unavailable")?
            .error_for_status()
            .map_err(|_| "Map provider refused the request")?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Map provider response interrupted")?
        {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err("Map provider response exceeds 4 MiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Map provider returned invalid JSON".into())
    }
    async fn local_places(&self) -> Result<Vec<Place>, String> {
        let file = tokio::fs::File::open(&self.settings.places)
            .await
            .map_err(|_| "Local place index unavailable")?;
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Cannot read local place index")?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Local place index exceeds 4 MiB".into());
        }
        let places: Vec<Place> =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid local place index")?;
        for place in &places {
            place.validate()?;
        }
        Ok(places)
    }
    fn endpoint(&self, base: &str, path: &str) -> Result<reqwest::Url, String> {
        if base.is_empty() {
            return Err("Configure this provider in Map Settings".into());
        }
        reqwest::Url::parse(&format!("{}/{path}", base.trim_end_matches('/')))
            .map_err(|_| "Invalid provider URL".into())
    }
    fn place(&self, raw: &Value) -> Result<Place, String> {
        let number = |name| {
            raw[name]
                .as_str()
                .ok_or("Provider omitted a coordinate")?
                .parse::<f64>()
                .map_err(|_| "Invalid provider coordinate")
        };
        let place = Place {
            name: raw["display_name"]
                .as_str()
                .ok_or("Provider omitted the place name")?
                .into(),
            coordinate: Coordinate {
                latitude: number("lat")?,
                longitude: number("lon")?,
            },
            credits: self.settings.credits.clone(),
        };
        place.validate()?;
        Ok(place)
    }
}
#[async_trait]
impl Geocoder for Provider {
    async fn search(&self, query: &str) -> Result<Vec<Place>, String> {
        if query.trim().is_empty() || query.len() > 512 {
            return Err("Enter a place name of 1–512 bytes".into());
        }
        if !self.settings.places.as_os_str().is_empty() {
            let mut places = self.local_places().await?;
            places.retain(|place| place.name.to_lowercase().contains(&query.to_lowercase()));
            places.truncate(10);
            if !places.is_empty() || self.settings.geocoder.is_empty() {
                return Ok(places);
            }
        }
        let mut url = self.endpoint(&self.settings.geocoder, "search")?;
        url.query_pairs_mut()
            .append_pair("format", "jsonv2")
            .append_pair("limit", "10")
            .append_pair("q", query);
        let raw = self.json(url).await?;
        raw.as_array()
            .ok_or("Invalid place search response")?
            .iter()
            .take(10)
            .map(|place| self.place(place))
            .collect()
    }
    async fn reverse(&self, point: Coordinate) -> Result<Place, String> {
        if !point.valid() {
            return Err("Select a visible map point".into());
        }
        if !self.settings.places.as_os_str().is_empty() {
            let places = self.local_places().await?;
            let distance = |place: &Place| {
                let lat = (place.coordinate.latitude - point.latitude).to_radians() / 2.0;
                let lon = (place.coordinate.longitude - point.longitude).to_radians() / 2.0;
                12_742_000.0
                    * (lat.sin().powi(2)
                        + place.coordinate.latitude.to_radians().cos()
                            * point.latitude.to_radians().cos()
                            * lon.sin().powi(2))
                    .clamp(0.0, 1.0)
                    .sqrt()
                    .asin()
            };
            if let Some(place) = places
                .into_iter()
                .filter(|place| distance(place) <= 1000.0)
                .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            {
                return Ok(place);
            }
            if self.settings.geocoder.is_empty() {
                return Err("No local place within one kilometre of this point".into());
            }
        }
        let mut url = self.endpoint(&self.settings.geocoder, "reverse")?;
        url.query_pairs_mut()
            .append_pair("format", "jsonv2")
            .append_pair("lat", &point.latitude.to_string())
            .append_pair("lon", &point.longitude.to_string());
        self.place(&self.json(url).await?)
    }
}
#[async_trait]
impl Router for Provider {
    async fn route(
        &self,
        points: &[Coordinate],
        mode: Mode,
        calculated_at: i64,
    ) -> Result<Route, String> {
        if !(2..=32).contains(&points.len()) || points.iter().any(|point| !point.valid()) {
            return Err("Choose 2–32 visible route points".into());
        }
        let coordinates = points
            .iter()
            .map(|point| format!("{},{}", point.longitude, point.latitude))
            .collect::<Vec<_>>()
            .join(";");
        let mut url = self.endpoint(
            &self.settings.router,
            &format!("route/v1/{}/{coordinates}", mode.profile()),
        )?;
        url.query_pairs_mut()
            .append_pair("geometries", "geojson")
            .append_pair("overview", "full")
            .append_pair("steps", "false");
        let raw = self.json(url).await?;
        if raw["code"] != "Ok" {
            return Err("No route within this provider's coverage".into());
        }
        let route = &raw["routes"][0];
        let coordinates = route["geometry"]["coordinates"]
            .as_array()
            .ok_or("Route geometry unavailable")?;
        if !(2..=32_768).contains(&coordinates.len()) {
            return Err("Route geometry exceeds its point limit".into());
        }
        let points = coordinates
            .iter()
            .map(|point| {
                let point = Coordinate {
                    longitude: point[0].as_f64().ok_or("Invalid route longitude")?,
                    latitude: point[1].as_f64().ok_or("Invalid route latitude")?,
                };
                if !point.valid() {
                    return Err("Invalid route geometry");
                }
                Ok(point)
            })
            .collect::<Result<Vec<_>, &str>>()?;
        let distance_metres = route["distance"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("Invalid route distance")?;
        let duration_seconds = route["duration"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("Invalid route duration")?;
        Ok(Route {
            points,
            distance_metres,
            duration_seconds,
            calculated_at,
            mode,
            credits: self.settings.credits.clone(),
        })
    }
}

#[derive(Default)]
pub struct Selection {
    generation: u64,
    pub places: Vec<Place>,
    pub selected: Option<Place>,
    pub route: Option<Route>,
}
impl Selection {
    pub fn invalidate(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.places.clear();
        self.selected = None;
        self.route = None;
        self.generation
    }
    pub fn accept_places(&mut self, generation: u64, places: Vec<Place>) -> bool {
        if generation != self.generation || places.iter().any(|place| place.validate().is_err()) {
            return false;
        }
        self.places = places;
        true
    }
    pub fn choose(&mut self, index: usize) -> Option<&Place> {
        self.selected = self.places.get(index).cloned();
        self.selected.as_ref()
    }
    pub fn accept_route(&mut self, generation: u64, route: Route) -> bool {
        if generation != self.generation || route.validate().is_err() {
            return false;
        }
        self.route = Some(route);
        true
    }
}
