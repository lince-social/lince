use std::collections::BTreeSet;
use std::net::SocketAddr;

use engine::private_admin_catalog::{
    AdminOperation, AuthorityBasis, CONFIGURATION_APPLICATION, CONFIGURATION_NAMESPACE,
    CONFIGURATION_VERSION, CatalogError, ConfigurationApplication, ConfigurationField,
    ConsumerRequirement, ControlEffect, CreationDefaults, CredentialAdmission, ExpectedState,
    MAX_COMMANDS_PER_RECEIPT, MAX_LISTEN_ADDR_BYTES, ReceiptRequirement, RecordEffect,
    TargetRequirement, parse_listen_addr, require_configuration_version, single_receipt_operation,
    standing_property,
};
use nucleus::RecordKind;
use protein::authority::{ExtensionProperty, Property};
use utils::auth::ALL_PERMISSIONS;

#[test]
fn private_admin_catalog_exact_operation_permission_matrix() {
    use AdminOperation::*;
    let expected: &[(AdminOperation, &str, &[&str])] = &[
        (
            CreatePerson,
            "create_person",
            &["record:read", "record:create", "user:create"],
        ),
        (
            SetPersonStanding,
            "set_person_standing",
            &["record:read", "record:update", "user:update"],
        ),
        (
            CreateCredential,
            "create_credential",
            &["record:read", "user:update", "permission:assign"],
        ),
        (
            ReplaceCredential,
            "replace_credential",
            &["record:read", "user:update", "permission:assign"],
        ),
        (
            RemoveCredential,
            "remove_credential",
            &["record:read", "user:update", "permission:assign"],
        ),
        (
            SetPersonRole,
            "set_person_role",
            &["record:read", "user:assign_role", "permission:assign"],
        ),
        (
            SetPersonReadFilter,
            "set_person_read_filter",
            &["record:read", "user:update", "permission:assign"],
        ),
        (
            RevokeDevice,
            "revoke_device",
            &["record:read", "user:update", "permission:assign"],
        ),
        (
            UnrevokeDevice,
            "unrevoke_device",
            &["record:read", "user:update", "permission:assign"],
        ),
        (CreateRole, "create_role", &["record:read", "role:create"]),
        (
            ReplaceRolePolicy,
            "replace_role_policy",
            &["record:read", "role:update", "permission:assign"],
        ),
        (
            ClearRolePolicy,
            "clear_role_policy",
            &["record:read", "role:update", "permission:assign"],
        ),
        (
            ReplaceRolePermissions,
            "replace_role_permissions",
            &["record:read", "permission:assign"],
        ),
        (
            CreateContact,
            "create_contact",
            &["record:read", "record:create", "organ:create"],
        ),
        (
            GrantOrganLogin,
            "grant_organ_login",
            &["record:read", "organ:update", "permission:assign"],
        ),
        (
            ReplaceOrganLogin,
            "replace_organ_login",
            &["record:read", "organ:update", "permission:assign"],
        ),
        (
            RevokeOrganLogin,
            "revoke_organ_login",
            &["record:read", "organ:update", "permission:assign"],
        ),
        (ReadPeople, "read_people", &["record:read", "user:read"]),
        (
            ReadRoleDetails,
            "read_role_details",
            &["record:read", "role:read", "permission:read"],
        ),
        (
            ReadConfiguration,
            "read_configuration",
            &["record:read", "configuration:read"],
        ),
        (
            ReplaceConfiguration,
            "replace_configuration",
            &["record:read", "record:update", "configuration:update"],
        ),
        (
            PreviewPersonAccess,
            "preview_person_access",
            &["record:read", "user:read", "role:read", "permission:read"],
        ),
    ];
    assert_eq!(AdminOperation::ALL.len(), 22);
    assert_eq!(expected.len(), AdminOperation::ALL.len());
    let operations: BTreeSet<_> = AdminOperation::ALL.iter().copied().collect();
    assert_eq!(operations.len(), AdminOperation::ALL.len());
    assert_eq!(
        operations,
        expected
            .iter()
            .map(|entry| entry.0)
            .collect::<BTreeSet<_>>()
    );
    let mut tags = BTreeSet::new();
    for &(operation, tag, permissions) in expected {
        assert!(tags.insert(tag));
        assert_eq!(operation.tag(), tag);
        assert_eq!(tag.parse::<AdminOperation>().unwrap(), operation);
        let actual: BTreeSet<_> = operation
            .permissions()
            .iter()
            .map(|key| key.as_str())
            .collect();
        let expected: BTreeSet<_> = permissions.iter().map(|key| (*key).to_string()).collect();
        assert_eq!(actual, expected, "{tag}");
        assert_eq!(
            operation.specification().permissions,
            operation.permissions()
        );
    }
}

#[test]
fn private_admin_catalog_permissions_are_unique_existing_tuples_without_legacy_extras() {
    let existing: BTreeSet<_> = ALL_PERMISSIONS
        .iter()
        .map(|key| (key.subject, key.action))
        .collect();
    let mut private = BTreeSet::new();
    for &operation in AdminOperation::ALL {
        let permissions = operation.permissions();
        let unique: BTreeSet<_> = permissions
            .iter()
            .map(|key| (key.subject, key.action))
            .collect();
        assert_eq!(permissions.len(), unique.len());
        assert!(unique.contains(&("record", "read")));
        for key in unique {
            assert!(existing.contains(&key), "{key:?}");
            private.insert(key);
        }
    }
    for forbidden in [
        ("record", "delete"),
        ("record", "delete_own"),
        ("user", "update_self"),
        ("user", "delete"),
        ("role", "delete"),
        ("configuration", "create"),
        ("configuration", "delete"),
        ("terminal", "execute"),
        ("karma", "execute"),
        ("transfer", "create"),
        ("file", "download"),
    ] {
        assert!(!private.contains(&forbidden), "{forbidden:?}");
    }
    assert!(private.len() < existing.len());
}

#[test]
fn private_admin_catalog_unknown_tags_aliases_and_implicit_admin_routes_refuse() {
    for tag in [
        "",
        "admin",
        "lince",
        "owner",
        "create_user",
        "create_person ",
        " CreatePerson",
        "CreatePerson",
        "user:create",
        "delete_person",
        "delete_role",
        "update_self",
        "grant_permission",
        "create_permission",
        "enroll_device",
        "register_device",
        "grant_device",
        "enroll_cell",
        "grant_replica",
        "push_ops",
        "discover",
        "publish",
        "terminal",
        "karma",
        "transfer",
        "batch",
        "initialize",
        "restore_record",
        "create_person\0",
        "{\"command\":\"create_person\"}",
    ] {
        assert_eq!(
            tag.parse::<AdminOperation>(),
            Err(CatalogError::UnknownOperation),
            "{tag}"
        );
    }
    assert_eq!(
        "x".repeat(65536).parse::<AdminOperation>(),
        Err(CatalogError::UnknownOperation)
    );
}

#[test]
fn private_admin_catalog_every_entry_requires_current_authority_and_a_separate_consumer() {
    for &operation in AdminOperation::ALL {
        let spec = operation.specification();
        assert_eq!(
            spec.authority,
            AuthorityBasis::CurrentAdmissionAndPolicyFilter
        );
        assert_eq!(
            spec.consumer,
            ConsumerRequirement::SeparatelyEnabledConsumer
        );
        assert_eq!(spec.receipt, operation.receipt());
        if spec.receipt == ReceiptRequirement::SingleCommand {
            assert_ne!(spec.expected, ExpectedState::ReadOnly);
        }
    }
}

#[test]
fn private_admin_catalog_creations_are_complete_proposals_with_default_deny_authority() {
    let person = AdminOperation::CreatePerson.specification();
    assert_eq!(person.target, TargetRequirement::ProposedPerson);
    assert_eq!(person.expected, ExpectedState::RecordAbsence);
    assert_eq!(
        person.record_effect,
        RecordEffect::CompleteProposedCreation {
            kind: RecordKind::Person
        }
    );
    assert_eq!(
        person.creation,
        CreationDefaults::CredentialFreeUnassignedPerson
    );
    assert!(person.control_effects.is_empty());

    let role = AdminOperation::CreateRole.specification();
    assert_eq!(role.target, TargetRequirement::NewRoleName);
    assert_eq!(role.expected, ExpectedState::RoleNameAbsence);
    assert_eq!(role.record_effect, RecordEffect::None);
    assert_eq!(role.control_effects, &[ControlEffect::RoleIdentity]);
    assert_eq!(role.creation, CreationDefaults::EmptyUnassignedRole);

    let contact = AdminOperation::CreateContact.specification();
    assert_eq!(contact.target, TargetRequirement::ProposedContact);
    assert_eq!(contact.expected, ExpectedState::ContactAndRecordAbsence);
    assert_eq!(
        contact.record_effect,
        RecordEffect::CompleteProposedCreation {
            kind: RecordKind::Organ
        }
    );
    assert_eq!(contact.control_effects, &[ControlEffect::ContactBinding]);
    assert_eq!(contact.creation, CreationDefaults::ContactWithoutLogin);
    for &operation in AdminOperation::ALL {
        if ![
            AdminOperation::CreatePerson,
            AdminOperation::CreateRole,
            AdminOperation::CreateContact,
        ]
        .contains(&operation)
        {
            assert_eq!(
                operation.specification().creation,
                CreationDefaults::NotCreation
            );
        }
    }
}

#[test]
fn private_admin_catalog_standing_uses_actual_extension_identity_and_whole_record_revision() {
    let expected = Property::Extension(ExtensionProperty {
        namespace: store::people::NAMESPACE.into(),
        field: store::people::STANDING_KEY.into(),
    });
    assert_eq!(standing_property(), expected);
    let spec = AdminOperation::SetPersonStanding.specification();
    assert_eq!(spec.target, TargetRequirement::ReadablePerson);
    assert_eq!(spec.expected, ExpectedState::WholeRecordRevision);
    assert_eq!(spec.record_effect, RecordEffect::ExactProperty(expected));
    assert!(spec.control_effects.is_empty());
}

#[test]
fn private_admin_catalog_control_effects_never_invent_record_properties_or_side_effects() {
    use AdminOperation::*;
    let cases = [
        (
            CreateCredential,
            ControlEffect::Credential,
            ExpectedState::CredentialAbsentAtAuthenticationGeneration,
        ),
        (
            ReplaceCredential,
            ControlEffect::Credential,
            ExpectedState::CredentialPresentAtAuthenticationGeneration,
        ),
        (
            RemoveCredential,
            ControlEffect::Credential,
            ExpectedState::CredentialPresentAtAuthenticationGeneration,
        ),
        (
            SetPersonRole,
            ControlEffect::PersonRole,
            ExpectedState::PersonAccessRevision,
        ),
        (
            SetPersonReadFilter,
            ControlEffect::PersonReadFilter,
            ExpectedState::PersonAccessRevision,
        ),
        (
            RevokeDevice,
            ControlEffect::DeviceRevocation,
            ExpectedState::ExistingDeviceRevision,
        ),
        (
            UnrevokeDevice,
            ControlEffect::DeviceRevocation,
            ExpectedState::ExistingDeviceRevision,
        ),
        (
            ReplaceRolePolicy,
            ControlEffect::RolePolicy,
            ExpectedState::RolePolicyRevision,
        ),
        (
            ClearRolePolicy,
            ControlEffect::RolePolicy,
            ExpectedState::RolePolicyRevision,
        ),
        (
            ReplaceRolePermissions,
            ControlEffect::RolePermissions,
            ExpectedState::RolePermissionSetRevision,
        ),
        (
            GrantOrganLogin,
            ControlEffect::OrganLogin,
            ExpectedState::LoginAbsentAtBindingGeneration,
        ),
        (
            ReplaceOrganLogin,
            ControlEffect::OrganLogin,
            ExpectedState::LoginPresentAtBindingGeneration,
        ),
        (
            RevokeOrganLogin,
            ControlEffect::OrganLogin,
            ExpectedState::LoginPresentAtBindingGeneration,
        ),
    ];
    for (operation, control, expected) in cases {
        let spec = operation.specification();
        assert_eq!(spec.record_effect, RecordEffect::None);
        assert_eq!(spec.control_effects, &[control]);
        assert_eq!(spec.expected, expected);
        assert_eq!(spec.creation, CreationDefaults::NotCreation);
    }
    assert_ne!(
        ReplaceRolePolicy.specification().expected,
        ReplaceRolePermissions.specification().expected
    );
    assert_ne!(
        SetPersonRole.specification().expected,
        ReplaceCredential.specification().expected
    );
}

#[test]
fn private_admin_catalog_existing_targets_and_preview_keep_explicit_read_requirements() {
    use AdminOperation::*;
    for operation in [
        SetPersonStanding,
        CreateCredential,
        ReplaceCredential,
        RemoveCredential,
        SetPersonReadFilter,
        RevokeDevice,
        UnrevokeDevice,
    ] {
        assert_eq!(
            operation.specification().target,
            TargetRequirement::ReadablePerson
        );
    }
    assert_eq!(
        SetPersonRole.specification().target,
        TargetRequirement::ReadablePersonAndOptionalRole
    );
    for operation in [GrantOrganLogin, ReplaceOrganLogin, RevokeOrganLogin] {
        assert_eq!(
            operation.specification().target,
            TargetRequirement::ReadableContactAndPerson
        );
    }
    for operation in [ReplaceRolePolicy, ClearRolePolicy, ReplaceRolePermissions] {
        assert_eq!(
            operation.specification().target,
            TargetRequirement::ExistingRole
        );
    }
    assert_eq!(
        ReadPeople.specification().target,
        TargetRequirement::ReadablePeople
    );
    assert_eq!(
        ReadRoleDetails.specification().target,
        TargetRequirement::RoleCatalog
    );
    for operation in [ReadConfiguration, ReplaceConfiguration] {
        assert_eq!(
            operation.specification().target,
            TargetRequirement::ReadableHostedCell
        );
    }
    assert_eq!(
        PreviewPersonAccess.specification().target,
        TargetRequirement::ReadablePersonAndPreviewRecord
    );
    for operation in [
        ReadPeople,
        ReadRoleDetails,
        ReadConfiguration,
        PreviewPersonAccess,
    ] {
        let spec = operation.specification();
        assert_eq!(spec.expected, ExpectedState::ReadOnly);
        assert_eq!(spec.record_effect, RecordEffect::None);
        assert!(spec.control_effects.is_empty());
    }
}

#[test]
fn private_admin_catalog_receipts_allow_one_mutation_not_an_authority_borrowing_batch() {
    assert_eq!(MAX_COMMANDS_PER_RECEIPT, 1);
    assert_eq!(
        single_receipt_operation(&[]),
        Err(CatalogError::InvalidReceiptCommands)
    );
    for &operation in AdminOperation::ALL {
        let expected = match operation.receipt() {
            ReceiptRequirement::SingleCommand => Ok(operation),
            ReceiptRequirement::ReadOnly => Err(CatalogError::InvalidReceiptCommands),
        };
        assert_eq!(single_receipt_operation(&[operation]), expected);
        for &other in AdminOperation::ALL {
            assert_eq!(
                single_receipt_operation(&[operation, other]),
                Err(CatalogError::InvalidReceiptCommands)
            );
        }
    }
}

#[test]
fn private_admin_catalog_configuration_properties_are_exact_individual_record_fields() {
    assert_eq!(CONFIGURATION_NAMESPACE, "lince.cell.private");
    let expected = [
        (ConfigurationField::Version, "version", false),
        (ConfigurationField::ListenAddr, "listen_addr", true),
        (
            ConfigurationField::AcceptCredentials,
            "accept_credentials",
            true,
        ),
    ];
    assert_eq!(ConfigurationField::ALL.len(), expected.len());
    let mut properties = BTreeSet::new();
    for (field, name, mutable) in expected {
        assert_eq!(field.name(), name);
        assert_eq!(name.parse::<ConfigurationField>().unwrap(), field);
        assert_eq!(field.mutable_on_replacement(), mutable);
        let property = Property::Extension(ExtensionProperty {
            namespace: "lince.cell.private".into(),
            field: name.into(),
        });
        assert_eq!(field.property(), property);
        assert!(properties.insert(property));
    }
    assert_eq!(properties.len(), 3);
    let listen_only = BTreeSet::from([ConfigurationField::ListenAddr.property()]);
    assert!(!listen_only.contains(&ConfigurationField::Version.property()));
    assert!(!listen_only.contains(&ConfigurationField::AcceptCredentials.property()));
    assert_eq!(
        AdminOperation::ReplaceConfiguration
            .specification()
            .record_effect,
        RecordEffect::ActualPrivateConfigurationProperties
    );
    for field in [
        "",
        "listen",
        "listen_addr ",
        "Version",
        "password",
        "jwt_secret",
        "replication",
        "discovery",
        "organ_uid",
        "namespace",
        "lince.cell.private.listen_addr",
    ] {
        assert_eq!(
            field.parse::<ConfigurationField>(),
            Err(CatalogError::UnknownConfigurationField)
        );
    }
}

#[test]
fn private_admin_catalog_configuration_version_and_restart_never_disable_person_authentication() {
    assert_eq!(CONFIGURATION_VERSION, 1);
    assert_eq!(require_configuration_version(1), Ok(()));
    for version in [0, 2, u16::MAX] {
        assert_eq!(
            require_configuration_version(version),
            Err(CatalogError::UnsupportedConfigurationVersion)
        );
    }
    assert_eq!(
        CONFIGURATION_APPLICATION,
        ConfigurationApplication::ExplicitProcessRestart
    );
    assert_eq!(
        CredentialAdmission::from_accept_credentials(true),
        CredentialAdmission::PasswordOrExplicitGrantedLogin
    );
    assert_eq!(
        CredentialAdmission::from_accept_credentials(false),
        CredentialAdmission::ExplicitGrantedLoginOnly
    );
}

#[test]
fn private_admin_catalog_listen_address_is_bounded_literal_socket_syntax_only() {
    assert_eq!(MAX_LISTEN_ADDR_BYTES, 64);
    for value in [
        "127.0.0.1:0",
        "0.0.0.0:443",
        "[::]:8080",
        "[2001:db8::1]:65535",
    ] {
        let expected: SocketAddr = value.parse().unwrap();
        assert_eq!(parse_listen_addr(value).unwrap(), expected);
    }
    for value in [
        "",
        "localhost:443",
        "127.0.0.1",
        "127.0.0.1:65536",
        "127.0.0.1:-1",
        "http://127.0.0.1:80",
        " 127.0.0.1:80",
        "127.0.0.1:80\n",
        "[::1",
        "false",
        "null",
    ] {
        assert_eq!(
            parse_listen_addr(value),
            Err(CatalogError::InvalidListenAddress)
        );
    }
    assert_eq!(
        parse_listen_addr(&"0".repeat(MAX_LISTEN_ADDR_BYTES + 1)),
        Err(CatalogError::InvalidListenAddress)
    );
}

#[test]
fn private_admin_catalog_refusals_do_not_echo_input_or_claim_an_enabled_handler() {
    let input = "private-secret-not-a-command";
    let operation_error = input.parse::<AdminOperation>().unwrap_err();
    let field_error = input.parse::<ConfigurationField>().unwrap_err();
    let listen_error = parse_listen_addr(input).unwrap_err();
    for error in [
        operation_error,
        field_error,
        listen_error,
        CatalogError::InvalidReceiptCommands,
        CatalogError::UnsupportedConfigurationVersion,
    ] {
        assert!(!error.to_string().contains(input));
        assert!(!format!("{error:?}").contains(input));
    }
}
