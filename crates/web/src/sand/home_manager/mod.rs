mod body;
mod knowledge_base;
mod script;
mod style;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.home_manager";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "home-manager.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "H".into(),
            title: "Home Manager".into(),
            author: "Lince Labs".into(),
            version: "0.2.0".into(),
            description: "Tabbed home operations sand with nutrition planning and bills.".into(),
            details: "Nutrition embeds a Brazilian-guide-based food catalog, custom alimentum records, marmita planning, shopping lists, prices, and a lowest-price optimizer. Bills is a thin tab for a later pass.".into(),
            initial_width: 10,
            initial_height: 8,
            requires_server: false,
            permissions: vec!["bridge_state".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
