// non-lean-yet
// Scope: Define the shared rule interface and registered build rules.
// BeautifulRust (baseline lines):
// - [manual: §4 API, §10 Documentation] Sites 5–29. Document the reported source line
//   numbering and the mutability contract for a rule's suggested rewrite. The rule registry
//   is shared by Cargo, the checker, and the fixer. Keep the disabled comment rule out of it.
// - [manual: §6 Idioms] Sites 14–16. Compare the OsStr extension directly with the Rust
//   extension literal. The legacy scanner duplicates this default.

pub mod no_comments;

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub complaint: String,
}

pub trait Rule: Send + Sync {
    fn name(&self) -> &'static str;

    fn wants(&self, path: &Path) -> bool {
        path.extension().and_then(|value| value.to_str()) == Some("rs")
    }

    fn inspect(&self, source: &str) -> Vec<Finding>;

    fn fix(&self, _source: &str) -> Option<String> {
        None
    }

    fn remedy(&self) -> &'static str;
}

pub fn enforced() -> Vec<Box<dyn Rule>> {
    Vec::new()
}
