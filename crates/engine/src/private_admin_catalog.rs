// non-lean-yet
// Scope: const MAX_COMMANDS_PER_RECEIPT, const CONFIGURATION_NAMESPACE, const
//   CONFIGURATION_VERSION, const MAX_LISTEN_ADDR_BYTES.
// BeautifulRust (baseline lines):
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   from_accept_credentials:accept_credentials (460).
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 445.
// - [Clippy probe: clippy::exhaustive_enums] Review whether public API evolution requires
//   non_exhaustive. Sites 34, 57, 83, 88, 93, 108, 125, 133, 146, 154, 387, 449, 454.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 445.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 370, 431, 439.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   9, 10, 11, 12, 13, 34, 35, 36, 37, 38, 39, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69,
//   70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 83, 84, 88, 89, 93, 94, 95, 96, 97, 98, 99, 100, 101,
//   102, 103, 104, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 125, 126,
//   127, 128, 129, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 146, 147, 148, 149, 150, 154,
//   155, 156, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 173, 198, 225, 251, 261, 370, 379,
//   387, 388, 389, 390, 394, 396, 404, 411, 431, 439, 449, 450, 454, 455, 456, 460.

use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

use nucleus::RecordKind;
use protein::authority::{ExtensionProperty, Property};
use utils::auth::PermissionKey;

pub const MAX_COMMANDS_PER_RECEIPT: usize = 1;
pub const CONFIGURATION_NAMESPACE: &str = "lince.cell.private";
pub const CONFIGURATION_VERSION: u16 = 1;
pub const MAX_LISTEN_ADDR_BYTES: usize = 64;
pub const CONFIGURATION_APPLICATION: ConfigurationApplication =
    ConfigurationApplication::ExplicitProcessRestart;

const READ: PermissionKey = PermissionKey::new("record", "read");
const CREATE: PermissionKey = PermissionKey::new("record", "create");
const UPDATE: PermissionKey = PermissionKey::new("record", "update");
const ASSIGN: PermissionKey = PermissionKey::new("permission", "assign");
const USER_CREATE: PermissionKey = PermissionKey::new("user", "create");
const USER_READ: PermissionKey = PermissionKey::new("user", "read");
const USER_UPDATE: PermissionKey = PermissionKey::new("user", "update");
const USER_ASSIGN_ROLE: PermissionKey = PermissionKey::new("user", "assign_role");
const ROLE_CREATE: PermissionKey = PermissionKey::new("role", "create");
const ROLE_READ: PermissionKey = PermissionKey::new("role", "read");
const ROLE_UPDATE: PermissionKey = PermissionKey::new("role", "update");
const PERMISSION_READ: PermissionKey = PermissionKey::new("permission", "read");
const ORGAN_CREATE: PermissionKey = PermissionKey::new("organ", "create");
const ORGAN_UPDATE: PermissionKey = PermissionKey::new("organ", "update");
const CONFIGURATION_READ: PermissionKey = PermissionKey::new("configuration", "read");
const CONFIGURATION_UPDATE: PermissionKey = PermissionKey::new("configuration", "update");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    UnknownOperation,
    InvalidReceiptCommands,
    UnknownConfigurationField,
    UnsupportedConfigurationVersion,
    InvalidListenAddress,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownOperation => "administrative operation is unsupported",
            Self::InvalidReceiptCommands => "receipt requires one administrative mutation",
            Self::UnknownConfigurationField => "private configuration field is unsupported",
            Self::UnsupportedConfigurationVersion => "private configuration version is unsupported",
            Self::InvalidListenAddress => "private listen address is invalid",
        })
    }
}

impl std::error::Error for CatalogError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdminOperation {
    CreatePerson,
    SetPersonStanding,
    CreateCredential,
    ReplaceCredential,
    RemoveCredential,
    SetPersonRole,
    SetPersonReadFilter,
    RevokeDevice,
    UnrevokeDevice,
    CreateRole,
    ReplaceRolePolicy,
    ClearRolePolicy,
    ReplaceRolePermissions,
    CreateContact,
    GrantOrganLogin,
    ReplaceOrganLogin,
    RevokeOrganLogin,
    ReadPeople,
    ReadRoleDetails,
    ReadConfiguration,
    ReplaceConfiguration,
    PreviewPersonAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityBasis {
    CurrentAdmissionAndPolicyFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerRequirement {
    SeparatelyEnabledConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRequirement {
    ProposedPerson,
    ReadablePerson,
    ReadablePersonAndOptionalRole,
    NewRoleName,
    ExistingRole,
    ProposedContact,
    ReadableContactAndPerson,
    ReadablePeople,
    RoleCatalog,
    ReadableHostedCell,
    ReadablePersonAndPreviewRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedState {
    ReadOnly,
    RecordAbsence,
    WholeRecordRevision,
    CredentialAbsentAtAuthenticationGeneration,
    CredentialPresentAtAuthenticationGeneration,
    PersonAccessRevision,
    RoleNameAbsence,
    RolePolicyRevision,
    RolePermissionSetRevision,
    ExistingDeviceRevision,
    ContactAndRecordAbsence,
    LoginAbsentAtBindingGeneration,
    LoginPresentAtBindingGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordEffect {
    None,
    CompleteProposedCreation { kind: RecordKind },
    ExactProperty(Property),
    ActualPrivateConfigurationProperties,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ControlEffect {
    Credential,
    PersonRole,
    PersonReadFilter,
    DeviceRevocation,
    RoleIdentity,
    RolePolicy,
    RolePermissions,
    ContactBinding,
    OrganLogin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationDefaults {
    NotCreation,
    CredentialFreeUnassignedPerson,
    EmptyUnassignedRole,
    ContactWithoutLogin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptRequirement {
    ReadOnly,
    SingleCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSpec {
    pub permissions: &'static [PermissionKey],
    pub authority: AuthorityBasis,
    pub consumer: ConsumerRequirement,
    pub target: TargetRequirement,
    pub expected: ExpectedState,
    pub record_effect: RecordEffect,
    pub control_effects: &'static [ControlEffect],
    pub creation: CreationDefaults,
    pub receipt: ReceiptRequirement,
}

impl AdminOperation {
    pub const ALL: &'static [Self] = &[
        Self::CreatePerson,
        Self::SetPersonStanding,
        Self::CreateCredential,
        Self::ReplaceCredential,
        Self::RemoveCredential,
        Self::SetPersonRole,
        Self::SetPersonReadFilter,
        Self::RevokeDevice,
        Self::UnrevokeDevice,
        Self::CreateRole,
        Self::ReplaceRolePolicy,
        Self::ClearRolePolicy,
        Self::ReplaceRolePermissions,
        Self::CreateContact,
        Self::GrantOrganLogin,
        Self::ReplaceOrganLogin,
        Self::RevokeOrganLogin,
        Self::ReadPeople,
        Self::ReadRoleDetails,
        Self::ReadConfiguration,
        Self::ReplaceConfiguration,
        Self::PreviewPersonAccess,
    ];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::CreatePerson => "create_person",
            Self::SetPersonStanding => "set_person_standing",
            Self::CreateCredential => "create_credential",
            Self::ReplaceCredential => "replace_credential",
            Self::RemoveCredential => "remove_credential",
            Self::SetPersonRole => "set_person_role",
            Self::SetPersonReadFilter => "set_person_read_filter",
            Self::RevokeDevice => "revoke_device",
            Self::UnrevokeDevice => "unrevoke_device",
            Self::CreateRole => "create_role",
            Self::ReplaceRolePolicy => "replace_role_policy",
            Self::ClearRolePolicy => "clear_role_policy",
            Self::ReplaceRolePermissions => "replace_role_permissions",
            Self::CreateContact => "create_contact",
            Self::GrantOrganLogin => "grant_organ_login",
            Self::ReplaceOrganLogin => "replace_organ_login",
            Self::RevokeOrganLogin => "revoke_organ_login",
            Self::ReadPeople => "read_people",
            Self::ReadRoleDetails => "read_role_details",
            Self::ReadConfiguration => "read_configuration",
            Self::ReplaceConfiguration => "replace_configuration",
            Self::PreviewPersonAccess => "preview_person_access",
        }
    }

    pub const fn permissions(self) -> &'static [PermissionKey] {
        match self {
            Self::CreatePerson => &[READ, CREATE, USER_CREATE],
            Self::SetPersonStanding => &[READ, UPDATE, USER_UPDATE],
            Self::CreateCredential
            | Self::ReplaceCredential
            | Self::RemoveCredential
            | Self::SetPersonReadFilter
            | Self::RevokeDevice
            | Self::UnrevokeDevice => &[READ, USER_UPDATE, ASSIGN],
            Self::SetPersonRole => &[READ, USER_ASSIGN_ROLE, ASSIGN],
            Self::CreateRole => &[READ, ROLE_CREATE],
            Self::ReplaceRolePolicy | Self::ClearRolePolicy => &[READ, ROLE_UPDATE, ASSIGN],
            Self::ReplaceRolePermissions => &[READ, ASSIGN],
            Self::CreateContact => &[READ, CREATE, ORGAN_CREATE],
            Self::GrantOrganLogin | Self::ReplaceOrganLogin | Self::RevokeOrganLogin => {
                &[READ, ORGAN_UPDATE, ASSIGN]
            }
            Self::ReadPeople => &[READ, USER_READ],
            Self::ReadRoleDetails => &[READ, ROLE_READ, PERMISSION_READ],
            Self::ReadConfiguration => &[READ, CONFIGURATION_READ],
            Self::ReplaceConfiguration => &[READ, UPDATE, CONFIGURATION_UPDATE],
            Self::PreviewPersonAccess => &[READ, USER_READ, ROLE_READ, PERMISSION_READ],
        }
    }

    pub const fn receipt(self) -> ReceiptRequirement {
        match self {
            Self::ReadPeople
            | Self::ReadRoleDetails
            | Self::ReadConfiguration
            | Self::PreviewPersonAccess => ReceiptRequirement::ReadOnly,
            _ => ReceiptRequirement::SingleCommand,
        }
    }

    pub fn specification(self) -> OperationSpec {
        let mut spec = OperationSpec {
            permissions: self.permissions(),
            authority: AuthorityBasis::CurrentAdmissionAndPolicyFilter,
            consumer: ConsumerRequirement::SeparatelyEnabledConsumer,
            target: TargetRequirement::ReadablePerson,
            expected: ExpectedState::ReadOnly,
            record_effect: RecordEffect::None,
            control_effects: &[],
            creation: CreationDefaults::NotCreation,
            receipt: self.receipt(),
        };
        match self {
            Self::CreatePerson => {
                spec.target = TargetRequirement::ProposedPerson;
                spec.expected = ExpectedState::RecordAbsence;
                spec.record_effect = RecordEffect::CompleteProposedCreation {
                    kind: RecordKind::Person,
                };
                spec.creation = CreationDefaults::CredentialFreeUnassignedPerson;
            }
            Self::SetPersonStanding => {
                spec.expected = ExpectedState::WholeRecordRevision;
                spec.record_effect = RecordEffect::ExactProperty(standing_property());
            }
            Self::CreateCredential => {
                spec.expected = ExpectedState::CredentialAbsentAtAuthenticationGeneration;
                spec.control_effects = &[ControlEffect::Credential];
            }
            Self::ReplaceCredential | Self::RemoveCredential => {
                spec.expected = ExpectedState::CredentialPresentAtAuthenticationGeneration;
                spec.control_effects = &[ControlEffect::Credential];
            }
            Self::SetPersonRole => {
                spec.target = TargetRequirement::ReadablePersonAndOptionalRole;
                spec.expected = ExpectedState::PersonAccessRevision;
                spec.control_effects = &[ControlEffect::PersonRole];
            }
            Self::SetPersonReadFilter => {
                spec.expected = ExpectedState::PersonAccessRevision;
                spec.control_effects = &[ControlEffect::PersonReadFilter];
            }
            Self::RevokeDevice | Self::UnrevokeDevice => {
                spec.expected = ExpectedState::ExistingDeviceRevision;
                spec.control_effects = &[ControlEffect::DeviceRevocation];
            }
            Self::CreateRole => {
                spec.target = TargetRequirement::NewRoleName;
                spec.expected = ExpectedState::RoleNameAbsence;
                spec.control_effects = &[ControlEffect::RoleIdentity];
                spec.creation = CreationDefaults::EmptyUnassignedRole;
            }
            Self::ReplaceRolePolicy | Self::ClearRolePolicy => {
                spec.target = TargetRequirement::ExistingRole;
                spec.expected = ExpectedState::RolePolicyRevision;
                spec.control_effects = &[ControlEffect::RolePolicy];
            }
            Self::ReplaceRolePermissions => {
                spec.target = TargetRequirement::ExistingRole;
                spec.expected = ExpectedState::RolePermissionSetRevision;
                spec.control_effects = &[ControlEffect::RolePermissions];
            }
            Self::CreateContact => {
                spec.target = TargetRequirement::ProposedContact;
                spec.expected = ExpectedState::ContactAndRecordAbsence;
                spec.record_effect = RecordEffect::CompleteProposedCreation {
                    kind: RecordKind::Organ,
                };
                spec.control_effects = &[ControlEffect::ContactBinding];
                spec.creation = CreationDefaults::ContactWithoutLogin;
            }
            Self::GrantOrganLogin => {
                spec.target = TargetRequirement::ReadableContactAndPerson;
                spec.expected = ExpectedState::LoginAbsentAtBindingGeneration;
                spec.control_effects = &[ControlEffect::OrganLogin];
            }
            Self::ReplaceOrganLogin | Self::RevokeOrganLogin => {
                spec.target = TargetRequirement::ReadableContactAndPerson;
                spec.expected = ExpectedState::LoginPresentAtBindingGeneration;
                spec.control_effects = &[ControlEffect::OrganLogin];
            }
            Self::ReadPeople => spec.target = TargetRequirement::ReadablePeople,
            Self::ReadRoleDetails => spec.target = TargetRequirement::RoleCatalog,
            Self::ReadConfiguration => spec.target = TargetRequirement::ReadableHostedCell,
            Self::ReplaceConfiguration => {
                spec.target = TargetRequirement::ReadableHostedCell;
                spec.expected = ExpectedState::WholeRecordRevision;
                spec.record_effect = RecordEffect::ActualPrivateConfigurationProperties;
            }
            Self::PreviewPersonAccess => {
                spec.target = TargetRequirement::ReadablePersonAndPreviewRecord;
            }
        }
        spec
    }
}

impl FromStr for AdminOperation {
    type Err = CatalogError;

    fn from_str(tag: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|operation| operation.tag() == tag)
            .ok_or(CatalogError::UnknownOperation)
    }
}

pub fn single_receipt_operation(
    operations: &[AdminOperation],
) -> Result<AdminOperation, CatalogError> {
    match operations {
        [operation] if operation.receipt() == ReceiptRequirement::SingleCommand => Ok(*operation),
        _ => Err(CatalogError::InvalidReceiptCommands),
    }
}

pub fn standing_property() -> Property {
    Property::Extension(ExtensionProperty {
        namespace: store::people::NAMESPACE.into(),
        field: store::people::STANDING_KEY.into(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigurationField {
    Version,
    ListenAddr,
    AcceptCredentials,
}

impl ConfigurationField {
    pub const ALL: &'static [Self] = &[Self::Version, Self::ListenAddr, Self::AcceptCredentials];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::ListenAddr => "listen_addr",
            Self::AcceptCredentials => "accept_credentials",
        }
    }

    pub fn property(self) -> Property {
        Property::Extension(ExtensionProperty {
            namespace: CONFIGURATION_NAMESPACE.into(),
            field: self.name().into(),
        })
    }

    pub const fn mutable_on_replacement(self) -> bool {
        match self {
            Self::Version => false,
            Self::ListenAddr | Self::AcceptCredentials => true,
        }
    }
}

impl FromStr for ConfigurationField {
    type Err = CatalogError;

    fn from_str(field: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.name() == field)
            .ok_or(CatalogError::UnknownConfigurationField)
    }
}

pub fn require_configuration_version(version: u16) -> Result<(), CatalogError> {
    if version == CONFIGURATION_VERSION {
        Ok(())
    } else {
        Err(CatalogError::UnsupportedConfigurationVersion)
    }
}

pub fn parse_listen_addr(value: &str) -> Result<SocketAddr, CatalogError> {
    if value.len() > MAX_LISTEN_ADDR_BYTES {
        return Err(CatalogError::InvalidListenAddress);
    }
    value
        .parse()
        .map_err(|_| CatalogError::InvalidListenAddress)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationApplication {
    ExplicitProcessRestart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialAdmission {
    PasswordOrExplicitGrantedLogin,
    ExplicitGrantedLoginOnly,
}

impl CredentialAdmission {
    pub const fn from_accept_credentials(accept_credentials: bool) -> Self {
        if accept_credentials {
            Self::PasswordOrExplicitGrantedLogin
        } else {
            Self::ExplicitGrantedLoginOnly
        }
    }
}
