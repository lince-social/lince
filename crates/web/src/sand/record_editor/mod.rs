use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.record_editor";

const HTML: &str = include_str!("record_editor.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "✎".into(),
        title: "Record editor".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Collaborative editing of a Record's title and body, with \
            live cursors."
            .into(),
        details: "Joins a Record's shared Loro document over the board socket and \
            edits it as ordinary text. Sends a DELTA since the last send rather than \
            a snapshot per keystroke — both converge, but only one stays cheap as the \
            document grows. Remote changes arrive as a merged document and are \
            imported; Loro dedupes by version vector, so the server's echo of this \
            client's own work is a no-op rather than duplicated text. \
            The loro-crdt bundle and its wasm are served by this Cell under \
            /board/vendor — nothing is fetched from anywhere else. \
            Cursors ride ephemeral lanes and are never written to the Ledger: a \
            caret position is not history. A cursor is only NAMED when the host \
            resolved an identity this viewer is allowed to see; otherwise it renders \
            unnamed, which is the honest answer rather than a missing feature."
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
    LincePackage::new(Some("record_editor.html".into()), manifest(), HTML)
        .expect("record editor official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::{HTML, manifest};

    #[test]
    fn the_editor_uses_the_shared_collab_element() {
        assert!(HTML.contains("/board/collab-editor.js"));
        assert!(HTML.contains("createCollabEditor({"));
        assert!(HTML.contains("bindInputs(editor,"));
        assert!(HTML.contains("/board/vendor/loro-index.js"));
        assert!(
            !HTML.contains("https://"),
            "a sand must never reach off this Cell for code"
        );
    }

    #[test]
    fn cursors_ride_lanes_and_are_named_only_when_the_host_says_so() {
        assert!(HTML.contains("onPresence:"));
        assert!(
            !HTML.contains("H.emit(\"record:\""),
            "presence is the shared element's job, not this sand's"
        );
        assert!(
            !HTML.contains("action: \"create-fact\""),
            "presence must never reach the Ledger"
        );
        assert!(HTML.contains("peer.name || \"someone\""));
        assert!(
            !HTML.contains("name: from"),
            "a connection id is not a name and must never be rendered as one"
        );
        assert!(HTML.contains("peer.focus !== peer.anchor"));
        assert!(HTML.contains("peer.idle"));
        assert_eq!(
            manifest().permissions,
            vec!["bridge_state", "protein_subscribe", "act"]
        );
    }

    #[test]
    fn a_locked_description_is_not_opened_for_collaborative_editing() {
        assert!(HTML.contains("/board/vault.js"));
        assert!(HTML.contains("Vault.isLocked(row.body || \"\")"));
        assert!(HTML.contains("unlock it in the Record sand"));
    }
}
