mod audit;
pub use audit::Audit;
mod export;
mod load;

use crate::{
    Library, RESOURCE_NAMESPACE, Resource, Result, SOURCE_NAMESPACE, address, fingerprint, resolve,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

fn required() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub uri: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "required")]
    pub required: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub label: String,
    pub kind: String,
    pub default: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Source {
    pub dependencies: Vec<Dependency>,
    pub parameters: BTreeMap<String, Input>,
    pub instances: Vec<String>,
    pub origin: Option<Origin>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub entry: String,
    pub revision: String,
    pub document: String,
    pub base: String,
    pub hash: String,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub uri: String,
    pub base: String,
    pub hash: String,
    pub text: String,
    pub raw: String,
    pub cached: bool,
    pub saved: bool,
    pub project: anicca::ProjectedDocument,
}

#[derive(Clone, Debug)]
pub struct Publication {
    pub entry: String,
    pub fragment: Option<String>,
    pub revision: String,
    pub documents: Vec<Document>,
    pub options: Vec<Dependency>,
    pub parameters: BTreeMap<String, Input>,
    pub instances: BTreeSet<String>,
}

impl Publication {
    pub fn resources(&self) -> Result<Vec<(String, Resource)>> {
        self.documents
            .iter()
            .flat_map(|document| &document.project.extensions)
            .filter(|extension| extension.namespace == RESOURCE_NAMESPACE)
            .map(|extension| {
                Ok((
                    extension
                        .target_uid
                        .clone()
                        .ok_or("Resource has no Record identity")?,
                    serde_json::from_value(extension.fields.clone())
                        .map_err(|error| error.to_string())?,
                ))
            })
            .collect()
    }
}

fn images(base: &str, text: &str) -> Result<String> {
    let pattern =
        regex::Regex::new(r"!\[([^\]\n]*)\]\(([^\s\)]+)\)").map_err(|error| error.to_string())?;
    let mut assets = BTreeMap::new();
    let mut error = None;
    let body = pattern.replace_all(text, |captures: &regex::Captures<'_>| {
        if captures[2].starts_with("data:") {
            return captures[0].to_owned();
        }
        let uri = match resolve(base, &captures[2]) {
            Ok(uri) => uri,
            Err(problem) => {
                error = Some(problem);
                return captures[0].to_owned();
            }
        };
        let hash = fingerprint(format!("{base}:{uri}").as_bytes());
        let slug = format!("asset-{}", &hash[..20]);
        assets.insert(slug.clone(), (uri, captures[1].to_owned(), hash));
        format!("![[{slug}]]")
    });
    if let Some(error) = error {
        return Err(error);
    }
    let mut output = body.into_owned();
    for (slug, (uri, title, hash)) in assets {
        let record = anicca::ProjectedRecord {
            uid: format!("r_0{}", hash[..25].to_ascii_uppercase()),
            slug: Some(slug.clone()),
            head: if title.is_empty() {
                "Illustration".into()
            } else {
                title
            },
            body: String::new(),
            quantity: Some(("0".into(), None)),
            assertions: Vec::new(),
        };
        output.push('\n');
        output.push_str(&anicca::render_record(&record).map_err(|error| error.to_string())?);
        let extension = uri
            .split('?')
            .next()
            .unwrap_or(&uri)
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let media_type = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/*",
        };
        output.push_str(
            &anicca::extension::render(
                &slug,
                RESOURCE_NAMESPACE,
                &serde_json::to_value(Resource {
                    uri,
                    title: record.head,
                    media_type: media_type.into(),
                    ..Default::default()
                })
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        );
    }
    Ok(output)
}
