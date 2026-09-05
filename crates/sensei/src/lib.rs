pub mod rules;

mod lesson;

pub use lesson::{
    Mended, examine, mend, teach, teach_workspace, workspace_root, workspace_sources,
};
pub use rules::{Finding, Rule};
