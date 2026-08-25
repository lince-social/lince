pub struct FrameAssembly<T> {
    entries: Vec<FrameEntry<T>>,
}

pub struct FrameSubmission<T> {
    entries: Vec<FrameEntry<T>>,
}

struct FrameEntry<T> {
    participant: String,
    commands: T,
}

impl<T> Default for FrameAssembly<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FrameAssembly<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn contribute(&mut self, participant: impl Into<String>, commands: T) {
        self.entries.push(FrameEntry {
            participant: participant.into(),
            commands,
        });
    }

    pub fn seal(mut self, final_compositor: impl Into<String>, commands: T) -> FrameSubmission<T> {
        self.contribute(final_compositor, commands);
        FrameSubmission {
            entries: self.entries,
        }
    }
}

impl<T> FrameSubmission<T> {
    pub fn command_buffer_count(&self) -> u64 {
        self.entries.len() as u64
    }

    pub fn participants(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.participant.clone())
            .collect()
    }

    pub fn into_commands(self) -> impl ExactSizeIterator<Item = T> {
        self.entries.into_iter().map(|entry| entry.commands)
    }
}

#[cfg(test)]
mod tests {
    use super::FrameAssembly;

    #[test]
    fn final_compositor_is_sealed_after_every_contribution() {
        let mut assembly = FrameAssembly::new();
        assembly.contribute("bevy-world", 10);
        assembly.contribute("html-surface", 20);
        let submission = assembly.seal("lince-compositor", 30);

        assert_eq!(submission.command_buffer_count(), 3);
        assert_eq!(
            submission.participants(),
            vec!["bevy-world", "html-surface", "lince-compositor"]
        );
        assert_eq!(submission.into_commands().collect::<Vec<_>>(), [10, 20, 30]);
    }
}
