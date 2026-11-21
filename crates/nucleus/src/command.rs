// non-lean-yet
// Scope: enum Command, struct CommandResponse.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 5.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 16.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 9, 10, 11, 16, 40, 41, 42, 43, 45.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites validate:
//   String (16).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    Shell {
        script: String,
    },
    Process {
        program: String,
        arguments: Vec<String>,
    },
}

impl Command {
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Shell { script } => {
                !script.trim().is_empty() && script.len() <= 16_384 && !script.contains('\0')
            }
            Self::Process { program, arguments } => {
                !program.trim().is_empty()
                    && program.len() <= 4096
                    && !program.contains('\0')
                    && arguments.len() <= 128
                    && arguments
                        .iter()
                        .all(|argument| argument.len() <= 4096 && !argument.contains('\0'))
                    && arguments.iter().map(String::len).sum::<usize>() <= 16_384
            }
        };
        valid.then_some(()).ok_or_else(|| {
            "Use a nonempty command with at most 16 KiB of configuration and 128 arguments".into()
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandResponse {
    pub command: String,
    pub ok: bool,
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}
