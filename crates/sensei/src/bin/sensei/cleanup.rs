use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sensei::rules::no_comments::strip_ordinary;

use super::{
    analyzer,
    compiler::{self, Problem, Suggestions},
    lsp::Client,
};

#[derive(Default)]
struct Options {
    preview: bool,
    no_comments: bool,
    compiler: bool,
    compiler_only: bool,
    shorten_paths: bool,
    workspace: bool,
    manifest: Option<PathBuf>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = OsString>) -> io::Result<Self> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--preview") => options.preview = true,
                Some("--no-comments") => options.no_comments = true,
                Some("--compiler") => options.compiler = true,
                Some("--compiler-only") => {
                    options.compiler = true;
                    options.compiler_only = true;
                }
                Some("--shorten-paths") => options.shorten_paths = true,
                Some("--workspace") => options.workspace = true,
                Some("--manifest-path") if options.manifest.is_none() => {
                    let path = args
                        .next()
                        .ok_or_else(|| io::Error::other("missing manifest path"))?;
                    options.manifest = Some(PathBuf::from(path).canonicalize()?);
                }
                _ => return Err(io::Error::other(format!("unknown fix option {arg:?}"))),
            }
        }
        if options.workspace && (!options.compiler || options.manifest.is_some()) {
            return Err(io::Error::other(
                "--workspace selects packages for --compiler",
            ));
        }
        if options.compiler && options.manifest.is_none() && !options.workspace {
            return Err(io::Error::other(
                "--compiler needs --manifest-path or --workspace",
            ));
        }
        if options.compiler_only && (options.no_comments || options.shorten_paths) {
            return Err(io::Error::other(
                "--compiler-only excludes --no-comments and --shorten-paths",
            ));
        }
        Ok(options)
    }
}

pub(super) fn fix(root: &Path, args: impl Iterator<Item = OsString>) -> io::Result<()> {
    let mut options = Options::parse(args)?;
    if !options.compiler {
        return fix_scope(root, &options).map(|_| ());
    }
    let manifests = if options.workspace {
        package_manifests(root)?
    } else {
        options.manifest.take().into_iter().collect()
    };
    let mut outcomes = Vec::new();
    for manifest in manifests {
        let relative = manifest.strip_prefix(root).unwrap_or(&manifest);
        println!("sensei: checking {}", relative.display());
        options.manifest = Some(manifest.clone());
        let outcome = fix_scope(root, &options);
        if options.workspace
            && let Err(error) = &outcome
        {
            eprintln!("sensei: {}: {error}", relative.display());
        }
        outcomes.push((manifest, outcome));
    }
    if !options.preview {
        compiler::write_report(root, &outcomes)?;
    }
    if !options.workspace
        && let Some((_, result)) = outcomes.pop()
    {
        return result.map(|_| ());
    }
    let failed = outcomes
        .iter()
        .filter(|(_, result)| result.is_err())
        .count();
    if failed > 0 {
        return Err(io::Error::other(format!(
            "{failed} package cleanup passes failed"
        )));
    }
    Ok(())
}

fn package_manifests(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    for relative in listed_paths(root, ":(glob)**/Cargo.toml")? {
        let manifest = root.join(&relative);
        if protected_path(&relative) || manifest.canonicalize().is_ok_and(|path| path != manifest) {
            continue;
        }
        let Ok(source) = fs::read_to_string(&manifest) else {
            continue;
        };
        if package_manifest(&source) {
            manifests.push(manifest);
        }
    }
    Ok(manifests)
}

fn package_manifest(source: &str) -> bool {
    source.lines().any(|line| line.trim() == "[package]")
}

fn fix_scope(root: &Path, options: &Options) -> io::Result<BTreeSet<Problem>> {
    let scope = match options.manifest.as_deref() {
        Some(path)
            if path.file_name().is_some_and(|name| name == "Cargo.toml")
                && path.starts_with(root) =>
        {
            if protected_path(path.strip_prefix(root).unwrap_or(path)) {
                return Err(io::Error::other("select a first-party package manifest"));
            }
            path.parent().unwrap_or(root)
        }
        Some(_) => return Err(io::Error::other("select a Cargo.toml inside this checkout")),
        None => root,
    };
    let sources = editable_sources(root, scope)?;
    let mut analysis = options
        .shorten_paths
        .then(|| {
            Client::start(
                root,
                options
                    .manifest
                    .as_deref()
                    .unwrap_or(&root.join("Cargo.toml")),
            )
        })
        .transpose()?;
    let compiler_manifest = options.manifest.as_deref().filter(|_| options.compiler);
    let suggestions = compiler_manifest
        .map(|manifest| {
            if !package_manifest(&fs::read_to_string(manifest)?) {
                return Err(io::Error::other("--compiler needs a package manifest"));
            }
            Suggestions::collect(root, manifest, &sources, !options.compiler_only)
                .map_err(|error| io::Error::other(format!("Baseline failed. {error}")))
        })
        .transpose()?;

    let mut changed = 0;
    let mut before_chars = 0;
    let mut after_chars = 0;
    let mut before_lines = 0;
    let mut after_lines = 0;
    let mut compiler_files = 0;
    let mut shortened_files = 0;
    let mut analyzer_failures = 0;
    // Compute proposed edits before writing any of them.
    let mut edits = Vec::new();
    for (path, source) in &sources {
        let corrected = suggestions
            .as_ref()
            .map(|suggestions| suggestions.apply(path, source));
        let input = corrected.as_deref().unwrap_or(source);
        compiler_files += usize::from(input != source);
        let shortened = match analysis
            .as_mut()
            .map(|client| analyzer::shorten(client, path, input))
        {
            Some(Ok(shortened)) => shortened,
            Some(Err(error)) => {
                eprintln!("sensei: analyzer skipped {}: {error}", path.display());
                analyzer_failures += 1;
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::BrokenPipe
                ) {
                    analysis = None;
                }
                None
            }
            None => None,
        };
        let input = shortened.as_deref().unwrap_or(input);
        shortened_files += usize::from(shortened.is_some());
        let stripped = options.no_comments.then(|| strip_ordinary(input));
        let input = stripped.as_deref().unwrap_or(input);
        let revised = if options.compiler_only {
            input.to_owned()
        } else {
            format(root, path, input)?
        };
        if revised != *source {
            changed += 1;
            before_chars += source.chars().filter(|c| !c.is_whitespace()).count();
            after_chars += revised.chars().filter(|c| !c.is_whitespace()).count();
            before_lines += source
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            after_lines += revised
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            println!(
                "{} {}",
                if options.preview {
                    "would edit"
                } else {
                    "edit"
                },
                path.display()
            );
            if !options.preview {
                edits.push((path.clone(), source.clone(), revised));
            }
        }
    }
    let verify = compiler_manifest.map(|manifest| compiler::check(root, manifest));
    apply(&edits, verify).map_err(|error| io::Error::other(format!("Cleanup failed. {error}")))?;
    println!(
        "sensei: {changed} changed files, {before_chars} to {after_chars} non-whitespace characters"
    );
    println!("sensei: {before_lines} to {after_lines} nonblank lines in changed files");
    if options.compiler || options.shorten_paths {
        println!(
            "sensei: {compiler_files} files with compiler fixes, {shortened_files} with shorter paths"
        );
    }
    if analyzer_failures > 0 {
        return Err(io::Error::other(format!(
            "{analyzer_failures} analyzer requests failed during cleanup"
        )));
    }
    match compiler_manifest {
        Some(manifest) if !edits.is_empty() => Suggestions::remaining(root, manifest, &sources)
            .map_err(|error| io::Error::other(format!("Post-edit Clippy report failed. {error}"))),
        _ => Ok(suggestions.map_or_else(BTreeSet::new, |suggestions| suggestions.problems)),
    }
}

fn apply(edits: &[(PathBuf, String, String)], verify: Option<Command>) -> io::Result<()> {
    if edits.is_empty() {
        return Ok(());
    }
    let mut written = 0;
    let result = (|| {
        for (path, source, formatted) in edits {
            if &path.canonicalize()? != path || &fs::read_to_string(path)? != source {
                return Err(io::Error::other(format!(
                    "{} changed during cleanup",
                    path.display()
                )));
            }
            fs::write(path, formatted)?;
            written += 1;
        }
        if let Some(mut command) = verify {
            run(&mut command)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for (path, original, formatted) in edits[..written].iter().rev() {
            match fs::read_to_string(path) {
                Ok(current) if current == *formatted => {
                    if let Err(error) = fs::write(path, original) {
                        eprintln!("sensei: could not restore {}: {error}", path.display());
                    }
                }
                _ => eprintln!("sensei: {} changed after the cleanup write", path.display()),
            }
        }
    }
    result
}

fn editable_sources(root: &Path, scope: &Path) -> io::Result<Vec<(PathBuf, String)>> {
    let mut sources = Vec::new();
    for relative in listed_paths(root, "*.rs")? {
        let path = root.join(&relative);
        if !path.starts_with(scope) || protected_path(&relative) {
            continue;
        }
        if scope != root
            && path
                .ancestors()
                .skip(1)
                .take_while(|dir| *dir != scope)
                .any(|dir| dir.join("Cargo.toml").is_file())
        {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            result => result?,
        };
        if metadata.is_file() && path.canonicalize()? == path {
            let source = fs::read_to_string(&path)?;
            if !generated(&source) {
                sources.push((path, source));
            }
        }
    }
    Ok(sources)
}

fn listed_paths(root: &Path, pattern: &str) -> io::Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            pattern,
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let inventory = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut paths: Vec<_> = inventory
        .split_terminator('\0')
        .map(PathBuf::from)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

fn protected_path(relative: &Path) -> bool {
    relative.iter().any(|component| component == "vendor")
        || relative.starts_with("crates/ailuros/firmware/iroh")
        || [
            "crates/ailuros/protocol/src/lib.rs",
            "crates/nucleus/src/karma/timezone_builtin.rs",
            "crates/nucleus/src/simulation/checks.rs",
            "crates/simulation/src/checks.rs",
            "crates/simulation/src/checks/quantity.rs",
        ]
        .iter()
        .any(|path| relative == Path::new(path))
}

fn generated(source: &str) -> bool {
    source.lines().take(40).any(|line| {
        let line = line.trim_start();
        (line.starts_with("//") || line.starts_with("/*"))
            && (line.contains("@generated") || line.to_ascii_lowercase().contains("do not edit"))
    })
}

fn format(root: &Path, path: &Path, source: &str) -> io::Result<String> {
    let mut child = Command::new("rustfmt")
        .current_dir(root)
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("rustfmt stdin is piped");
    // Drain output while feeding input, including when rustfmt rejects a file early.
    let (write_result, output) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(source.as_bytes()));
        let output = child.wait_with_output();
        (
            writer.join().expect("rustfmt input writer did not panic"),
            output,
        )
    });
    let output = output?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "rustfmt failed for {}\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    write_result?;
    String::from_utf8(output.stdout).map_err(io::Error::other)
}

pub(super) fn run(command: &mut Command) -> io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{command:?} exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_comments_by_default_and_filters_compiler_edits() -> io::Result<()> {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let root = checkout
            .join("target")
            .join(format!("sensei-cleanup-fixture-{}", std::process::id()));
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fs::create_dir(&root)?;
        let _fixture = Fixture(root.clone());
        fs::create_dir(root.join("src"))?;
        fs::create_dir(root.join("vendor"))?;
        run(Command::new("git")
            .current_dir(&root)
            .args(["init", "--quiet"]))?;
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"cleanup-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )?;
        let path = root.join("src/lib.rs");
        let source = concat!(
            "//! Public docs.\n",
            "// ordinary explanation\n",
            "use std::env::args_os;\n",
            "use std::process::exit;\n",
            "use std::io::Write;\n",
            "pub fn entry(input: Vec<String>) -> Vec<String> {\n",
            "  if args_os().count() == 0 { exit(1); }\n",
            "  input.into_iter().map(|value| identity(value)).collect()\n",
            "}\n",
            "pub fn with_reason(input: Vec<String>) -> Vec<String> {\n",
            "  input.into_iter().map(|value| /* keep this */ identity(value)).collect()\n",
            "}\n",
            "fn identity(value: String) -> String { value }\n",
            "pub fn borrow<'a>(value: &'a str) -> &'a str { return value; }\n",
            "pub fn label(value: u32) -> String { format!(\"{}\", value) }\n",
            "pub struct Item { pub name: String }\n",
            "pub fn item(name: String) -> Item { Item { name: name } }\n",
            "pub fn title(name: Option<String>) { let _ = name.as_ref().map(String::as_str); }\n",
            "pub fn empty(name: Option<&str>) -> bool { name.map_or(true, str::is_empty) }\n",
            "pub fn own(items: &[String]) -> Vec<String> { items.iter().cloned().collect() }\n",
            "pub fn count(items: &[String]) -> usize { items.iter().count() }\n",
            "pub fn slice(value: &[u8]) -> &[u8] { value.as_ref() }\n",
            "pub fn print_if_set(first: bool, second: bool) { if first { if second { println!(\"set\"); } } }\n",
            "pub fn required(name: Option<String>) -> String { name.unwrap() }\n",
            "#[allow(clippy::unwrap_used)]\n",
            "pub fn excused(name: Option<String>) -> String { name.unwrap() }\n",
        );
        fs::write(&path, source)?;
        run(Command::new("cargo")
            .current_dir(&root)
            .args(["generate-lockfile", "--offline"]))?;
        let vendor = root.join("vendor/upstream.rs");
        let generated = root.join("src/generated.rs");
        let unformatted = "fn   example() {}\n";
        fs::write(&vendor, unformatted)?;
        fs::write(&generated, format!("// @generated\n{unformatted}"))?;

        let compiler = [
            OsString::from("--compiler"),
            OsString::from("--manifest-path"),
            manifest.clone().into_os_string(),
        ];
        fix(
            &root,
            [
                "--compiler".into(),
                "--workspace".into(),
                "--preview".into(),
            ]
            .into_iter(),
        )?;
        assert_eq!(fs::read_to_string(&path)?, source);
        let problems = root.join("institute/trails/lince/problems.md");
        assert!(!problems.exists());

        fix(
            &root,
            ["--compiler-only".into(), "--workspace".into()].into_iter(),
        )?;
        let revised = fs::read_to_string(&path)?;
        assert!(revised.contains("let _ = name.as_deref();"), "{revised}");
        assert!(
            revised.contains("{ name.is_none_or(str::is_empty) }"),
            "{revised}"
        );
        assert!(revised.contains("{ items.to_vec() }"), "{revised}");
        assert!(revised.contains("{ items.len() }"), "{revised}");
        assert!(revised.contains("-> &[u8] { value }"), "{revised}");
        assert!(revised.contains(".map(|value| identity(value))"));
        assert!(revised.contains(".map(|value| /* keep this */ identity(value))"));
        let report = fs::read_to_string(&problems)?;
        assert_eq!(
            report.matches("`clippy::unwrap_used`").count(),
            1,
            "{report}"
        );
        assert!(report.contains("`clippy::redundant_closure`"), "{report}");
        assert!(report.contains("`clippy::collapsible_if`"), "{report}");
        assert!(
            !report.contains("`clippy::iter_cloned_collect`"),
            "{report}"
        );

        fix(&root, compiler.into_iter())?;
        let result = fs::read_to_string(&path)?;
        assert!(
            result.contains("use std::env::args_os;\nuse std::process::exit;"),
            "{result}"
        );
        assert!(result.contains(".map(identity)"), "{result}");
        assert!(
            result.contains("pub fn borrow(value: &str) -> &str"),
            "{result}"
        );
        assert!(result.contains("format!(\"{value}\")"), "{result}");
        assert!(result.contains("Item { name: name }"), "{result}");
        assert!(result.contains("if first && second {"), "{result}");
        assert!(result.contains("ordinary explanation"));
        assert!(result.contains("/* keep this */"));
        assert!(!result.contains("io::Write"));

        fix(&root, ["--no-comments".into()].into_iter())?;
        let stripped = fs::read_to_string(&path)?;
        assert!(stripped.starts_with("//! Public docs."));
        assert!(!stripped.contains("ordinary explanation"));
        assert!(!stripped.contains("/* keep this */"));
        assert_eq!(fs::read_to_string(&vendor)?, unformatted);
        assert_eq!(
            fs::read_to_string(&generated)?,
            format!("// @generated\n{unformatted}")
        );
        let invalid = [(
            path.clone(),
            stripped.clone(),
            "#![deny(unused_imports)]\nuse std::env::args_os;\n".into(),
        )];
        assert!(apply(&invalid, Some(compiler::check(&root, &manifest))).is_err());
        assert_eq!(fs::read_to_string(&path)?, stripped);
        if std::env::var_os("SENSEI_TEST_ANALYZER").is_some() {
            fs::write(
                &path,
                format!("{stripped}\npub mod paths;\npub mod imported;\n"),
            )?;
            let calls = root.join("src/paths.rs");
            let imported = root.join("src/imported.rs");
            fs::write(
                &imported,
                "use std::env::var_os;\npub fn value() -> Option<std::ffi::OsString> { std::env::var_os(\"A\").or_else(|| var_os(\"B\")) }\n",
            )?;
            let original = concat!(
                "pub fn count() -> usize {\n",
                "    let note = \"é\"; let first = std::env::args_os().count();\n",
                "    note.len() + first + std::env::args_os().count() + std::env::args_os().count()\n",
                "}\n",
                "pub fn current_dir() -> std::io::Result<std::path::PathBuf> { std::env::current_dir() }\n",
            );
            fs::write(&calls, original)?;
            let analyzer = [
                OsString::from("--shorten-paths"),
                OsString::from("--manifest-path"),
                manifest.into_os_string(),
            ];
            fix(&root, analyzer.iter().cloned().chain(["--preview".into()]))?;
            assert_eq!(fs::read_to_string(&calls)?, original);
            fix(&root, analyzer.into_iter())?;
            let result = fs::read_to_string(calls)?;
            assert!(result.contains("use std::env::args_os;"), "{result}");
            assert_eq!(result.matches("args_os().count()").count(), 3, "{result}");
            assert_eq!(result.matches("std::env::args_os").count(), 1, "{result}");
            assert!(result.contains("std::env::current_dir()"), "{result}");
            let result = fs::read_to_string(imported)?;
            assert!(result.contains("var_os(\"A\")"), "{result}");
            assert_eq!(result.matches("std::env::var_os").count(), 1, "{result}");
        }
        Ok(())
    }
}
