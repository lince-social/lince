mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.trail_relation";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "trail_relation.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "🧭".into(),
            title: "Trail Relation".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Per-user record trails with root discovery, sync, and progression.".into(),
            details: "Search records by assignee/category/head, bind or create a trail root, inspect overwrite rules from sync metadata, and progress the copied tree with host-backed updates.".into(),
            initial_width: 7,
            initial_height: 6,
            requires_server: true,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
                "write_table".into(),
            ],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
