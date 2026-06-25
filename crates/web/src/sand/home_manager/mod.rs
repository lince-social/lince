mod body;
mod script;
mod style;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.home_manager";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "home-manager.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "⌂".into(),
            title: "Home Manager".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Personal Home Manager control board for modules, packages, services, and generations.".into(),
            details: "Tracks a local Home Manager profile plan inside the widget, including module toggles, package intent, service health, activation steps, and rollback notes. State is stored per widget instance.".into(),
            initial_width: 8,
            initial_height: 6,
            requires_server: false,
            permissions: vec!["bridge_state".into()],
        },
        head_links: vec![],
        inline_styles: style::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
