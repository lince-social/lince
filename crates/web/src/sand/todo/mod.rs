mod body;
mod script;
mod style;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.view_todo_editor";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "todo.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▦".into(),
            title: "Todo".into(),
            author: "Lince Labs".into(),
            version: "1.0.0".into(),
            description: "Server-rendered todo table for arbitrary view snapshots.".into(),
            details:
                "Reads server_id and view_id from the host, streams datastar HTML fragments from the backend, and keeps the todo markup on the server instead of building it in JavaScript."
                    .into(),
            initial_width: 7,
            initial_height: 5,
            requires_server: true,
            permissions: vec!["bridge_state".into(), "read_view_stream".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
