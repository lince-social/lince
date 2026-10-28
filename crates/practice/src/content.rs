use crate::{
    handbook::Page,
    practice::{Lesson, Operation, Step},
};
use lince_resources::{Library, publication::Publication};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authored {
    pages: Vec<AuthoredPage>,
    lessons: Vec<AuthoredLesson>,
    reading: Vec<AuthoredLesson>,
    foundations: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    starts: BTreeMap<String, AuthoredStart>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredPage {
    slug: String,
    chapter: String,
    reference: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredLesson {
    subject: String,
    prerequisites: Vec<String>,
    steps: Vec<AuthoredStep>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredStep {
    slug: String,
    subject: String,
    operation: Option<Operation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredStart {
    lesson: String,
    step: usize,
}

#[derive(Default)]
pub(crate) struct Content {
    pub pages: Vec<Page>,
    pub lessons: Vec<Lesson>,
    pub reading: Vec<Lesson>,
    pub foundations: BTreeMap<String, &'static [&'static str]>,
    starts: BTreeMap<String, (&'static str, usize)>,
}

pub fn location() -> Option<String> {
    if let Ok(value) = std::env::var("LINCE_CONTENT") {
        if value.is_empty() {
            return None;
        }
        let mut url = lince_resources::address(&value).ok()?;
        if url.scheme() == "file" && url.to_file_path().is_ok_and(|path| path.is_dir()) {
            return url
                .to_file_path()
                .ok()
                .and_then(|path| {
                    lince_resources::Reference::path(&path.join("instinct/Instinct.lingua")).ok()
                })
                .map(|reference| reference.target);
        }
        if url.path().ends_with('/') {
            url = url.join("instinct/Instinct.lingua").ok()?;
        }
        return Some(url.to_string());
    }
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    for root in [
        directory.to_path_buf(),
        directory.parent().unwrap_or(directory).to_path_buf(),
    ] {
        let path = root.join("content/instinct/Instinct.lingua");
        if path.is_file() {
            return lince_resources::Reference::path(&path)
                .ok()
                .map(|reference| reference.target);
        }
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let path = config.join("lince/content/instinct/Instinct.lingua");
    path.is_file()
        .then(|| {
            lince_resources::Reference::path(&path)
                .ok()
                .map(|reference| reference.target)
        })
        .flatten()
}

pub fn publication() -> Result<Option<&'static Publication>, String> {
    static SOURCE: OnceLock<Result<Option<Publication>, String>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let Some(location) = location() else {
                return Ok(None);
            };
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })
                .unwrap_or_else(std::env::temp_dir);
            let library = Library::new(config.join("lince"));
            Publication::load(&library, &location, &[], false).map(Some)
        })
        .as_ref()
        .map(Option::as_ref)
        .map_err(Clone::clone)
}

fn text(value: String) -> Result<&'static str, String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("Invalid practice reference".into());
    }
    Ok(Box::leak(value.into_boxed_str()))
}
fn lessons(values: Vec<AuthoredLesson>) -> Result<Vec<Lesson>, String> {
    values
        .into_iter()
        .map(|lesson| {
            let steps = lesson
                .steps
                .into_iter()
                .map(|step| {
                    Ok(Step {
                        slug: text(step.slug)?,
                        subject: text(step.subject)?,
                        operation: step.operation,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let prerequisites = lesson
                .prerequisites
                .into_iter()
                .map(text)
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Lesson {
                subject: text(lesson.subject)?,
                prerequisites: Box::leak(prerequisites.into_boxed_slice()),
                steps: Box::leak(steps.into_boxed_slice()),
            })
        })
        .collect()
}

pub(crate) fn get() -> &'static Content {
    static CONTENT: OnceLock<Result<Content, String>> = OnceLock::new();
    static EMPTY: OnceLock<Content> = OnceLock::new();
    CONTENT
        .get_or_init(|| {
            let Some(source) = publication()? else {
                return Ok(Content::default());
            };
            let values: Vec<_> = source
                .documents
                .iter()
                .flat_map(|document| &document.project.extensions)
                .filter(|extension| extension.namespace == "lince.practice")
                .collect();
            if values.len() != 1 {
                return Err("The external handbook needs one practice declaration".into());
            }
            let value: Authored = serde_json::from_value(values[0].fields.clone())
                .map_err(|error| error.to_string())?;
            if value.pages.len() > 1024
                || value.lessons.len() > 1024
                || value.reading.len() > 1024
                || value.foundations.len() > 1024
                || value
                    .lessons
                    .iter()
                    .chain(&value.reading)
                    .any(|lesson| lesson.steps.len() > 256 || lesson.prerequisites.len() > 128)
            {
                return Err("Practice declarations exceed their per-document limits".into());
            }
            if value.starts.len() > 1024
                || value.starts.values().any(|start| {
                    !value.lessons.iter().chain(&value.reading).any(|lesson| {
                        lesson.subject == start.lesson && start.step < lesson.steps.len()
                    })
                })
            {
                return Err("Invalid authored lesson starting point".into());
            }
            Ok(Content {
                starts: value
                    .starts
                    .into_iter()
                    .map(|(name, start)| Ok((name, (text(start.lesson)?, start.step))))
                    .collect::<Result<_, String>>()?,
                pages: value
                    .pages
                    .into_iter()
                    .map(|page| {
                        Ok(Page {
                            slug: text(page.slug)?,
                            chapter: text(page.chapter)?,
                            reference: page.reference.map(text).transpose()?,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                lessons: lessons(value.lessons)?,
                reading: lessons(value.reading)?,
                foundations: value
                    .foundations
                    .into_iter()
                    .map(|(subject, values)| {
                        Ok((
                            subject,
                            &*Box::leak(
                                values
                                    .into_iter()
                                    .map(text)
                                    .collect::<Result<Vec<_>, String>>()?
                                    .into_boxed_slice(),
                            ),
                        ))
                    })
                    .collect::<Result<_, String>>()?,
            })
        })
        .as_ref()
        .unwrap_or_else(|_| EMPTY.get_or_init(Content::default))
}

pub fn start(subject: &str) -> Option<(&'static str, usize)> {
    get().starts.get(subject).copied()
}
