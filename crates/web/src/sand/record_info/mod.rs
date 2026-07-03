mod body;
pub(crate) mod script;
pub(crate) mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.record_info";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "record-info.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "◉".into(),
            title: "Record Info".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Sidepanel com os dados do ultimo record clicado em qualquer sand.".into(),
            details: "Consumidor de referencia do ABI de eventos: fica como uma bolinha e expande \
                      em um sidepanel via query SSE quando recebe o evento recordClicked."
                .into(),
            initial_width: 2,
            initial_height: 3,
            requires_server: false,
            permissions: vec!["bridge_state".into(), "read_view_stream".into()],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
