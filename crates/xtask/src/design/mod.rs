mod native;
mod server;

use lince_interface::tokens::{
    ColorScheme, SandStyleKind, ThemeSettings, TokenOverrides, css, document::ThemeDocument,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::Result;

const FOCUSES: &[&str] = &[
    "structure",
    "style",
    "interaction",
    "density",
    "copy",
    "accessibility",
];
const VIEWER: &str = include_str!("assets/viewer.html");
const PROTOTYPE: &str = include_str!("assets/prototype.html");
const STYLE: &str = include_str!("assets/prototype.css");
const SCRIPT: &str = include_str!("assets/prototype.js");

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Study {
    title: String,
    focus: String,
    features: Vec<Feature>,
    alternatives: Vec<Alternative>,
    scenarios: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Feature {
    id: String,
    label: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Alternative {
    id: String,
    title: String,
    file: String,
    hypothesis: String,
    placements: BTreeMap<String, String>,
}

pub(super) fn dispatch(root: &Path, args: &[OsString]) -> Result<()> {
    let args: Vec<_> = args
        .iter()
        .map(|arg| {
            arg.to_str()
                .ok_or("Arguments must be UTF-8")
                .map(str::to_owned)
        })
        .collect::<std::result::Result<_, _>>()?;
    let command = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    match command {
        "help" | "--help" | "-h" => {
            println!("cargo xtask design new <slug> --focus <focus>");
            println!(
                "Focus examples: {}. Use one narrowly named focus per study.",
                FOCUSES.join(", ")
            );
            println!("cargo xtask design serve <slug> [--port 6180]");
            println!("cargo xtask design check <slug>");
            println!(
                "cargo xtask design tokens [--scheme Dark|Light|ComfyPink|Moss] [--theme path.json] [--kind Square|Text|EditableText|Record|TimeCastle] [--format css|json]"
            );
            println!(
                "cargo xtask design native <component> [--theme path.json] [--watch] [--mobile]"
            );
            println!("Native components: {}", native::COMPONENTS.join(", "));
            Ok(())
        }
        "new" => {
            let (slug, options) = named(rest, &["--focus"], &[])?;
            let focus = options
                .get("--focus")
                .ok_or("Choose exactly one --focus; see design help")?;
            create(&study_path(root, &slug)?, &slug, focus)
        }
        "check" => {
            let (slug, _) = named(rest, &[], &[])?;
            let directory = study_path(root, &slug)?;
            let study = load(&directory)?;
            validate(&directory, &study)?;
            println!(
                "Study valid: {} · {} · {} alternatives · {} retained capabilities",
                study.title,
                study.focus,
                study.alternatives.len(),
                study.features.len()
            );
            println!(
                "Declared coverage checked. Review actual interactions and desktop/mobile layouts in the browser."
            );
            Ok(())
        }
        "serve" => {
            let (slug, options) = named(rest, &["--port"], &[])?;
            let port = options.get("--port").map_or(Ok(6180), |value| {
                value.parse::<u16>().map_err(|error| error.to_string())
            })?;
            server::serve(root, &study_path(root, &slug)?, port)
        }
        "tokens" => {
            let options = options(rest, &["--theme", "--scheme", "--kind", "--format"], &[])?;
            let theme = theme(
                options.get("--theme").map(Path::new),
                options
                    .get("--scheme")
                    .map(String::as_str)
                    .unwrap_or("Dark"),
            )?;
            let kind = kind(
                options
                    .get("--kind")
                    .map(String::as_str)
                    .unwrap_or("Square"),
            )?;
            match options.get("--format").map(String::as_str) {
                Some("css") => print!(
                    "{}",
                    css::export(&theme, Some(kind), &TokenOverrides::default())?
                ),
                Some("json") => print!("{}", ThemeDocument::export(&theme)?),
                None => {
                    print!("{}", ThemeDocument::export(&theme)?);
                    print!(
                        "{}",
                        css::export(&theme, Some(kind), &TokenOverrides::default())?
                    );
                }
                _ => return Err("--format must be css or json".into()),
            }
            Ok(())
        }
        "native" => {
            let (component, options) = named(rest, &["--theme"], &["--watch", "--mobile"])?;
            native::run(
                root,
                &component,
                options.get("--theme").map(Path::new),
                options.contains_key("--watch"),
                options.contains_key("--mobile"),
            )
        }
        _ => Err(format!(
            "Unknown design command: {command}; use cargo xtask design help"
        )),
    }
}

fn options(args: &[String], values: &[&str], flags: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut args = args.iter();
    while let Some(option) = args.next() {
        let value = if values.contains(&option.as_str()) {
            args.next()
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| format!("Missing value for {option}"))?
                .clone()
        } else if flags.contains(&option.as_str()) {
            String::new()
        } else {
            return Err(format!("Unexpected argument: {option}"));
        };
        if result.insert(option.clone(), value).is_some() {
            return Err(format!("Repeated argument: {option}"));
        }
    }
    Ok(result)
}

fn named(
    args: &[String],
    values: &[&str],
    flags: &[&str],
) -> Result<(String, BTreeMap<String, String>)> {
    let name = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .ok_or("Missing study or component name; see design help")?;
    Ok((name.clone(), options(&args[1..], values, flags)?))
}

fn slug_valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 80
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
}

fn study_path(root: &Path, slug: &str) -> Result<PathBuf> {
    if !slug_valid(slug) {
        return Err("Use a study slug of 1–80 lowercase letters, digits, and hyphens, starting with a letter or digit".into());
    }
    let parent = root.join("design/studies");
    let directory = parent.join(slug);
    if directory.exists() {
        let canonical = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let expected = root
            .canonicalize()
            .map_err(|error| error.to_string())?
            .join("design/studies")
            .join(slug);
        if canonical != expected {
            return Err("Study directories must not use symlinks".into());
        }
    }
    Ok(directory)
}

fn create(directory: &Path, slug: &str, focus: &str) -> Result<()> {
    if !slug_valid(focus) {
        return Err(format!(
            "Choose one focus slug, for example: {}",
            FOCUSES.join(", ")
        ));
    }
    let parent = directory.parent().ok_or("Missing study parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    if parent.canonicalize().map_err(|error| error.to_string())? != parent {
        return Err("Study parent directories must not use symlinks".into());
    }
    fs::create_dir(directory).map_err(|error| {
        format!("Cannot create study (existing studies are never overwritten): {error}")
    })?;
    let study = Study {
        title: format!("{slug} · starter study"),
        focus: focus.into(),
        features: vec![
            Feature {
                id: "primary-task".into(),
                label: "Example primary task — replace with the actual capability".into(),
            },
            Feature {
                id: "secondary-task".into(),
                label: "Example occasional task — replace with the actual capability".into(),
            },
        ],
        alternatives: (1..=3)
            .map(|index| Alternative {
                id: format!("alternative-{index}"),
                title: format!("Alternative {index} · scaffold"),
                file: format!("alternative-{index}.html"),
                hypothesis:
                    "Replace this scaffold with a distinct hypothesis within the chosen focus"
                        .into(),
                placements: BTreeMap::from([
                    ("primary-task".into(), "Main task tab".into()),
                    (
                        "secondary-task".into(),
                        "Main task → Optional details".into(),
                    ),
                ]),
            })
            .collect(),
        scenarios: vec![
            "Complete the primary task with populated and empty data".into(),
            "Find and use the occasional task".into(),
            "Recover from a simulated failure".into(),
            "Navigate with keyboard and at narrow touch widths".into(),
        ],
    };
    write_json(&directory.join("study.json"), &study)?;
    fs::write(directory.join("brief.md"), format!("# {slug}\n\nFocus: {focus}\n\nThis is illustrative scaffolding. Inspect the real workflow and replace example capabilities, fixtures, and alternatives before review.\n\n## Question and audience\n\nPending.\n\n## Primary tasks and current behavior\n\nPending code inspection and intent conversation.\n\n## Fixed dimensions\n\nKeep dimensions outside {focus} unchanged. Reference the existing UI or preceding decision here.\n\n## Contexts and acceptance\n\nDesktop panel, narrow mobile, and constrained Sand where applicable. Record observable acceptance tasks here.\n")).map_err(|error| error.to_string())?;
    fs::write(
        directory.join("decision.md"),
        "# Decision\n\nPending human review. No alternative has been selected.\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        directory.join("fixtures.json"),
        include_str!("assets/fixtures.json"),
    )
    .map_err(|error| error.to_string())?;
    for alternative in &study.alternatives {
        fs::write(
            directory.join(&alternative.file),
            PROTOTYPE.replace("__ALTERNATIVE__", &alternative.title),
        )
        .map_err(|error| error.to_string())?;
    }
    println!(
        "Created {}. Replace the illustrative tasks and alternatives, then run cargo xtask design serve {slug}.",
        directory.display()
    );
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())
}

fn load(directory: &Path) -> Result<Study> {
    let path = local_file(directory, "study.json")?;
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid study.json: {error}"))
}

fn local_file(directory: &Path, relative: &str) -> Result<PathBuf> {
    if !relative
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return Err("Use URL-safe local filenames containing letters, digits, dots, underscores, or hyphens".into());
    }
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Only local relative file paths are allowed".into());
    }
    let root = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("File must stay inside the study directory".into());
    }
    Ok(path)
}

fn validate(directory: &Path, study: &Study) -> Result<()> {
    if study.title.trim().is_empty() || !slug_valid(&study.focus) {
        return Err("A study needs a title and exactly one valid focus".into());
    }
    if study.features.is_empty()
        || study.alternatives.is_empty()
        || study.scenarios.is_empty()
        || study
            .scenarios
            .iter()
            .any(|scenario| scenario.trim().is_empty())
    {
        return Err(
            "Include retained capabilities, at least one alternative, and acceptance scenarios"
                .into(),
        );
    }
    let mut features = BTreeSet::new();
    for feature in &study.features {
        if !slug_valid(&feature.id)
            || feature.label.trim().is_empty()
            || !features.insert(feature.id.as_str())
        {
            return Err("Capability IDs must be unique slugs with nonempty labels".into());
        }
    }
    let mut alternatives = BTreeSet::new();
    let mut files = BTreeSet::new();
    for alternative in &study.alternatives {
        if !slug_valid(&alternative.id)
            || !alternatives.insert(&alternative.id)
            || alternative.title.trim().is_empty()
            || alternative.hypothesis.trim().is_empty()
        {
            return Err("Alternatives need unique IDs, titles, and hypotheses".into());
        }
        let path = local_file(directory, &alternative.file)?;
        if !files.insert(path.clone())
            || path.extension().and_then(|extension| extension.to_str()) != Some("html")
        {
            return Err("Each alternative needs its own local HTML file".into());
        }
        let placements: BTreeSet<_> = alternative.placements.keys().map(String::as_str).collect();
        if placements != features
            || alternative
                .placements
                .values()
                .any(|placement| placement.trim().is_empty())
        {
            return Err(format!(
                "{} must specify a destination for every retained capability, with no unknown capabilities",
                alternative.id
            ));
        }
    }
    for file in ["brief.md", "decision.md", "fixtures.json"] {
        local_file(directory, file)?;
    }
    let fixtures =
        fs::read(local_file(directory, "fixtures.json")?).map_err(|error| error.to_string())?;
    let fixtures: serde_json::Value =
        serde_json::from_slice(&fixtures).map_err(|error| error.to_string())?;
    if !fixtures
        .as_object()
        .is_some_and(|fixtures| !fixtures.is_empty())
    {
        return Err("fixtures.json must contain a nonempty object of named scenarios".into());
    }
    if directory.join("theme.json").exists() {
        theme(Some(&local_file(directory, "theme.json")?), "Dark")?;
    }
    Ok(())
}

fn theme(path: Option<&Path>, scheme: &str) -> Result<ThemeSettings> {
    if let Some(path) = path {
        return ThemeDocument::parse(&fs::read_to_string(path).map_err(|error| error.to_string())?);
    }
    let scheme = serde_json::from_value::<ColorScheme>(serde_json::Value::String(scheme.into()))
        .map_err(|_| "Unknown theme scheme".to_owned())?;
    Ok(ThemeSettings {
        scheme,
        ..Default::default()
    })
}

fn kind(value: &str) -> Result<SandStyleKind> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|_| "Unknown Sand style kind".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(super) struct Directory(pub(super) PathBuf);
    impl Directory {
        pub(super) fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lince-design-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn studies_preserve_capability_coverage_and_existing_work() {
        let temp = Directory::new();
        let path = temp.0.join("study");
        create(&path, "study", "structure").unwrap();
        let mut study = load(&path).unwrap();
        validate(&path, &study).unwrap();
        let original = fs::read(path.join("study.json")).unwrap();
        assert!(create(&path, "study", "style").is_err());
        assert_eq!(fs::read(path.join("study.json")).unwrap(), original);
        study.alternatives[0].placements.remove("secondary-task");
        assert!(
            validate(&path, &study)
                .unwrap_err()
                .contains("every retained capability")
        );
    }

    #[test]
    fn local_paths_reject_traversal_absolute_paths_and_symlink_escapes() {
        let temp = Directory::new();
        let path = temp.0.join("study");
        create(&path, "study", "style").unwrap();
        for relative in ["../study/study.json", "/etc/passwd", "./study.json", ""] {
            assert!(local_file(&path, relative).is_err(), "{relative}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                temp.0.join("study/study.json"),
                temp.0.join("outside.json"),
            )
            .unwrap();
            std::os::unix::fs::symlink(temp.0.join("outside.json"), path.join("escape.json"))
                .unwrap();
            fs::write(temp.0.join("secret.json"), "secret").unwrap();
            std::os::unix::fs::symlink(temp.0.join("secret.json"), path.join("secret.json"))
                .unwrap();
            assert!(local_file(&path, "secret.json").is_err());
            assert!(study_path(&temp.0, "../outside").is_err());
        }
    }

    #[test]
    fn malformed_studies_and_duplicate_arguments_are_rejected() {
        assert!(
            options(
                &[
                    "--focus".into(),
                    "structure".into(),
                    "--focus".into(),
                    "style".into()
                ],
                &["--focus"],
                &[]
            )
            .is_err()
        );
        let temp = Directory::new();
        assert!(create(&temp.0.join("invalid"), "invalid", "structure,style").is_err());
        create(&temp.0.join("motion"), "motion", "motion").unwrap();
        let path = temp.0.join("study");
        create(&path, "study", "interaction").unwrap();
        let mut study = load(&path).unwrap();
        study.alternatives[1].file = study.alternatives[0].file.clone();
        assert!(validate(&path, &study).is_err());
        let study = load(&path).unwrap();
        fs::write(path.join("fixtures.json"), "{}").unwrap();
        assert!(
            validate(&path, &study)
                .unwrap_err()
                .contains("nonempty object")
        );
    }
}
