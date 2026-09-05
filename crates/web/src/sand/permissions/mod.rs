use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.permissions";

const HTML: &str = include_str!("permissions.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "🔑".into(),
        title: "Roles & Permissions".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
        description: "Manage users, Person identities, roles, permissions, and who still uses this Organ.".into(),
        details:
            "Admin surface for the role/permission/user system: create roles and users, bind each app user to the Ledger Person they represent, assign roles, toggle permission grants, and deactivate someone who has stopped using Lince — reversibly, without deleting their Person or their history. Reads through Protein and writes through Actions; the engine enforces every operation server-side."
                .into(),
        initial_width: 5,
        initial_height: 5,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("permissions.html".into()), manifest(), HTML)
        .expect("permissions official sand should render as a valid package")
}
