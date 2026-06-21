mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.role_access";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "role-access.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "ACL".into(),
            title: "Role Access".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Admin control surface for users, roles, and role permissions.".into(),
            details: "Lists users, roles, hardcoded permissions, and role assignments through the host-mediated backend API. Admins can create roles, create users, assign user roles, and toggle permissions on roles.".into(),
            initial_width: 8,
            initial_height: 6,
            requires_server: true,
            permissions: vec!["bridge_state".into(), "write_table".into()],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
