use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs, io,
    ops::Range,
    path::{Path, PathBuf},
    process::Command,
};

use sensei::rules::no_comments::comments_in;
use serde_json::Value;

const CLIPPY: &[&str] = &[
    "clippy::iter_cloned_collect",
    "clippy::iter_count",
    "clippy::option_as_ref_deref",
    "clippy::unnecessary_map_or",
    "clippy::useless_asref",
];

const WITH_RUSTFMT: &[&str] = &[
    "clippy::collapsible_if",
    "clippy::let_and_return",
    "clippy::map_identity",
    "clippy::match_like_matches_macro",
    "clippy::needless_borrow",
    "clippy::needless_lifetimes",
    "clippy::needless_return",
    "clippy::redundant_closure",
    "clippy::redundant_closure_for_method_calls",
    "clippy::uninlined_format_args",
    "clippy::useless_conversion",
];

const REVIEW: &[&str] = &["clippy::unnecessary_to_owned", "clippy::unwrap_used"];

const RUSTC: &[&str] = &[
    "non_shorthand_field_patterns",
    "redundant_semicolons",
    "unused_import_braces",
    "unused_imports",
    "unused_parens",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    range: Range<usize>,
    replacement: String,
}

#[derive(Default)]
pub(super) struct Suggestions {
    edits: BTreeMap<PathBuf, Vec<Vec<Edit>>>,
    pub(super) problems: BTreeSet<Problem>,
}

impl Suggestions {
    pub(super) fn collect(
        root: &Path,
        manifest: &Path,
        sources: &[(PathBuf, String)],
        with_rustfmt: bool,
    ) -> io::Result<Self> {
        let mut suggestions = Self::default();
        let (mut baseline, rustc_lints) = if with_rustfmt {
            (rustc_suggestions(root, manifest), RUSTC)
        } else {
            (check(root, manifest), &[][..])
        };
        baseline.arg("--message-format=json");
        suggestions.read(&mut baseline, root, manifest, (rustc_lints, &[]))?;
        let lints = if with_rustfmt {
            [CLIPPY, WITH_RUSTFMT].concat()
        } else {
            CLIPPY.to_vec()
        };
        suggestions.read(
            &mut clippy(root, manifest),
            root,
            manifest,
            (&lints, sources),
        )?;
        Ok(suggestions)
    }

    pub(super) fn remaining(
        root: &Path,
        manifest: &Path,
        sources: &[(PathBuf, String)],
    ) -> io::Result<BTreeSet<Problem>> {
        let mut suggestions = Self::default();
        suggestions.read(&mut clippy(root, manifest), root, manifest, (&[], sources))?;
        Ok(suggestions.problems)
    }

    fn read(
        &mut self,
        cmd: &mut Command,
        root: &Path,
        manifest: &Path,
        (lints, sources): (&[&str], &[(PathBuf, String)]),
    ) -> io::Result<()> {
        let output = cmd.output()?;
        let mut first_error = None;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if !line.starts_with('{') {
                continue;
            }
            let entry: Value = serde_json::from_str(line).map_err(io::Error::other)?;
            if entry["reason"] != "compiler-message" {
                continue;
            }
            let diagnostic = &entry["message"];
            if diagnostic["level"] == "error" {
                first_error = first_error.or(diagnostic["message"].as_str().map(str::to_owned));
                if let Some(rendered) = diagnostic["rendered"].as_str() {
                    eprint!("{rendered}");
                }
            }
            if entry["manifest_path"].as_str().map(Path::new) != Some(manifest) {
                continue;
            }
            let Some(lint) = diagnostic["code"]["code"].as_str() else {
                continue;
            };
            if lint.starts_with("clippy::") {
                self.problems.extend(Problem::from_diagnostic(
                    diagnostic, lint, root, manifest, sources,
                ));
            }
            if !lints.contains(&lint) {
                continue;
            }
            for child in diagnostic["children"].as_array().into_iter().flatten() {
                if let Some((path, edits)) = single_file_edits(child, root, manifest) {
                    self.edits.entry(path).or_default().push(edits);
                }
            }
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprint!("{stderr}");
            let cause = first_error
                .as_deref()
                .or_else(|| stderr.lines().find_map(|line| line.strip_prefix("error: ")));
            let subcommand = cmd.get_args().next().unwrap_or_default().to_string_lossy();
            return Err(io::Error::other(format!(
                "cargo {subcommand} exited with {}. {}",
                output.status,
                cause.unwrap_or_default()
            )));
        }
        Ok(())
    }

    pub(super) fn apply(&self, path: &Path, source: &str) -> String {
        let Some(groups) = self.edits.get(path) else {
            return source.to_owned();
        };
        let comments = comments_in(source);
        let mut chosen: Vec<Edit> = Vec::new();
        for group in groups {
            // A multipart suggestion is applied as a unit. Keep original comments
            // and discard sets whose edits would overlap another replacement.
            if group.iter().any(|edit| {
                source.get(edit.range.clone()).is_none()
                    || comments.iter().any(|comment| {
                        edit.range.start < comment.end && comment.start < edit.range.end
                    })
                    || chosen
                        .iter()
                        .any(|prior| prior != edit && overlaps(prior, edit))
            }) {
                continue;
            }
            for edit in group {
                if !chosen.contains(edit) {
                    chosen.push(edit.clone());
                }
            }
        }
        chosen.sort_unstable_by_key(|edit| edit.range.start);
        let mut result = source.to_owned();
        for edit in chosen.into_iter().rev() {
            result.replace_range(edit.range, &edit.replacement);
        }
        result
    }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Problem {
    path: PathBuf,
    line: u64,
    column: u64,
    lint: String,
    message: String,
}

impl Problem {
    fn from_diagnostic(
        diagnostic: &Value,
        lint: &str,
        root: &Path,
        manifest: &Path,
        sources: &[(PathBuf, String)],
    ) -> Option<Self> {
        let span = diagnostic["spans"]
            .as_array()?
            .iter()
            .find(|span| span["is_primary"] == true)?;
        let path = span_path(span, root, manifest)?;
        sources
            .binary_search_by(|(source, _)| source.cmp(&path))
            .ok()?;
        Some(Self {
            path: path.strip_prefix(root).ok()?.to_owned(),
            line: span["line_start"].as_u64()?,
            column: span["column_start"].as_u64()?,
            lint: lint.into(),
            message: diagnostic["message"]
                .as_str()?
                .lines()
                .collect::<Vec<_>>()
                .join(" "),
        })
    }
}

pub(super) fn write_report(
    root: &Path,
    outcomes: &[(PathBuf, io::Result<BTreeSet<Problem>>)],
) -> io::Result<()> {
    let mut report = String::from(
        "# Clippy problems\n\nGenerated by Sensei for the latest compiler cleanup. Findings are from first-party sources on host targets with default features. They include diagnostics that need review and edits the cleanup skipped. Failed packages have incomplete coverage.\n",
    );
    let mut count = 0;
    for (manifest, outcome) in outcomes {
        let name = manifest.strip_prefix(root).unwrap_or(manifest).display();
        match outcome {
            Ok(problems) if problems.is_empty() => continue,
            Ok(problems) => {
                let _ = writeln!(report, "\n## `{name}`\n");
                count += problems.len();
                for problem in problems {
                    let Problem {
                        path,
                        line,
                        column,
                        lint,
                        message,
                    } = problem;
                    let _ = writeln!(
                        report,
                        "- `{}:{line}:{column}` · `{lint}` · {message}",
                        path.display()
                    );
                }
            }
            Err(error) => {
                let reason = error.to_string().lines().collect::<Vec<_>>().join(" ");
                let _ = writeln!(report, "\n## `{name}`\n\nCoverage incomplete. {reason}");
            }
        }
    }
    let passed = outcomes.iter().filter(|(_, result)| result.is_ok()).count();
    let _ = writeln!(
        report,
        "\n{count} findings across {passed} of {} selected packages.",
        outcomes.len()
    );
    let directory = root.join("institute/trails/lince");
    fs::create_dir_all(&directory)?;
    let pending = directory.join(format!(".problems.{}.tmp", std::process::id()));
    let path = directory.join("problems.md");
    fs::write(&pending, report)?;
    if let Err(error) = fs::rename(&pending, &path) {
        let _ = fs::remove_file(&pending);
        return Err(error);
    }
    println!("sensei: {count} Clippy findings in {}", path.display());
    Ok(())
}

pub(super) fn check(root: &Path, manifest: &Path) -> Command {
    cargo(root, manifest, "check")
}

fn rustc_suggestions(root: &Path, manifest: &Path) -> Command {
    let flags = RUSTC
        .iter()
        .map(|lint| format!("'--force-warn={lint}'"))
        .collect::<Vec<_>>()
        .join(",");
    let mut cmd = check(root, manifest);
    cmd.arg("--config")
        .arg(format!("build.rustflags=[{flags}]"));
    cmd
}

fn clippy(root: &Path, manifest: &Path) -> Command {
    let mut cmd = cargo(root, manifest, "clippy");
    cmd.args([
        "--message-format=json",
        "--no-deps",
        "--",
        "--cap-lints=warn",
    ])
    .args(
        CLIPPY
            .iter()
            .chain(WITH_RUSTFMT)
            .chain(REVIEW)
            .map(|lint| format!("-W{lint}")),
    );
    cmd
}

fn cargo(root: &Path, manifest: &Path, subcommand: &str) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .arg(subcommand)
        .args(["--locked", "--all-targets", "--manifest-path"])
        .arg(manifest)
        .arg("--target-dir")
        .arg(root.join("target/sensei-fix"));
    cmd
}

fn single_file_edits(
    diagnostic: &Value,
    root: &Path,
    manifest: &Path,
) -> Option<(PathBuf, Vec<Edit>)> {
    let mut edits = Vec::new();
    let mut file = None;
    for span in diagnostic["spans"].as_array()? {
        if span["suggestion_applicability"] != "MachineApplicable" || !span["expansion"].is_null() {
            return None;
        }
        let path = span_path(span, root, manifest)?;
        if file.as_ref().is_some_and(|chosen| *chosen != path) {
            return None;
        }
        file = Some(path);
        let start = usize::try_from(span["byte_start"].as_u64()?).ok()?;
        let end = usize::try_from(span["byte_end"].as_u64()?).ok()?;
        let edit = Edit {
            range: start..end,
            replacement: span["suggested_replacement"].as_str()?.to_owned(),
        };
        if edits.iter().any(|prior| overlaps(prior, &edit)) {
            return None;
        }
        edits.push(edit);
    }
    Some((file?, edits))
}

fn span_path(span: &Value, root: &Path, manifest: &Path) -> Option<PathBuf> {
    let name = Path::new(span["file_name"].as_str()?);
    root.join(name)
        .canonicalize()
        .or_else(|_| manifest.parent().unwrap_or(root).join(name).canonicalize())
        .ok()
}

fn overlaps(a: &Edit, b: &Edit) -> bool {
    a.range.start <= b.range.end && b.range.start <= a.range.end
}
