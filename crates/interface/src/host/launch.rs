use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Clipboard,
    WindowList,
    WindowControl,
    OutputControl,
    Capture,
    InputMethod,
    DesktopControl,
    Launch,
}

impl Capability {
    pub const ALL: [Self; 8] = [Self::Clipboard, Self::WindowList, Self::WindowControl, Self::OutputControl, Self::Capture, Self::InputMethod, Self::DesktopControl, Self::Launch];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub directory: Option<PathBuf>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

impl Description {
    pub fn valid(&self) -> bool {
        !self.program.is_empty()
            && self.program.len() <= 4096
            && !self.program.contains('\0')
            && self.capabilities.len() <= Capability::ALL.len()
            && self.capabilities.iter().enumerate().all(|(index, capability)| !self.capabilities[..index].contains(capability))
            && self.arguments.len() <= 4096
            && self
                .arguments
                .iter()
                .all(|argument| !argument.contains('\0'))
            && self.arguments.iter().map(String::len).sum::<usize>() <= 262144
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_connection_grants_are_bounded_and_cannot_be_duplicated() {
        let mut description = Description { program: "fixture-helper".into(), arguments: Vec::new(), directory: None, capabilities: Capability::ALL.to_vec() };
        assert!(description.valid());
        description.capabilities.push(Capability::DesktopControl);
        assert!(!description.valid());
        description.capabilities = vec![Capability::Launch, Capability::Launch];
        assert!(!description.valid());
        description.capabilities = vec![Capability::DesktopControl, Capability::Launch];
        assert!(description.valid());
    }
}
