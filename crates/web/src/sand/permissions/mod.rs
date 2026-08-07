use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.permissions";

// Roles/users/permission-grant CRUD, driven through Protein (sources "auth"
// and Person records) + Actions (create-role, create-user, assign-role,
// assign-user-person, grant-permission, revoke-permission). The auth system
// predates and is deliberately NOT a Ledger record type (`store::auth`'s own
// doc comment),
// but reads/writes it the same way every other sand does. The engine
// enforces every one of those actions against the matching permission key
// (role:create, user:create, user:assign_role,
// permission:assign); this sand is a UI on top, not a second enforcement
// point.
const HTML: &str = include_str!("permissions.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "🔑".into(),
        title: "Roles & Permissions".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Manage users, Person identities, roles, and permissions.".into(),
        details:
            "Admin surface for the role/permission/user system: create roles and users, bind each app user to the Ledger Person they represent, assign roles, and toggle permission grants. Reads through Protein and writes through Actions; the engine enforces every operation server-side."
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
