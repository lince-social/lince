use std::fmt;

use sqlx::SqliteConnection;

use crate::StoreError;

pub const MAX_USERNAME_BYTES: usize = 256;
pub const MAX_PASSWORD_HASH_BYTES: usize = 4096;
pub const MAX_DEVICES_PER_PERSON: usize = 32;
pub const MAX_DEVICES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticationSource {
    Password,
    OrganLogin {
        organ_uid: String,
        node_id: String,
        generation: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticationState {
    person_uid: String,
    generation: i64,
    source: AuthenticationSource,
}

impl AuthenticationState {
    pub fn person_uid(&self) -> &str {
        &self.person_uid
    }

    pub fn generation(&self) -> i64 {
        self.generation
    }

    pub fn source(&self) -> &AuthenticationSource {
        &self.source
    }
}

pub struct PasswordCredential {
    authentication: AuthenticationState,
    username: String,
    password_hash: String,
}

impl PasswordCredential {
    pub fn authentication(&self) -> &AuthenticationState {
        &self.authentication
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password_hash(&self) -> &str {
        &self.password_hash
    }
}

impl fmt::Debug for PasswordCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordCredential")
            .field("authentication", &self.authentication)
            .field("password_hash", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactTrust {
    Unknown,
    Known,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerContact {
    pub organ_uid: String,
    pub node_id: String,
    pub trust: ContactTrust,
    pub generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonDevice {
    pub person_uid: String,
    pub node_id: String,
    pub revoked: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAdmission {
    authentication: AuthenticationState,
    device: PersonDevice,
    peer_contact: Option<PeerContact>,
}

impl DeviceAdmission {
    pub fn authentication(&self) -> &AuthenticationState {
        &self.authentication
    }

    pub fn device(&self) -> &PersonDevice {
        &self.device
    }

    pub fn peer_contact(&self) -> Option<&PeerContact> {
        self.peer_contact.as_ref()
    }
}

fn protocol(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_string())
}

fn record_uid(uid: &str) -> Result<(), StoreError> {
    if nucleus::valid_uid(uid, "r") {
        Ok(())
    } else {
        Err(protocol("invalid authentication Record identity"))
    }
}

fn node_id(value: &str) -> Result<(), StoreError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(protocol("device identity must be a canonical QUIC NodeId"))
    }
}

async fn generation_on(
    connection: &mut SqliteConnection,
    uid: &str,
    organ: bool,
) -> Result<i64, StoreError> {
    record_uid(uid)?;
    let query = if organ {
        "SELECT CASE WHEN typeof(generation) = 'integer' AND generation > 0
                     THEN generation END FROM organ_login_generation WHERE organ_uid = ?"
    } else {
        "SELECT CASE WHEN typeof(generation) = 'integer' AND generation > 0
                     THEN generation END FROM person_auth_generation WHERE person_uid = ?"
    };
    sqlx::query_scalar::<_, Option<i64>>(query)
        .bind(uid)
        .fetch_optional(&mut *connection)
        .await?
        .flatten()
        .ok_or_else(|| protocol("authentication generation is missing or corrupt"))
}

pub async fn person_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<i64, StoreError> {
    if !crate::people::is_active_on(connection, person_uid).await? {
        return Err(protocol("authentication Person is not active"));
    }
    generation_on(connection, person_uid, false).await
}

pub async fn password_on(
    connection: &mut SqliteConnection,
    username: &str,
) -> Result<Option<PasswordCredential>, StoreError> {
    if username.trim().is_empty() || username.len() > MAX_USERNAME_BYTES {
        return Err(protocol(
            "login username is empty or exceeds its byte limit",
        ));
    }
    credential_on(connection, username, false).await
}

async fn credential_on(
    connection: &mut SqliteConnection,
    identity: &str,
    by_person: bool,
) -> Result<Option<PasswordCredential>, StoreError> {
    let column = if by_person { "person_uid" } else { "username" };
    let query = format!(
        "WITH bounded AS (
             SELECT person_uid, username, password_hash,
                    CASE WHEN typeof(person_uid) = 'text' AND length(CAST(person_uid AS BLOB)) = 28
                              AND typeof(username) = 'text' AND length(trim(username)) > 0
                              AND length(CAST(username AS BLOB)) <= ?
                              AND typeof(password_hash) = 'text' AND length(trim(password_hash)) > 0
                              AND length(CAST(password_hash AS BLOB)) <= ?
                         THEN 1 ELSE 0 END AS valid
               FROM person_credential WHERE {column} = ?
         )
         SELECT valid, CASE WHEN valid = 1 THEN person_uid END,
                CASE WHEN valid = 1 THEN username END,
                CASE WHEN valid = 1 THEN password_hash END FROM bounded"
    );
    let row = sqlx::query_as::<_, (i64, Option<String>, Option<String>, Option<String>)>(&query)
        .bind(MAX_USERNAME_BYTES as i64)
        .bind(MAX_PASSWORD_HASH_BYTES as i64)
        .bind(identity)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (1, Some(person_uid), Some(username), Some(password_hash)) = row else {
        return Err(protocol("credential has invalid or oversized stored data"));
    };
    let generation = person_generation_on(connection, &person_uid).await?;
    Ok(Some(PasswordCredential {
        authentication: AuthenticationState {
            person_uid,
            generation,
            source: AuthenticationSource::Password,
        },
        username,
        password_hash,
    }))
}

pub async fn peer_contact_on(
    connection: &mut SqliteConnection,
    authenticated_node_id: &str,
) -> Result<Option<PeerContact>, StoreError> {
    node_id(authenticated_node_id)?;
    let row = sqlx::query_as::<_, (i64, i64, Option<String>, Option<String>)>(
        "WITH bounded AS (
             SELECT COUNT(*) AS matches,
                    COALESCE(SUM(CASE WHEN typeof(c.record_uid) = 'text' AND length(CAST(c.record_uid AS BLOB)) = 28
                              AND typeof(c.node_id) = 'text' AND c.node_id = ?
                              AND typeof(c.trust) = 'text'
                              AND c.trust IN ('unknown', 'known', 'blocked')
                              AND typeof(r.kind) = 'text' AND r.kind = 'organ' AND r.deleted_at IS NULL
                         THEN 0 ELSE 1 END), 0) AS invalid
               FROM organ_contact c LEFT JOIN record r ON r.uid = c.record_uid
              WHERE lower(trim(CAST(c.node_id AS TEXT))) = ?
         )
         SELECT b.matches, b.invalid, c.record_uid, c.trust FROM bounded b
           LEFT JOIN organ_contact c ON b.matches = 1 AND b.invalid = 0 AND c.node_id = ? LIMIT 1",
    ).bind(authenticated_node_id).bind(authenticated_node_id).bind(authenticated_node_id).fetch_one(&mut *connection).await?;
    if row == (0, 0, None, None) {
        return Ok(None);
    }
    let (1, 0, Some(organ_uid), Some(trust)) = row else {
        return Err(protocol(
            "peer contact has corrupt or ambiguous identity data",
        ));
    };
    let trust = match trust.as_str() {
        "unknown" => ContactTrust::Unknown,
        "known" => ContactTrust::Known,
        "blocked" => ContactTrust::Blocked,
        _ => return Err(protocol("peer contact trust is invalid")),
    };
    let generation = generation_on(connection, &organ_uid, true).await?;
    Ok(Some(PeerContact {
        organ_uid,
        node_id: authenticated_node_id.to_string(),
        trust,
        generation,
    }))
}

pub async fn granted_login_on(
    connection: &mut SqliteConnection,
    organ_uid: &str,
    authenticated_node_id: &str,
) -> Result<Option<AuthenticationState>, StoreError> {
    record_uid(organ_uid)?;
    node_id(authenticated_node_id)?;
    let row = sqlx::query_as::<_, (i64, i64, Option<String>)>(
        "WITH bounded AS (
             SELECT COUNT(*) AS matches,
                    COALESCE(SUM(CASE WHEN typeof(organ_uid) = 'text' AND typeof(person_uid) = 'text'
                              AND length(CAST(person_uid AS BLOB)) = 28 THEN 0 ELSE 1 END), 0) AS invalid
               FROM organ_login WHERE CAST(organ_uid AS TEXT) = ?
         )
         SELECT b.matches, b.invalid, l.person_uid FROM bounded b
           LEFT JOIN organ_login l ON b.matches = 1 AND b.invalid = 0 AND l.organ_uid = ? LIMIT 1",
    )
    .bind(organ_uid)
    .bind(organ_uid)
    .fetch_one(&mut *connection)
    .await?;
    if row == (0, 0, None) {
        return Ok(None);
    }
    let (1, 0, Some(person_uid)) = row else {
        return Err(protocol("Organ login has invalid stored identities"));
    };
    let peer = peer_contact_on(connection, authenticated_node_id)
        .await?
        .ok_or_else(|| protocol("Organ login has no current peer contact"))?;
    if peer.organ_uid != organ_uid || peer.trust == ContactTrust::Blocked {
        return Err(protocol("Organ login peer identity is unavailable"));
    }
    let generation = person_generation_on(connection, &person_uid).await?;
    Ok(Some(AuthenticationState {
        person_uid,
        generation,
        source: AuthenticationSource::OrganLogin {
            organ_uid: organ_uid.to_string(),
            node_id: authenticated_node_id.to_string(),
            generation: peer.generation,
        },
    }))
}

pub async fn require_authentication_on(
    connection: &mut SqliteConnection,
    authentication: &AuthenticationState,
) -> Result<(), StoreError> {
    let current = match &authentication.source {
        AuthenticationSource::Password => {
            credential_on(connection, &authentication.person_uid, true)
                .await?
                .map(|credential| credential.authentication)
        }
        AuthenticationSource::OrganLogin {
            organ_uid, node_id, ..
        } => granted_login_on(connection, organ_uid, node_id).await?,
    };
    if current.as_ref() == Some(authentication) {
        Ok(())
    } else {
        Err(protocol("authentication state is no longer current"))
    }
}

pub async fn device_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    authenticated_node_id: &str,
) -> Result<Option<PersonDevice>, StoreError> {
    record_uid(person_uid)?;
    node_id(authenticated_node_id)?;
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        "SELECT CASE WHEN typeof(revoked) = 'integer' AND revoked IN (0, 1) THEN revoked END,
                CASE WHEN typeof(revision) = 'integer' AND revision > 0 THEN revision END
           FROM person_device WHERE person_uid = ? AND node_id = ?",
    )
    .bind(person_uid)
    .bind(authenticated_node_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (Some(revoked), Some(revision)) = row else {
        return Err(protocol("device state is corrupt"));
    };
    Ok(Some(PersonDevice {
        person_uid: person_uid.to_string(),
        node_id: authenticated_node_id.to_string(),
        revoked: revoked == 1,
        revision,
    }))
}

pub async fn register_device_on(
    connection: &mut SqliteConnection,
    authentication: &AuthenticationState,
    authenticated_node_id: &str,
) -> Result<DeviceAdmission, StoreError> {
    node_id(authenticated_node_id)?;
    require_authentication_on(connection, authentication).await?;
    if let AuthenticationSource::OrganLogin { node_id, .. } = authentication.source()
        && node_id != authenticated_node_id
    {
        return Err(protocol(
            "granted authentication belongs to a different device",
        ));
    }
    let peer_contact = peer_contact_on(connection, authenticated_node_id).await?;
    if peer_contact
        .as_ref()
        .is_some_and(|peer| peer.trust == ContactTrust::Blocked)
    {
        return Err(protocol("device peer contact is blocked"));
    }
    if device_on(
        connection,
        authentication.person_uid(),
        authenticated_node_id,
    )
    .await?
    .is_none()
    {
        sqlx::query(
            "INSERT INTO person_device (person_uid, node_id, revoked, revision)
             SELECT ?, ?, 0, 1 WHERE (SELECT COUNT(*) FROM person_device) < ?
                AND (SELECT COUNT(*) FROM person_device WHERE person_uid = ?) < ?
             ON CONFLICT(person_uid, node_id) DO NOTHING",
        )
        .bind(authentication.person_uid())
        .bind(authenticated_node_id)
        .bind(MAX_DEVICES as i64)
        .bind(authentication.person_uid())
        .bind(MAX_DEVICES_PER_PERSON as i64)
        .execute(&mut *connection)
        .await?;
    }
    let device = device_on(
        connection,
        authentication.person_uid(),
        authenticated_node_id,
    )
    .await?
    .ok_or_else(|| protocol("device registration limit exceeded"))?;
    if device.revoked {
        return Err(protocol("device is revoked"));
    }
    Ok(DeviceAdmission {
        authentication: authentication.clone(),
        device,
        peer_contact,
    })
}

pub async fn compare_and_set_revoked_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    authenticated_node_id: &str,
    expected_revision: i64,
    revoked: bool,
) -> Result<PersonDevice, StoreError> {
    record_uid(person_uid)?;
    node_id(authenticated_node_id)?;
    if expected_revision <= 0 || expected_revision == i64::MAX {
        return Err(protocol("invalid or exhausted device revision"));
    }
    let current = device_on(connection, person_uid, authenticated_node_id)
        .await?
        .ok_or_else(|| protocol("device is missing"))?;
    if current.revision != expected_revision {
        return Err(protocol("device revision conflict"));
    }
    let changed = sqlx::query(
        "UPDATE person_device SET revoked = ?, revision = revision + 1
          WHERE person_uid = ? AND node_id = ? AND revision = ?",
    )
    .bind(i64::from(revoked))
    .bind(person_uid)
    .bind(authenticated_node_id)
    .bind(expected_revision)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(protocol("device revision conflict"));
    }
    device_on(connection, person_uid, authenticated_node_id)
        .await?
        .ok_or_else(|| protocol("device is missing"))
}

pub async fn require_admission_on(
    connection: &mut SqliteConnection,
    admission: &DeviceAdmission,
) -> Result<(), StoreError> {
    require_authentication_on(connection, &admission.authentication).await?;
    let device = device_on(
        connection,
        &admission.device.person_uid,
        &admission.device.node_id,
    )
    .await?;
    if device.as_ref() != Some(&admission.device) || admission.device.revoked {
        return Err(protocol("device admission is no longer current"));
    }
    let peer = peer_contact_on(connection, &admission.device.node_id).await?;
    if peer != admission.peer_contact
        || peer
            .as_ref()
            .is_some_and(|peer| peer.trust == ContactTrust::Blocked)
    {
        return Err(protocol("device peer admission is no longer current"));
    }
    Ok(())
}
