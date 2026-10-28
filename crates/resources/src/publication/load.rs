use super::*;
use crate::{Reference, reference::Address};
use std::collections::VecDeque;

pub fn files(directory: &Path) -> Result<Vec<std::path::PathBuf>> {
    let root = fs::canonicalize(directory).map_err(|error| error.to_string())?;
    let mut pending = vec![root.clone()];
    let mut output = Vec::new();
    let mut count = 0usize;
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let item = item.map_err(|error| error.to_string())?;
            count += 1;
            if count > 100_000 {
                return Err("Choose a smaller source folder".into());
            }
            let kind = item.file_type().map_err(|error| error.to_string())?;
            if kind.is_dir() {
                pending.push(item.path());
            } else if kind.is_file()
                && item
                    .path()
                    .extension()
                    .is_some_and(|value| value == "lingua")
            {
                output.push(item.path());
            }
        }
    }
    output.sort();
    if output.len() > 4096 {
        return Err("Select at most 4096 source documents at a time".into());
    }
    Ok(output)
}

fn read(library: &Library, candidate: &Address, refresh: bool) -> Result<Document> {
    let retained = if let Some(hash) = &candidate.hash {
        library
            .find(&Resource {
                uri: candidate.uri.clone(),
                ..Default::default()
            })?
            .filter(|entry| entry.hash.eq_ignore_ascii_case(hash))
    } else {
        None
    };
    let saved = retained.is_some();
    let raw = if let Some(entry) = retained {
        if entry.bytes > 4 * 1024 * 1024 {
            return Err("The source exceeds its parsing limit".into());
        }
        fs::read_to_string(entry.path).map_err(|error| error.to_string())?
    } else {
        library.source_text(&candidate.uri, 4 * 1024 * 1024, refresh)?
    };
    let hash = fingerprint(raw.as_bytes());
    if candidate
        .hash
        .as_ref()
        .is_some_and(|expected| !expected.eq_ignore_ascii_case(&hash))
    {
        return Err("The source does not match its pinned content hash".into());
    }
    let base = library.source_base(&candidate.uri)?;
    let text = anicca::ensure_uids(&images(&base, &raw)?)
        .map_err(|error| error.to_string())?
        .0;
    let document = anicca::parse(&text).map_err(|error| error.to_string())?;
    let project = anicca::project(&document).map_err(|error| error.to_string())?;
    if let Some(uid) = &candidate.uid
        && !project.records.iter().any(|record| record.uid == *uid)
    {
        return Err(format!(
            "{} does not define the requested Record {uid}",
            candidate.uri
        ));
    }
    Ok(Document {
        uri: candidate.uri.clone(),
        base,
        hash,
        text,
        raw,
        cached: library.source_cached(&candidate.uri),
        saved,
        project,
    })
}

impl Publication {
    pub fn files(directory: &Path) -> Result<Vec<std::path::PathBuf>> {
        files(directory)
    }

    pub fn index(library: &Library, directory: &Path) -> Result<Vec<std::path::PathBuf>> {
        let files = files(directory)?;
        for path in &files {
            if fs::metadata(path).map_err(|error| error.to_string())?.len() > 4 * 1024 * 1024 {
                continue;
            }
            let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
            if let Ok(project) = anicca::parse(&raw).and_then(|document| anicca::project(&document))
            {
                let base = Reference::path(path)?.target;
                let hash = fingerprint(raw.as_bytes());
                for record in project.records {
                    library.remember_definition(&record.uid, &base, Some(&hash))?;
                }
            }
        }
        Ok(files)
    }

    pub fn load(
        library: &Library,
        entry: &str,
        selected: &[String],
        refresh: bool,
    ) -> Result<Self> {
        Self::load_reference(library, Reference::new(entry), selected, refresh)
    }

    pub fn load_reference(
        library: &Library,
        reference: Reference,
        selected: &[String],
        refresh: bool,
    ) -> Result<Self> {
        let addresses = reference.addresses(library)?;
        let fragment = addresses.first().and_then(|value| value.fragment.clone());
        let mut entry = addresses.first().ok_or("Choose a source")?.uri.clone();
        let url = address(&entry)?;
        let mut pending = VecDeque::new();
        let mut directory_entry = false;
        if url.scheme() == "file"
            && let Ok(path) = url.to_file_path()
            && path.is_dir()
        {
            directory_entry = true;
            for path in files(&path)? {
                let uri = Reference::path(&path)?.target;
                if selected.is_empty()
                    || selected
                        .iter()
                        .any(|value| value == &uri || path.ends_with(value))
                {
                    pending.push_back(Reference::new(uri));
                }
            }
        } else {
            pending.push_back(reference);
        }
        let mut visited = BTreeMap::<String, usize>::new();
        let mut documents = Vec::<Document>::new();
        let mut options = Vec::new();
        let mut parameters = BTreeMap::new();
        let mut instances = BTreeSet::new();
        let mut records = BTreeMap::new();
        let mut expanded_bytes = 0usize;
        let mut slugs = BTreeMap::new();
        while let Some(reference) = pending.pop_front() {
            let mut failures = Vec::new();
            let mut obtained = None;
            let mut already_loaded = false;
            for candidate in reference.addresses(library)? {
                if let Some(index) = visited.get(&candidate.uri) {
                    let document = &documents[*index];
                    if candidate.uid.as_ref().is_some_and(|uid| {
                        !document
                            .project
                            .records
                            .iter()
                            .any(|record| &record.uid == uid)
                    }) || candidate
                        .hash
                        .as_ref()
                        .is_some_and(|hash| !document.hash.eq_ignore_ascii_case(hash))
                    {
                        failures.push(format!("{} has a conflicting identity", candidate.uri));
                        continue;
                    }
                    already_loaded = true;
                    break;
                }
                match read(library, &candidate, refresh) {
                    Ok(document) => {
                        obtained = Some(document);
                        break;
                    }
                    Err(error) => failures.push(format!("{}: {error}", candidate.uri)),
                }
            }
            if already_loaded {
                continue;
            }
            let mut document = obtained.ok_or_else(|| failures.join("\n"))?;
            if documents.is_empty() && !directory_entry {
                entry = document.uri.clone();
            }
            if let Some(index) = visited.get(&document.base) {
                if documents[*index].hash != document.hash {
                    return Err(format!(
                        "{} changed while its composition was being read; reopen one source revision",
                        document.base
                    ));
                }
                visited.insert(document.uri, *index);
                continue;
            }
            if documents.len() >= 4096 {
                return Err("Select at most 4096 source documents at a time".into());
            }
            expanded_bytes = expanded_bytes
                .checked_add(document.raw.len())
                .ok_or("Source composition size overflow")?;
            if expanded_bytes > 256 * 1024 * 1024 {
                return Err("Choose a composition under 256 MiB of Lingua text; originals stream separately".into());
            }
            let base = &document.base;
            for import in &document.project.imports {
                if let Some(uid) = import.target.strip_prefix('@')
                    && !crate::reference::record_uid(uid)
                {
                    return Err("An identity import needs an authored Record UID; use a relative URI for a document".into());
                }
                let mut reference = Reference::at(&import.target, base);
                reference.locations = import.locations.clone();
                pending.push_back(reference);
            }
            for record in &mut document.project.records {
                for assertion in &mut record.assertions {
                    if let Some(uri) = &mut assertion.object_uri {
                        *uri = resolve(base, uri)?;
                    }
                }
                if let Some(previous) = records.insert(record.uid.clone(), document.uri.clone()) {
                    return Err(format!(
                        "Record {} has conflicting definitions in {previous} and {}",
                        record.uid, document.uri
                    ));
                }
                if let Some(slug) = &record.slug
                    && let Some(previous) = slugs.insert(slug.clone(), record.uid.clone())
                {
                    return Err(format!(
                        "@{slug} is declared by {previous} and {}",
                        record.uid
                    ));
                }
            }
            for extension in &mut document.project.extensions {
                if extension.namespace == SOURCE_NAMESPACE {
                    let source: Source = serde_json::from_value(extension.fields.clone())
                        .map_err(|error| error.to_string())?;
                    for mut dependency in source.dependencies {
                        let original = dependency.uri.clone();
                        dependency.uri = resolve(base, &dependency.uri)?;
                        if dependency.required
                            || selected
                                .iter()
                                .any(|value| value == &original || value == &dependency.uri)
                        {
                            pending.push_back(Reference::new(&dependency.uri));
                        }
                        options.push(dependency);
                    }
                    for (name, input) in source.parameters {
                        if let Some(existing) = parameters.get(&name)
                            && serde_json::to_value(existing).map_err(|error| error.to_string())?
                                != serde_json::to_value(&input)
                                    .map_err(|error| error.to_string())?
                        {
                            return Err(format!("Conflicting source parameter @{name}"));
                        }
                        parameters.insert(name, input);
                    }
                    instances.extend(source.instances);
                } else if extension.namespace == RESOURCE_NAMESPACE {
                    let mut resource: Resource = serde_json::from_value(extension.fields.clone())
                        .map_err(|error| error.to_string())?;
                    resource.uri = resolve(base, &resource.uri)?;
                    for location in &mut resource.locations {
                        *location = resolve(base, location)?;
                    }
                    for representation in &mut resource.representations {
                        representation.uri = resolve(base, &representation.uri)?;
                    }
                    resource.validate()?;
                    extension.fields =
                        serde_json::to_value(resource).map_err(|error| error.to_string())?;
                }
            }
            let index = documents.len();
            visited.insert(document.uri.clone(), index);
            visited.insert(document.base.clone(), index);
            documents.push(document);
        }
        if documents.is_empty() {
            return Err("The selection contains no Lingua documents".into());
        }
        for document in &mut documents {
            anicca::resolve_project_references(&mut document.project, &slugs)
                .map_err(|error| error.to_string())?;
        }
        for instance in &instances {
            if !slugs.contains_key(instance) || parameters.contains_key(instance) {
                return Err(format!(
                    "The personal instance @{instance} needs its own declared Record"
                ));
            }
        }
        for document in &documents {
            for record in &document.project.records {
                library.remember_definition(&record.uid, &document.base, Some(&document.hash))?;
            }
        }
        let revision = fingerprint(
            documents
                .iter()
                .map(|doc| format!("{}:{}:{}\n", doc.uri, doc.base, doc.hash))
                .collect::<String>()
                .as_bytes(),
        );
        Ok(Self {
            entry,
            fragment,
            revision,
            documents,
            options,
            parameters,
            instances,
        })
    }
}
