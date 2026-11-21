use serde::{Deserialize, Serialize};

const MAX_CONFIGURATION_BYTES: usize = 16 * 1024;
const MAX_PROCESS_VALUE_BYTES: usize = 4 * 1024;
const MAX_ARGUMENTS: usize = 128;

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
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
    /// Checks that the command text and any process arguments fit the configuration limits.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for missing command text, embedded NULs, or exceeded limits.
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Shell { script } => valid_command_text(script, MAX_CONFIGURATION_BYTES),
            Self::Process { program, arguments } => {
                valid_command_text(program, MAX_PROCESS_VALUE_BYTES) && arguments_fit(arguments)
            }
        };
        valid.then_some(()).ok_or_else(|| {
            "Use a nonempty command with at most 16 KiB of configuration and 128 arguments".into()
        })
    }
}

fn valid_command_text(text: &str, max_bytes: usize) -> bool {
    text.len() <= max_bytes && !text.trim().is_empty() && !text.contains('\0')
}

fn arguments_fit(arguments: &[String]) -> bool {
    arguments.len() <= MAX_ARGUMENTS
        && arguments
            .iter()
            .try_fold(MAX_CONFIGURATION_BYTES, |remaining, argument| {
                let bytes = argument.len();
                if bytes > MAX_PROCESS_VALUE_BYTES || argument.contains('\0') {
                    return None;
                }
                remaining.checked_sub(bytes)
            })
            .is_some()
}

#[expect(missing_docs, reason = "Rustdoc feeds the published response schema")]
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandResponse {
    pub command: String,
    pub ok: bool,
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}
