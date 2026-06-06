mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.transfer";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "transfer.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "T".into(),
            title: "Transfer".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Manual signed Transfer workflow.".into(),
            details:
                "Local-coordinator Transfer flow for signed proposal packages between Lince Organs."
                    .into(),
            initial_width: 8,
            initial_height: 6,
            requires_server: false,
            permissions: vec![
                "bridge_state".into(),
                "read_transfer_stream".into(),
                "write_transfer".into(),
            ],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
