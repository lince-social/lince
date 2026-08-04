use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.conversation";

// Reading and answering conversations (Ontology §11 "Threads"). Nothing here
// is a messaging subsystem: a conversation, a topic, and a message are all
// Records, and this sand is a view over them plus the two Actions that answer
// an invite.
const HTML: &str = include_str!("conversation.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "✉".into(),
        title: "Conversations".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Read and answer conversations, topics, and messages, and \
            decide who may open one."
            .into(),
        details: "Three levels of ordinary Records — conversation, thread, message — \
            joined by `in` Assertions and assembled here from a `links` include, \
            since Protein has no 'linked to X' predicate. Sending uses \
            `send-message`; a new topic uses `open-thread` and needs no new grant, \
            because it is born inside the conversation that was already shared. \
            Pending invites appear at the top: accepting keeps a copy of what was \
            offered and nothing else — it sets no trust, enables no sync, and adopts \
            no key, because agreeing to read what someone sends is not deciding who \
            they are. Declining revokes the offered grant, which is what frees the \
            sender to ask once more; there is deliberately no 'dismiss' that would \
            leave them waiting forever."
            .into(),
        initial_width: 4,
        initial_height: 5,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("conversation.html".into()), manifest(), HTML)
        .expect("conversation official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::{HTML, manifest};

    #[test]
    fn conversations_read_records_and_answer_invites_through_actions() {
        assert!(HTML.contains("kind_eq: \"conversation\""));
        assert!(HTML.contains("kind_eq: \"thread_invite\""));
        assert!(HTML.contains("action: \"send-message\""));
        assert!(HTML.contains("action: \"open-thread\""));
        assert!(HTML.contains("action: \"accept-thread-invite\""));
        assert!(HTML.contains("action: \"decline-thread-invite\""));
        // No camera, no terminal, no host state beyond the board's own: a
        // conversation view is Protein plus Actions and nothing more.
        assert_eq!(
            manifest().permissions,
            vec!["bridge_state", "protein_subscribe", "act"]
        );
    }

    /// An invite must render the Organ uid the connection PROVED, never a
    /// display name the sender chose. Showing a claim as a name is how the
    /// wrong person gets trusted.
    #[test]
    fn an_invite_shows_the_organ_that_asked_not_a_name_it_chose() {
        assert!(HTML.contains("from_organ"));
        assert!(HTML.contains("Someone wants to talk"));
    }
}
