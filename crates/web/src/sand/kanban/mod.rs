mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.kanban_record_view";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "kanban-record-view.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▥".into(),
            title: "Kanban".into(),
            author: "Lince Labs".into(),
            version: "2.0.0".into(),
            description: "Live kanban board over Protein reads and typed Actions.".into(),
            details:
                "Stage 8b rebuild: subscribes to a records Protein through the widget bridge, buckets records into columns by a configurable field (default: concept), renders the board client-side, moves cards between columns with set-concept, and creates/edits/deletes cards with typed Actions. No server view stream required. Clicking a card emits the recordClicked ABI event so a Record Info sand can show its details."
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
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
