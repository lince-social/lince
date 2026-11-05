use super::{Action, Capability, Description};

const SCRIPT: &str = r#"set -eu
mode=$1
directory=$2
pointer=$3
output=${4-}
command -v grim >/dev/null
geometry=
if [ "$mode" = region ]; then
    command -v slurp >/dev/null
    geometry=$(env -u WAYLAND_SOCKET slurp) || exit 0
    [ -n "$geometry" ] || exit 0
fi
if [ -z "$directory" ]; then
    directory=$(xdg-user-dir PICTURES 2>/dev/null) || directory="$HOME/Pictures"
    [ -n "$directory" ] || directory="$HOME/Pictures"
fi
mkdir -p -- "$directory"
file=$(mktemp "$directory/Screenshot-$(date +%Y-%m-%d_%H-%M-%S)-XXXXXX.png")
trap 'rm -f -- "$file"' EXIT
set --
if [ "$pointer" = true ]; then set -- "$@" -c; fi
if [ "$mode" = region ]; then set -- "$@" -g "$geometry"; fi
if [ "$mode" = screen ] && [ -n "$output" ]; then set -- "$@" -o "$output"; fi
grim "$@" "$file"
trap - EXIT
"#;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    pub directory: String,
    pub show_pointer: bool,
}

impl Options {
    pub fn target_output(action: Action, command: &mut Description, output: &str) {
        if action == Action::ScreenshotScreen && Self::from_command(action, command).is_some() {
            command.arguments.push(output.into());
        }
    }

    pub fn command(&self, action: Action) -> Option<Description> {
        let mode = match action {
            Action::ScreenshotScreen => "screen",
            Action::ScreenshotRegion => "region",
            _ => return None,
        };
        Some(Description {
            program: "/bin/sh".into(),
            arguments: vec![
                "-c".into(),
                SCRIPT.into(),
                "lince-screenshot".into(),
                mode.into(),
                self.directory.clone(),
                self.show_pointer.to_string(),
            ],
            directory: None,
            capabilities: vec![Capability::Capture],
        })
    }

    pub fn from_command(action: Action, command: &Description) -> Option<Self> {
        let options = Self {
            directory: command.arguments.get(4)?.clone(),
            show_pointer: command.arguments.get(5)?.parse().ok()?,
        };
        (options.command(action).as_ref() == Some(command)).then_some(options)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.directory.len() > 4096
            || self.directory.contains('\0')
            || !self.directory.is_empty() && !std::path::Path::new(&self.directory).is_absolute()
        {
            return Err(
                "Choose an absolute screenshot folder, or leave it empty to use Pictures.".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
