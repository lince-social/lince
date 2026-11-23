// non-lean-yet
// Scope: struct Authored, struct AuthoredPage, struct AuthoredLesson, struct AuthoredStep.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 176.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 176.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 103.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 57,
//   103, 240.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites publication:
//   String (103).

use std::{
    collections::BTreeMap,
    env::var_os,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use lince_resources::{Library, publication::Publication};
use serde::Deserialize;

use crate::{
    handbook::Page,
    practice::{Lesson, Operation, Step},
};

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

const ENTRY: &str = "Lince.lingua";

fn local_entry(directory: &Path) -> Option<PathBuf> {
    [ENTRY, "Instinct.lingua", "instinct/Instinct.lingua"]
        .into_iter()
        .map(|entry| directory.join(entry))
        .find(|path| path.is_file())
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
                    let entry = local_entry(&path).unwrap_or_else(|| path.join(ENTRY));
                    lince_resources::Reference::path(&entry).ok()
                })
                .map(|reference| reference.target);
        }
        if url.path().ends_with('/') {
            url = url.join(ENTRY).ok()?;
        }
        return Some(url.to_string());
    }
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    for root in [
        directory.to_path_buf(),
        directory.parent().unwrap_or(directory).to_path_buf(),
    ] {
        if let Some(path) = local_entry(&root.join("content")) {
            return lince_resources::Reference::path(&path)
                .ok()
                .map(|reference| reference.target);
        }
    }
    let config = var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let path = local_entry(&config.join("lince/content"))?;
    lince_resources::Reference::path(&path)
        .ok()
        .map(|reference| reference.target)
}

pub fn publication() -> Result<Option<&'static Publication>, String> {
    static SOURCE: OnceLock<Result<Option<Publication>, String>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let Some(location) = location() else {
                return Ok(None);
            };
            let config = var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
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
