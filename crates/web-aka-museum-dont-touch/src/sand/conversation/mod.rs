use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.conversation";

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
            `create-message`; a new topic uses `open-thread` and needs no new grant, \
            because it is born inside the conversation that was already shared. \
            Pending invites appear here and in board notifications: accepting keeps a \
            copy of what was offered and nothing else — it does not promote the \
            sender to known or enable the general feed. Verification keys retained \
            for that authenticated unknown identity are not a friendship decision. \
            Declining revokes the offered grant, which is what frees the \
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
    fn conversations_read_records_and_acknowledge_invites_through_the_host() {
        assert!(HTML.contains("kind_eq: \"conversation\""));
        assert!(HTML.contains("kind_eq: \"thread_invite\""));
        assert!(HTML.contains("action: \"create-message\""));
        assert!(HTML.contains("action: \"open-thread\""));
        assert!(HTML.contains("/host/notifications/${encodeURIComponent(invite.uid)}/accept"));
        assert!(HTML.contains("/host/notifications/${encodeURIComponent(invite.uid)}/decline"));
        assert_eq!(
            manifest().permissions,
            vec!["bridge_state", "protein_subscribe", "act"]
        );
    }

    #[test]
    fn an_invite_shows_the_organ_that_asked_not_a_name_it_chose() {
        assert!(HTML.contains("from_organ"));
        assert!(HTML.contains("Someone wants to talk"));
    }
}
