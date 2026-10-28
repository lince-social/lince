use super::*;
use crate::Reference;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Audit {
    pub documents: usize,
    pub links: usize,
    pub issues: Vec<String>,
    pub unchecked: Vec<String>,
}

impl Publication {
    pub fn audit(
        library: &Library,
        directory: &Path,
        inverses: &[(String, String)],
    ) -> Result<Audit> {
        let mut report = Audit::default();
        let mut documents = Vec::new();
        let mut identities = BTreeMap::new();
        for path in Self::files(directory)? {
            let reference = Reference::path(&path)?;
            let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let project = anicca::parse(&raw)
                .and_then(|document| anicca::project(&document))
                .map_err(|error| error.to_string())?;
            for record in &project.records {
                if let Some(previous) =
                    identities.insert(record.uid.clone(), reference.target.clone())
                {
                    report.issues.push(format!(
                        "{} is defined in both {previous} and {}",
                        record.uid, reference.target
                    ));
                }
            }
            documents.push((reference.target, project));
        }
        report.documents = documents.len();
        let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
        let mut assertions = BTreeSet::new();
        let markdown =
            regex::Regex::new(r"\[[^\]\n]*\]\(([^\)\n]+)\)").map_err(|error| error.to_string())?;
        let wiki = regex::Regex::new(r"\[\[([^\]\n]+)\]\]").map_err(|error| error.to_string())?;
        let mut targets = Vec::new();
        for (base, project) in &documents {
            let slugs: BTreeMap<_, _> = project
                .records
                .iter()
                .filter_map(|record| Some((record.slug.as_ref()?, &record.uid)))
                .collect();
            for import in &project.imports {
                let mut reference = Reference::at(&import.target, base);
                reference.locations = import.locations.clone();
                targets.push((base.clone(), reference));
            }
            for record in &project.records {
                edges.entry(record.uid.clone()).or_default();
                for capture in markdown.captures_iter(&record.body) {
                    targets.push((record.uid.clone(), Reference::at(&capture[1], base)));
                }
                for capture in wiki.captures_iter(&record.body) {
                    let target = capture[1]
                        .split_once('|')
                        .map_or(&capture[1], |(_, target)| target)
                        .trim_start_matches("record:")
                        .trim_start_matches('@');
                    let reference = if let Some(uid) = slugs.get(&target.to_owned()) {
                        Reference::new(format!("@{uid}"))
                    } else {
                        Reference::at(target, base)
                    };
                    targets.push((record.uid.clone(), reference));
                }
                for assertion in &record.assertions {
                    if let Some(uri) = &assertion.object_uri {
                        targets.push((record.uid.clone(), Reference::at(uri, base)));
                        if let Ok(url) = address(&resolve(base, uri)?)
                            && let Some(uid) = url.fragment()
                        {
                            assertions.insert((
                                record.uid.clone(),
                                assertion.predicate.clone(),
                                uid.to_owned(),
                            ));
                        }
                    } else if let Some(object) = assertion.object_uid.as_ref().or_else(|| {
                        assertion
                            .object_slug
                            .as_ref()
                            .and_then(|slug| slugs.get(slug).copied())
                    }) {
                        targets.push((record.uid.clone(), Reference::new(format!("@{object}"))));
                        assertions.insert((
                            record.uid.clone(),
                            assertion.predicate.clone(),
                            object.clone(),
                        ));
                    }
                }
                for extension in project.extensions.iter().filter(|extension| {
                    extension.target_uid.as_ref() == Some(&record.uid)
                        && extension.namespace == RESOURCE_NAMESPACE
                }) {
                    let resource: Resource = serde_json::from_value(extension.fields.clone())
                        .map_err(|error| error.to_string())?;
                    let mut reference = Reference::at(resource.uri, base);
                    reference.locations = resource.locations;
                    reference.hash = resource.hash;
                    targets.push((record.uid.clone(), reference));
                }
            }
        }
        for (from, reference) in targets {
            report.links += 1;
            let identity = reference.target.trim_start_matches('@');
            if crate::reference::is_id(&reference.target)
                && let Some(document) = identities.get(identity)
            {
                edges.entry(from).or_default().insert(identity.into());
                let _ = document;
                continue;
            }
            let candidates = match reference.addresses(library) {
                Ok(candidates) => candidates,
                Err(error) => {
                    report.issues.push(error);
                    continue;
                }
            };
            let mut valid = false;
            for candidate in candidates {
                let url = address(&candidate.uri)?;
                if let Some(uid) = &candidate.uid {
                    if identities
                        .get(uid)
                        .is_some_and(|document| document == &candidate.uri)
                    {
                        edges.entry(from.clone()).or_default().insert(uid.clone());
                    } else if url.scheme() == "file" {
                        let target = url
                            .to_file_path()
                            .map_err(|_| "Invalid audit file address")?;
                        let declared = fs::metadata(&target).ok().is_some_and(|metadata| {
                            metadata.is_file() && metadata.len() <= 4 * 1024 * 1024
                        }) && fs::read_to_string(&target)
                            .ok()
                            .and_then(|text| {
                                anicca::parse(&text)
                                    .and_then(|document| anicca::project(&document))
                                    .ok()
                            })
                            .is_some_and(|document| {
                                document.records.iter().any(|record| &record.uid == uid)
                            });
                        if !declared {
                            report
                                .issues
                                .push(format!("{} does not define fragment {uid}", candidate.uri));
                            continue;
                        }
                        report
                            .unchecked
                            .push(format!("Outside navigation scope: {}", candidate.uri));
                    }
                }
                if url.scheme() == "file" {
                    if url.to_file_path().is_ok_and(|path| path.is_file()) {
                        valid = true;
                        break;
                    }
                } else {
                    report.unchecked.push(candidate.uri);
                    valid = true;
                    break;
                }
            }
            if !valid {
                report
                    .issues
                    .push(format!("No available target for {}", reference.target));
            }
        }
        for (from, predicate, to) in &assertions {
            for (forward, reverse) in inverses {
                if predicate == forward
                    && identities.contains_key(to)
                    && !assertions.contains(&(to.clone(), reverse.clone(), from.clone()))
                {
                    report
                        .issues
                        .push(format!("{to} lacks inverse #{reverse} to {from}"));
                }
            }
        }
        let mut traversal_budget = 1_000_000usize;
        for entry in identities.keys() {
            let mut seen = BTreeSet::new();
            let mut pending = vec![entry.clone()];
            while let Some(uid) = pending.pop() {
                if traversal_budget == 0 {
                    report.unchecked.push(
                        "Navigation analysis reached its one-million-edge operation budget".into(),
                    );
                    break;
                }
                traversal_budget -= 1;
                if seen.insert(uid.clone())
                    && let Some(next) = edges.get(&uid)
                {
                    pending.extend(next.iter().cloned());
                }
            }
            if traversal_budget == 0 {
                break;
            }
            let missing: Vec<_> = identities
                .keys()
                .filter(|uid| !seen.contains(*uid))
                .cloned()
                .collect();
            if !missing.is_empty() {
                report.unchecked.push(format!(
                    "From {entry}, {} known Records have no directed navigation path",
                    missing.len()
                ));
            }
        }
        report.issues.sort();
        report.issues.dedup();
        report.unchecked.sort();
        report.unchecked.dedup();
        Ok(report)
    }
}
