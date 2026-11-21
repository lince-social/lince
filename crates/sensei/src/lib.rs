// non-lean-yet
// Scope: Expose the build hook, CLI operations, and rule contracts.
// BeautifulRust (baseline lines):
// - [manual: §4 API, §10 Documentation] Sites 1–8. Document which entry points fail Cargo
//   builds and which return reports or rewrite files. Check consumers before narrowing the
//   Rule and Finding re-exports or the unused workspace-wide build hook.

pub mod rules;

mod lesson;

pub use lesson::{
    Mended, examine, mend, teach, teach_workspace, workspace_root, workspace_sources,
};
pub use rules::{Finding, Rule};
