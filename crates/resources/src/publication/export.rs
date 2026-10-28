use super::*;
use anicca::grammar::grammar::{self as ast, Declaration};
use serde_json::Value;

fn relative(
    base: &str,
    value: &str,
    output: &crate::Url,
    locations: &BTreeMap<String, crate::Url>,
) -> Result<String> {
    let resolved = resolve(base, value)?;
    let mut url = address(&resolved)?;
    let fragment = url.fragment().map(str::to_owned);
    url.set_fragment(None);
    if let Some(destination) = locations.get(url.as_str()) {
        let mut destination = destination.clone();
        destination.set_fragment(fragment.as_deref());
        Ok(output
            .make_relative(&destination)
            .unwrap_or_else(|| destination.to_string()))
    } else {
        Ok(resolved)
    }
}

impl Publication {
    pub fn export(
        &self,
        library: &Library,
        destination: &Path,
        media: bool,
    ) -> Result<std::path::PathBuf> {
        if destination.exists()
            && fs::read_dir(destination)
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
        {
            return Err("Choose an empty destination directory".into());
        }
        let destination = std::path::absolute(destination).map_err(|error| error.to_string())?;
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        let mut common = address(&self.documents[0].base)?
            .join(".")
            .map_err(|error| error.to_string())?;
        while self
            .documents
            .iter()
            .any(|document| !document.base.starts_with(common.as_str()))
        {
            let parent = common.join("..").map_err(|error| error.to_string())?;
            if parent == common {
                break;
            }
            common = parent;
        }
        let root =
            crate::Url::from_directory_path(&destination).map_err(|_| "Invalid export folder")?;
        let mut locations = BTreeMap::new();
        let mut names = BTreeSet::new();
        for document in &self.documents {
            let url = address(&document.base)?;
            let name = if document.base.starts_with(common.as_str()) {
                document.base[common.as_str().len()..].to_owned()
            } else {
                format!(
                    "{}/{}",
                    &fingerprint(document.base.as_bytes())[..12],
                    url.path().rsplit('/').next().unwrap_or("Document.lingua")
                )
            };
            let mut target = root.join(&name).map_err(|error| error.to_string())?;
            target.set_query(None);
            target.set_fragment(None);
            if !names.insert(target.to_string()) {
                let basename = url.path().rsplit('/').next().unwrap_or("Document.lingua");
                target = root
                    .join(&format!("{}/{basename}", &document.hash[..12]))
                    .map_err(|error| error.to_string())?;
                if !names.insert(target.to_string()) {
                    return Err("Conflicting export filenames".into());
                }
            }
            locations.insert(document.uri.clone(), target.clone());
            locations.insert(document.base.clone(), target);
        }
        if media {
            for (_, resource) in self.resources()? {
                if locations.contains_key(&resource.uri) {
                    continue;
                }
                let entry = library.fetch(&resource)?;
                let name = address(&resource.uri)?
                    .path()
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Resource")
                    .to_owned();
                let target = root
                    .join(&format!("files/{}/{name}", &entry.hash[..12]))
                    .map_err(|error| error.to_string())?;
                let path = target
                    .to_file_path()
                    .map_err(|_| "Invalid media export path")?;
                fs::create_dir_all(path.parent().ok_or("Invalid media export path")?)
                    .map_err(|error| error.to_string())?;
                fs::copy(&entry.path, path).map_err(|error| error.to_string())?;
                locations.insert(resource.uri, target);
            }
        }
        for source in &self.documents {
            let output = &locations[&source.uri];
            let rewrite = |value: &str| relative(&source.base, value, output, &locations);
            let mut document = anicca::parse(&source.text).map_err(|error| error.to_string())?;
            for declaration in &mut document.declarations {
                match declaration {
                    Declaration::Import(import) => {
                        if import.target.is_uri() {
                            import.target = ast::ImportTarget::uri(rewrite(import.target.value())?);
                        }
                        if let Some(locations) = &mut import.locations {
                            for location in &mut locations.values {
                                location.value.0 = rewrite(&location.value.0)?;
                            }
                        }
                    }
                    Declaration::Record(record) => {
                        for field in &mut record.header.rest {
                            if let ast::RecordField::Assertion(assertion) = &mut field.field
                                && let Some(ast::AssertionTail::Link(link)) = &mut assertion.tail
                                && link.target.is_uri()
                            {
                                link.target = ast::Reference::uri(rewrite(link.target.value())?);
                            }
                        }
                        if let Some(description) = &mut record.description {
                            let pattern = regex::Regex::new(r"(!?\[[^\]\n]*\]\()([^\)\n]+)(\))")
                                .map_err(|error| error.to_string())?;
                            let mut problem = None;
                            description.0 = pattern
                                .replace_all(&description.0, |captures: &regex::Captures<'_>| {
                                    match rewrite(&captures[2]) {
                                        Ok(uri) => {
                                            format!("{}{}{}", &captures[1], uri, &captures[3])
                                        }
                                        Err(error) => {
                                            problem = Some(error);
                                            captures[0].to_owned()
                                        }
                                    }
                                })
                                .into_owned();
                            if let Some(error) = problem {
                                return Err(error);
                            }
                            let pattern = regex::Regex::new(r"\[\[([^\]\n]+)\]\]")
                                .map_err(|error| error.to_string())?;
                            description.0 = pattern
                                .replace_all(&description.0, |captures: &regex::Captures<'_>| {
                                    let (label, target) = captures[1]
                                        .split_once('|')
                                        .map_or((None, &captures[1]), |(label, target)| {
                                            (Some(label), target)
                                        });
                                    if !crate::reference::is_address(target)
                                        && !target.starts_with('#')
                                    {
                                        return captures[0].to_owned();
                                    }
                                    match rewrite(target) {
                                        Ok(uri) => label.map_or_else(
                                            || format!("[[{uri}]]"),
                                            |label| format!("[[{label}|{uri}]]"),
                                        ),
                                        Err(error) => {
                                            problem = Some(error);
                                            captures[0].to_owned()
                                        }
                                    }
                                })
                                .into_owned();
                            if let Some(error) = problem {
                                return Err(error);
                            }
                        }
                    }
                    Declaration::Extension(extension) => {
                        let projected = anicca::extension::project(extension)
                            .map_err(|error| error.to_string())?;
                        let mut fields = projected.fields;
                        if projected.namespace == RESOURCE_NAMESPACE {
                            let mut resource: Resource = serde_json::from_value(fields)
                                .map_err(|error| error.to_string())?;
                            let original = resolve(&source.base, &resource.uri)?;
                            resource.uri = rewrite(&resource.uri)?;
                            for location in &mut resource.locations {
                                *location = rewrite(location)?;
                            }
                            for representation in &mut resource.representations {
                                representation.uri = rewrite(&representation.uri)?;
                            }
                            if media {
                                if resource.source_uri.is_empty() {
                                    resource.source_uri = original.clone();
                                }
                                if let Some(entry) = library.find(&Resource {
                                    uri: original,
                                    hash: resource.hash.clone(),
                                    ..Default::default()
                                })? {
                                    resource.hash = Some(entry.hash);
                                }
                            }
                            fields = serde_json::to_value(resource)
                                .map_err(|error| error.to_string())?;
                        } else if projected.namespace == SOURCE_NAMESPACE {
                            fields["origin"] = serde_json::to_value(Origin {
                                entry: self.entry.clone(),
                                revision: self.revision.clone(),
                                document: source.uri.clone(),
                                base: source.base.clone(),
                                hash: source.hash.clone(),
                            })
                            .map_err(|error| error.to_string())?;
                            if let Some(dependencies) =
                                fields.get_mut("dependencies").and_then(Value::as_array_mut)
                            {
                                for dependency in dependencies {
                                    if let Some(uri) = dependency["uri"].as_str() {
                                        dependency["uri"] = Value::String(rewrite(uri)?);
                                    }
                                }
                            }
                        }
                        let rendered = anicca::extension::render(
                            &projected.target_slug,
                            &projected.namespace,
                            &fields,
                        )
                        .map_err(|error| error.to_string())?;
                        *declaration = anicca::parse(&rendered)
                            .map_err(|error| error.to_string())?
                            .declarations
                            .remove(0);
                    }
                    _ => {}
                }
            }
            let path = output
                .to_file_path()
                .map_err(|_| "Invalid export document path")?;
            fs::create_dir_all(path.parent().ok_or("Invalid export document path")?)
                .map_err(|error| error.to_string())?;
            let text = anicca::format(&document);
            fs::write(&path, &text).map_err(|error| error.to_string())?;
            fs::write(path.with_extension("lingua.origin.json"), serde_json::to_vec(&serde_json::json!({"origin":Origin { entry:self.entry.clone(),revision:self.revision.clone(),document:source.uri.clone(),base:source.base.clone(),hash:source.hash.clone()},"exported_hash":fingerprint(text.as_bytes())})).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        }
        if let Some(target) = locations.get(&self.entry) {
            target
                .to_file_path()
                .map_err(|_| "Invalid export path".into())
        } else {
            Ok(destination)
        }
    }
}
