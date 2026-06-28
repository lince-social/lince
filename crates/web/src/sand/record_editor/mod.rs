mod body;
pub(crate) mod script;
pub(crate) mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.record_editor";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "record-editor.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "✎".into(),
            title: "Record editor".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Reusable editor for Record head and body.".into(),
            details: "Shared Record text editor used standalone or embedded by other sands.".into(),
            initial_width: 4,
            initial_height: 4,
            requires_server: true,
            permissions: vec!["write_records".into()],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
