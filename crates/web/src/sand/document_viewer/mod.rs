mod body;
mod script;
mod style;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.bucket_image_view";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "bucket-image-view.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "◧".into(),
            title: "Document Viewer".into(),
            author: "Lince Labs".into(),
            version: "0.2.0".into(),
            description: "Views PDF, JPEG, and PNG documents from disk, a Lince bucket, or an internet URL.".into(),
            details: "Choose a local path, a Lince bucket object path, or an HTTP URL. The widget stores the selected source and path in card state and renders supported PDF, JPEG, and PNG files.".into(),
            initial_width: 5,
            initial_height: 5,
            requires_server: false,
            permissions: vec!["bridge_state".into(), "read_files".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
