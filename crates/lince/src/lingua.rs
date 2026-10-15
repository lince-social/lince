mod data;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anicca::grammar::grammar as ast;
use nucleus::script::{Evaluator, Host};

const USAGE: &str = "usage: lince lingua FILE [--directory DIR]\n       lince lingua check [FILE_OR_DIR]\n       lince lingua fmt [--write] FILE_OR_DIR\n       lince lingua eval EXPR [--directory DIR]\n\nScripts support dir = \"...\", let, local assignment, if/else, for, print, exact arithmetic, comparisons, and Boolean conditions.\nArrays: let result = []; for value in [1, 2] { if value > 1 { result = push(result, value); } } print(result);\nReads: quantity(@record), record(@record), property(@record, \"head\"), records(#concept), query({source: \"record\", where: [{quantity_lt: 0}], limit: 20}).\nData is opened only when a read executes. No database is created or migrated.";

pub fn dispatch(arguments: &[String]) -> Result<(), String> {
    if arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{USAGE}");
        return Ok(());
    }
    let mut arguments = arguments.iter();
    let first = arguments.next().expect("nonempty arguments");
    let command = if matches!(first.as_str(), "check" | "fmt" | "eval") {
        first.as_str()
    } else {
        "execute"
    };
    let mut target = (command == "execute").then(|| first.clone());
    let mut directory = None;
    let mut write = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--directory" if matches!(command, "execute" | "eval") => {
                if directory.is_some() {
                    return Err("--directory was provided twice".into());
                }
                directory = Some(PathBuf::from(
                    arguments.next().ok_or("--directory needs a path")?,
                ));
            }
            "--write" if command == "fmt" => write = true,
            value if value.starts_with('-') && command != "eval" => {
                return Err(format!("Unknown option `{value}`\n{USAGE}"));
            }
            _ => {
                if target.replace(argument.clone()).is_some() {
                    return Err(USAGE.into());
                }
            }
        }
    }
    if command == "eval" {
        let source = target.ok_or(USAGE)?;
        let expression = anicca::expression::parse(&source).map_err(|error| error.to_string())?;
        let mut host = data::DataHost::new(directory);
        let value = Evaluator::new(&mut host)
            .evaluate(&expression, 0)
            .map_err(|error| error.to_string())?;
        return host.print(&format!("{}\n", value.render()));
    }
    if command == "fmt" && target.is_none() {
        return Err(USAGE.into());
    }
    let path = PathBuf::from(target.unwrap_or_else(|| "institute/anicca".into()));
    if matches!(command, "check" | "fmt") {
        let paths = if command == "check" {
            anicca::check_project(&path).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            })?
        } else if path.is_dir() {
            anicca::lingua_paths(&path).map_err(|error| error.to_string())?
        } else {
            vec![path]
        };
        for path in &paths {
            let source = read_source(path)?;
            let document = anicca::parse(&source)
                .map_err(|error| diagnostic(path, &source, 0, &error.message))?;
            configuration(&document, path)?;
            if command == "check" {
                nucleus::script::validate(&document)
                    .map_err(|error| diagnostic(path, &source, error.offset, &error.message))?;
            }
            if command == "fmt" {
                let formatted = anicca::format(&document);
                if write {
                    if source != formatted {
                        write_formatted(path, &formatted)?;
                    }
                } else {
                    data::print_output(&formatted)?;
                }
            }
        }
        if command == "check" {
            println!("checked {} .lingua file(s)", paths.len());
        }
        return Ok(());
    }
    let source = read_source(&path)?;
    run_source(&path, &source, directory)
}

fn read_source(path: &Path) -> Result<String, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.len() > 1024 * 1024 {
        return Err(format!("{}: script exceeds 1 MiB", path.display()));
    }
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn run_source(path: &Path, source: &str, directory: Option<PathBuf>) -> Result<(), String> {
    let document =
        anicca::parse(source).map_err(|error| diagnostic(path, source, 0, &error.message))?;
    let configured = configuration(&document, path)?;
    let mut host = data::DataHost::new(directory.or(configured));
    Evaluator::new(&mut host)
        .run(&document)
        .map_err(|error| diagnostic(path, source, error.offset, &error.message))
}

fn configuration(document: &ast::Document, path: &Path) -> Result<Option<PathBuf>, String> {
    let mut directory = None;
    let mut names = BTreeSet::new();
    for declaration in &document.declarations {
        match declaration {
            ast::Declaration::Statement(ast::Statement::Assignment(value))
                if value.name.0 == "dir" =>
            {
                if directory.is_some() {
                    return Err(format!(
                        "{}: dir is assigned more than once",
                        path.display()
                    ));
                }
                let ast::ExpressionKind::Text(value) = &value.value.node.value else {
                    return Err(format!(
                        "{}: dir needs a literal path in quotes",
                        path.display()
                    ));
                };
                if value.0.is_empty() {
                    return Err(format!("{}: dir cannot be empty", path.display()));
                }
                let selected = PathBuf::from(&value.0);
                directory = Some(if selected.is_absolute() {
                    selected
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(selected)
                });
            }
            ast::Declaration::Statement(ast::Statement::Binding(value)) => {
                if value.name.0 == "dir" || !names.insert(&value.name.0) {
                    return Err(format!(
                        "{}: duplicate or reserved binding `{}`",
                        path.display(),
                        value.name.0
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(directory)
}

fn diagnostic(path: &Path, source: &str, offset: usize, message: &str) -> String {
    let before = source.get(..offset).unwrap_or(source);
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    format!("{}:{line}:{column}: {message}", path.display())
}

fn write_formatted(path: &Path, source: &str) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().unwrap_or(Path::new("."));
    let temporary = parent.join(format!(".lingua-format-{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let permissions = std::fs::metadata(path)?.permissions();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&temporary)?;
        output.write_all(source.as_bytes())?;
        output.sync_all()?;
        std::fs::set_permissions(&temporary, permissions)?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error: std::io::Error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests;
