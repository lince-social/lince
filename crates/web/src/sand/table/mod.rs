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
            description: "Server-rendered table for arbitrary view snapshots.".into(),
            details:
                "Reads server_id and view_id from the host, streams datastar HTML fragments from the backend, keeps the table markup on the server, and opens a minimal create panel when you want to add a new row."
                    .into(),
            initial_width: 7,
            initial_height: 5,
            requires_server: true,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_table".into(),
            ],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
