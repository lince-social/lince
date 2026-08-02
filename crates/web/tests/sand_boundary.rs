//! A sand is a bundle of HTML and JavaScript. It is not a layer.
//!
//! Economy is the case that forced this test. It began as a domain silo — a
//! `store::economy` module, an `economy_event` table, a private
//! `Source::Economy` Protein union, and a 655-line `nucleus::karma::economy` in
//! the *kernel*. All of it was deleted and rebuilt on general primitives:
//! entries, classification, schedules, and three ordinary Protein sources. The
//! surface itself is now the Karma sand, and economy is one way to use it.
//!
//! Nothing stops the pressure coming back. It is ordinary and recurring — a
//! surface needs something the general primitive does not quite do, and a
//! domain-named shortcut is the shortest path. The correct move is to
//! strengthen the general primitive; this test is here to make the shortcut
//! fail loudly instead of quietly becoming the architecture.
//!
//! So the guard is deliberately *not* about economy. It is about any word that
//! names a use rather than a mechanism. `money` is on the list for the same
//! reason: a currency is a unit like any other, conversion is a rule, and the
//! kernel that once carried a `Money` type beside an identically shaped
//! `Quantity` was carrying a second spelling of one idea.
//!
//! What it checks: no backend crate contains such a name as an *identifier* —
//! module, type, table, column, string literal or serde tag. Prose is exempt.
//! Comments explaining why a domain name was rejected are some of the most
//! useful lines in those files, and a guard that deleted them would be worse
//! than no guard.

use std::path::{Path, PathBuf};

/// Every crate that is not the web surface. A sand may name itself; nothing
/// underneath it may.
const BACKEND_CRATES: [&str; 6] = [
    "nucleus",
    "store",
    "engine",
    "protein",
    "transport",
    "lince",
];

/// Words that name a *use* of the primitives rather than a primitive.
///
/// Each one earned its place by having actually been in the backend. Adding to
/// this list is cheap; the expensive thing is discovering later that a surface's
/// vocabulary has been load-bearing in the kernel for months.
const DOMAIN_NAMES: [&str; 4] = ["economy", "money", "currency", "finance"];

fn workspace_root() -> PathBuf {
    // `crates/web` -> `crates` -> workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the web crate sits two levels below the workspace root")
        .to_path_buf()
}

/// Strip comments so the check reads code, not commentary.
///
/// Deliberately line-based and slightly crude. A `//` inside a string literal
/// would truncate that line early, which can only ever *hide* a match — and a
/// false negative in a guard is a bug we would find, whereas a false positive
/// on the word "economy" in a sentence is a guard nobody keeps.
fn strip_comments(source: &str, line_comment: &str) -> String {
    source
        .lines()
        .map(|line| match line.find(line_comment) {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, out);
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

#[test]
fn no_backend_crate_names_a_domain_it_serves() {
    let root = workspace_root();
    let mut offences: Vec<String> = Vec::new();

    for crate_name in BACKEND_CRATES {
        let crate_dir = root.join("crates").join(crate_name);
        if !crate_dir.exists() {
            continue;
        }

        let mut files = Vec::new();
        collect_files(&crate_dir, "rs", &mut files);
        collect_files(&crate_dir, "sql", &mut files);

        for file in files {
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };
            let comment_marker = if file.extension().is_some_and(|e| e == "sql") {
                "--"
            } else {
                "//"
            };
            let code = strip_comments(&source, comment_marker).to_lowercase();
            for name in DOMAIN_NAMES {
                if !code.contains(name) {
                    continue;
                }
                let line = code
                    .lines()
                    .find(|line| line.contains(name))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                offences.push(format!(
                    "{} [{name}]: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display()
                ));
            }
        }
    }

    assert!(
        offences.is_empty(),
        "These are names for how the primitives get used, not for the primitives. \
         A sand — a bundle of HTML and JS — may say them; nothing underneath it may, \
         as a module, type, table, column, or literal. Whatever the surface needs, it \
         needs as a *general* primitive with a general name. Found:\n  {}",
        offences.join("\n  ")
    );
}

#[test]
fn the_guard_would_actually_catch_a_relapse() {
    // A guard that cannot fail is decoration. This proves the matcher sees a
    // domain-named identifier while ignoring prose about one.
    let relapse = strip_comments("pub mod economy;\n", "//");
    assert!(relapse.to_lowercase().contains("economy"));

    let sql_relapse = strip_comments("CREATE TABLE economy_event (uid TEXT);\n", "--");
    assert!(sql_relapse.to_lowercase().contains("economy"));

    // The type that actually came back once, under a different word.
    let money_relapse = strip_comments("    Money { amount: DecimalValue },\n", "//");
    assert!(money_relapse.to_lowercase().contains("money"));

    // And that the comments explaining why the name was rejected survive.
    let explanation = strip_comments(
        "// domain-specific `economy.add` became the generic `record.add-quantity`\n",
        "//",
    );
    assert!(
        !explanation.to_lowercase().contains("economy"),
        "prose about the rejected name must not trip the guard"
    );
}
