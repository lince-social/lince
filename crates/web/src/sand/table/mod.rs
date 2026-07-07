mod body;
mod script;
mod style;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.view_table_editor";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "table.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▦".into(),
            title: "Table".into(),
            author: "Lince Labs".into(),
            version: "1.0.0".into(),
            description: "Live records table over Protein reads and typed Actions.".into(),
            details:
                "Subscribes to a records Protein through the widget bridge, renders the table client-side, and creates/edits/deletes rows with typed Actions (create-record, set-slug, edit-record-text, set-quantity, deactivate). No server view stream required."
                    .into(),
            initial_width: 7,
            initial_height: 5,
            requires_server: false,
            permissions: vec![
                "bridge_state".into(),
                "protein_subscribe".into(),
                "act".into(),
            ],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
