use crate::{
    config::{ProviderKind, Settings},
    driver::Driver,
};
use genai::adapter::AdapterKind;
use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
};

static BUNDLED: std::sync::OnceLock<Driver> = std::sync::OnceLock::new();

pub fn register_bundled(executable: std::path::PathBuf) {
    let _ = BUNDLED.set(Driver {
        executable,
        arguments: vec!["--fiote-provider".into()],
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    Browser,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMethod {
    pub id: String,
    pub label: String,
    pub kind: AuthKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    pub id: ProviderKind,
    pub label: String,
    pub endpoint: String,
    pub auth_methods: Vec<AuthMethod>,
    #[serde(default)]
    pub model_optional: bool,
}

#[derive(Default)]
pub struct Catalog {
    pub descriptors: Vec<Descriptor>,
    drivers: Vec<(String, Driver)>,
}

impl Catalog {
    pub async fn load(directory: &std::path::Path) -> Result<Self, String> {
        let mut variants = Vec::new();
        let _ = AdapterKind::deserialize(Variants(&mut variants));
        let client = genai::Client::builder()
            .with_auth_resolver_fn(|_| Ok(Some(genai::resolver::AuthData::from_single(""))))
            .build();
        let mut catalog = Self::default();
        for variant in variants {
            let adapter: AdapterKind =
                serde_json::from_value(serde_json::Value::String(variant.into()))
                    .map_err(|e| e.to_string())?;
            let target = client
                .resolve_service_target(genai::ModelIden::new(adapter, "catalog"))
                .await
                .map_err(|_| "Cannot read provider metadata from the model library.")?;
            let kind = if adapter.default_key_env_name().is_some() {
                AuthKind::ApiKey
            } else {
                AuthKind::None
            };
            catalog.descriptors.push(Descriptor {
                id: ProviderKind(adapter.as_lower_str().into()),
                label: adapter.to_string(),
                endpoint: target.endpoint.base_url().into(),
                auth_methods: vec![AuthMethod {
                    id: "default".into(),
                    label: if kind == AuthKind::None {
                        "Local connection"
                    } else {
                        "API key"
                    }
                    .into(),
                    kind,
                }],
                model_optional: false,
            });
        }
        let path = directory.join("providers.json");
        if let Some(driver) = BUNDLED.get() {
            for descriptor in crate::provider_adapter::descriptors() {
                catalog
                    .drivers
                    .push((descriptor.id.0.clone(), driver.clone()));
                catalog.descriptors.push(descriptor);
            }
        }
        match std::fs::read(path) {
            Ok(bytes) => {
                if bytes.len() > 65_536 {
                    return Err("Provider adapter configuration is too large.".into());
                }
                let drivers: Vec<Driver> = serde_json::from_slice(&bytes)
                    .map_err(|_| "Invalid provider adapter configuration.")?;
                if drivers.len() > 32 {
                    return Err("Too many provider adapters.".into());
                }
                for driver in drivers {
                    for descriptor in driver.discover().await? {
                        if descriptor.id.0.is_empty()
                            || descriptor.id.0.len() > 128
                            || descriptor.label.len() > 256
                            || descriptor.auth_methods.is_empty()
                            || descriptor.auth_methods.len() > 8
                            || descriptor.auth_methods.iter().any(|method| {
                                method.id.is_empty()
                                    || method.id.len() > 128
                                    || method.label.len() > 256
                            })
                        {
                            return Err("Invalid provider descriptor.".into());
                        }
                        if catalog
                            .descriptors
                            .iter()
                            .any(|existing| existing.id == descriptor.id)
                        {
                            return Err("Provider adapters returned duplicate identifiers.".into());
                        }
                        catalog
                            .drivers
                            .push((descriptor.id.0.clone(), driver.clone()));
                        catalog.descriptors.push(descriptor);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        catalog.descriptors.sort_by(|a, b| {
            let browser = |item: &Descriptor| {
                item.auth_methods
                    .iter()
                    .any(|method| method.kind == AuthKind::Browser)
            };
            browser(b)
                .cmp(&browser(a))
                .then_with(|| a.label.cmp(&b.label))
        });
        Ok(catalog)
    }

    pub fn descriptor(&self, id: &ProviderKind) -> Result<&Descriptor, String> {
        self.descriptors
            .iter()
            .find(|item| &item.id == id)
            .ok_or_else(|| "Choose an available provider.".into())
    }

    pub fn method(&self, settings: &Settings) -> Result<&AuthMethod, String> {
        let descriptor = self.descriptor(&settings.provider)?;
        descriptor
            .auth_methods
            .iter()
            .find(|method| method.id == settings.auth_method)
            .ok_or_else(|| "Choose an available login method.".into())
    }

    pub fn validate(&self, settings: &mut Settings) -> Result<(), String> {
        let descriptor = self.descriptor(&settings.provider)?;
        if settings.auth_method.is_empty() {
            settings.auth_method = descriptor.auth_methods[0].id.clone();
        }
        self.method(settings)?;
        if settings.endpoint.trim().is_empty() {
            settings.endpoint = descriptor.endpoint.clone();
        }
        if descriptor.endpoint.is_empty() && !settings.endpoint.is_empty() {
            return Err("This adapter manages its own endpoint.".into());
        }
        if settings.model.trim().is_empty() && !descriptor.model_optional {
            return Err("Enter a model name.".into());
        }
        settings.validate()
    }

    pub fn driver(&self, id: &ProviderKind) -> Option<&Driver> {
        self.drivers
            .iter()
            .find(|(provider, _)| provider == &id.0)
            .map(|(_, driver)| driver)
    }
}

struct Variants<'a>(&'a mut Vec<&'static str>);
impl<'de> de::Deserializer<'de> for Variants<'_> {
    type Error = de::value::Error;
    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom("metadata only"))
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.extend_from_slice(variants);
        Err(de::Error::custom("metadata only"))
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any }
}
