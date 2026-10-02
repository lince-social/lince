use super::*;
use nucleus::social::requests::*;

fn generation(value: &str) -> Result<i64, EngineError> {
    let number = value
        .parse::<i64>()
        .map_err(|_| invalid("Invalid private reply authority generation"))?;
    if number <= 0 || number.to_string() != value {
        return Err(invalid("Invalid private reply authority generation"));
    }
    Ok(number)
}

pub(super) fn ed_key(value: &str) -> Result<(), EngineError> {
    let bytes = B64
        .decode(value)
        .map_err(|_| invalid("Invalid private reply signing key"))?;
    if bytes.len() != 32 || B64.encode(bytes) != value {
        return Err(invalid("Use a canonical private reply signing key"));
    }
    Ok(())
}

fn curve_key(value: &str) -> Result<(), EngineError> {
    let key = vodozemac::Curve25519PublicKey::from_base64(value)
        .map_err(|_| invalid("Invalid private session key"))?;
    if key.to_base64() != value || key.to_bytes().iter().all(|byte| *byte == 0) {
        return Err(invalid("Use a canonical nonzero private session key"));
    }
    Ok(())
}

pub fn validate_device_request(
    request: &DeviceAuthorizationRequest,
    now: i64,
) -> Result<(), EngineError> {
    for key in [
        &request.operational_key,
        &request.route.signing_key,
        &request.route.pickup_key,
    ] {
        ed_key(key)?;
    }
    curve_key(&request.route.identity_key)?;
    curve_key(&request.route.prekey)?;
    if !nucleus::valid_uid(&request.context, "r")
        || !nucleus::valid_uid(&request.cell, "r")
        || !nucleus::valid_uid(&request.route.mailbox, "mail")
        || request.route.mailbox != mailbox_id(&request.route)?
        || request.route.services.is_empty()
        || request.route.services.len() > 8
        || request
            .route
            .services
            .iter()
            .any(|value| value.parse::<iroh::EndpointId>().is_err())
        || request.requested_at <= 0
        || request.requested_at > now + 300
        || !crate::roster::verify_with(
            &request.operational_key,
            &signing_bytes("private-device-request", request)?,
            &request.signature,
        )
    {
        return Err(invalid("Invalid private device authorization request"));
    }
    Ok(())
}

pub fn validate_control(control: &OwnerControl, now: i64) -> Result<(), EngineError> {
    generation(&control.generation)?;
    ed_key(&control.owner_key)?;
    if control.issued_at <= 0
        || control.issued_at > now + 300
        || control.expires_at <= now
        || control.expires_at <= control.issued_at
        || control.expires_at.saturating_sub(control.issued_at) > AUTHORITY_LIFETIME
        || !crate::roster::verify_with(
            &control.owner_key,
            &signing_bytes("reply-owner", control)?,
            &control.signature,
        )
    {
        return Err(invalid(
            "Private reply authority is expired or could not be authenticated",
        ));
    }
    Ok(())
}

pub fn validate_certificate(
    control: &OwnerControl,
    certificate: &DeviceCertificate,
    now: i64,
) -> Result<(), EngineError> {
    validate_control(control, now)?;
    for key in [&certificate.signing_key, &certificate.pickup_key] {
        ed_key(key)?;
    }
    curve_key(&certificate.identity_key)?;
    if certificate.owner_key != control.owner_key
        || certificate.generation != control.generation
        || !nucleus::valid_uid(&certificate.mailbox, "mail")
        || certificate.issued_at < control.issued_at
        || certificate.issued_at > now + 300
        || certificate.expires_at > control.expires_at
        || certificate.expires_at <= now
        || certificate.expires_at <= certificate.issued_at
        || !crate::roster::verify_with(
            &control.owner_key,
            &signing_bytes("reply-device", certificate)?,
            &certificate.signature,
        )
    {
        return Err(invalid(
            "This device lacks fresh private reply authorization. Refresh it through the owner device",
        ));
    }
    Ok(())
}

pub fn validate_route(document: &CertifiedRoute, now: i64) -> Result<(), EngineError> {
    validate_certificate(&document.control, &document.certificate, now)?;
    let route = &document.route;
    curve_key(&route.prekey)?;
    if route.mailbox != document.certificate.mailbox
        || route.mailbox != mailbox_id(route)?
        || route.signing_key != document.certificate.signing_key
        || route.identity_key != document.certificate.identity_key
        || route.pickup_key != document.certificate.pickup_key
        || route.services.len() > 8
        || route.services.is_empty()
        || route
            .services
            .iter()
            .any(|value| value.parse::<iroh::EndpointId>().is_err())
        || document.expires_at <= now
        || document
            .expires_at
            .saturating_sub(document.control.issued_at)
            > 30 * 86400
        || !crate::roster::verify_with(
            &route.signing_key,
            &signing_bytes("reply-route", document)?,
            &document.signature,
        )
        || serde_json::to_vec(document)?.len() > 8192
    {
        return Err(invalid("Invalid, oversized or expired private reply route"));
    }
    Ok(())
}

pub fn mailbox_id(route: &ReplyRoute) -> Result<String, EngineError> {
    use sha2::{Digest, Sha256};
    let bytes = signing_bytes(
        "mailbox-identity",
        &json!({"signing_key":route.signing_key,"identity_key":route.identity_key,"pickup_key":route.pickup_key}),
    )?;
    let digest = Sha256::digest(bytes);
    let mut time = [0u8; 8];
    time[2..].copy_from_slice(&digest[..6]);
    Ok(format!(
        "mail_{}",
        nucleus::id::ulid_from(
            u64::from_be_bytes(time),
            u128::from_be_bytes(digest[..16].try_into().unwrap())
        )
    ))
}

pub(super) fn validate_collected_delivery(
    document: &PrivateDelivery,
    accepted_at: i64,
    now: i64,
) -> Result<String, EngineError> {
    if accepted_at <= 0
        || accepted_at > now + 300
        || accepted_at < document.envelope.created_at.saturating_sub(300)
        || document.envelope.expires_at <= now
    {
        return Err(invalid(
            "The selected mailbox's admission time or ciphertext lifetime is invalid",
        ));
    }
    validate_delivery(document, accepted_at)
}

pub fn validate_delivery(document: &PrivateDelivery, now: i64) -> Result<String, EngineError> {
    let limit = if document.envelope.purpose == EnvelopePurpose::Content {
        MAX_ATTACHMENT_ENVELOPE_BYTES
    } else {
        MAX_ENVELOPE_BYTES
    };
    if serde_json::to_vec(document)?.len() > limit {
        return Err(invalid(
            "The private delivery exceeds its envelope size limit",
        ));
    }
    let envelope = &document.envelope;
    if envelope.id != envelope_id(envelope)? {
        return Err(invalid(
            "The envelope ID does not match its authenticated sender and ciphertext",
        ));
    }
    validate_certificate(
        &envelope.control,
        &envelope.certificate,
        envelope.created_at,
    )?;
    validate_certificate(
        &document.authorization.control,
        &document.authorization.certificate,
        now,
    )?;
    let fresh = &document.authorization.certificate;
    let original = &envelope.certificate;
    if envelope.protocol != "lince.private-message.1"
        || !nucleus::valid_uid(&envelope.id, "env")
        || !nucleus::valid_uid(&envelope.route, "mail")
        || !nucleus::valid_uid(&envelope.message, "msg")
        || envelope.created_at <= 0
        || envelope.created_at > now + 300
        || envelope.expires_at <= now
        || envelope.expires_at <= envelope.created_at
        || envelope.expires_at.saturating_sub(envelope.created_at) > 30 * 86400
        || envelope.sender_owner != original.owner_key
        || envelope.sender_key != original.signing_key
        || envelope.identity_key != original.identity_key
        || envelope.sender_owner != fresh.owner_key
        || envelope.sender_key != fresh.signing_key
        || envelope.identity_key != fresh.identity_key
        || envelope.session_id.len() != 43
        || envelope.content_hash.len() != 64
        || !envelope
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !matches!(envelope.message_type, 0 | 1)
        || !crate::roster::verify_with(
            &envelope.sender_key,
            &signing_bytes("private-envelope", envelope)?,
            &envelope.signature,
        )
    {
        return Err(invalid(
            "The private message authority, identity or lifetime could not be verified",
        ));
    }
    let bytes = B64
        .decode(&envelope.ciphertext)
        .map_err(|_| invalid("Malformed private message ciphertext"))?;
    vodozemac::olm::OlmMessage::from_parts(envelope.message_type as usize, &bytes)
        .map_err(|_| invalid("Malformed private session message"))?;
    document_hash("private-envelope", envelope)
}

pub fn envelope_id(envelope: &PrivateEnvelope) -> Result<String, EngineError> {
    use sha2::{Digest, Sha256};
    let mut value = serde_json::to_value(envelope)?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid private envelope"))?
        .remove("id");
    let digest = Sha256::digest(signing_bytes("private-envelope-id", &value)?);
    let mut time = [0u8; 8];
    time[2..].copy_from_slice(&digest[..6]);
    let entropy = u128::from_be_bytes(digest[..16].try_into().unwrap());
    Ok(format!(
        "env_{}",
        nucleus::id::ulid_from(u64::from_be_bytes(time), entropy)
    ))
}

pub fn validate_access(
    access: &MailboxAccess,
    route: &CertifiedRoute,
    now: i64,
) -> Result<(), EngineError> {
    generation(&access.sequence)?;
    validate_certificate(&access.control, &access.certificate, now)?;
    let nonce = B64
        .decode(&access.nonce)
        .map_err(|_| invalid("Invalid mailbox pickup nonce"))?;
    if nonce.len() != 16
        || B64.encode(nonce) != access.nonce
        || access.at.abs_diff(now) > 300
        || access.mailbox != route.route.mailbox
        || access.certificate.owner_key != route.control.owner_key
        || access.certificate.pickup_key != route.route.pickup_key
        || access.certificate.signing_key != route.route.signing_key
        || access.certificate.identity_key != route.route.identity_key
        || access.certificate.mailbox != access.mailbox
        || access.envelopes.len() > 8
        || access
            .envelopes
            .iter()
            .any(|id| !nucleus::valid_uid(id, "env"))
        || !crate::roster::verify_with(
            &route.route.pickup_key,
            &signing_bytes("mailbox-access", access)?,
            &access.signature,
        )
    {
        return Err(invalid(
            "The mailbox pickup request is stale or unauthorized",
        ));
    }
    Ok(())
}

pub fn validate_admission(
    document: &SenderAdmission,
    route: &CertifiedRoute,
    now: i64,
) -> Result<(), EngineError> {
    validate_certificate(&document.control, &document.certificate, now)?;
    ed_key(&document.sender_owner)?;
    if document.mailbox != route.route.mailbox
        || document.window < 0
        || document.window > document.issued_at
        || document.certificate.owner_key != route.control.owner_key
        || document.certificate.mailbox != document.mailbox
        || document.certificate.signing_key != route.route.signing_key
        || document.certificate.identity_key != route.route.identity_key
        || document.certificate.pickup_key != route.route.pickup_key
        || document.issued_at > now + 300
        || document.expires_at <= now
        || document.expires_at <= document.issued_at
        || document.expires_at.saturating_sub(document.issued_at) > 30 * 86400
        || !crate::roster::verify_with(
            &document.certificate.signing_key,
            &signing_bytes("sender-admission", document)?,
            &document.signature,
        )
    {
        return Err(invalid(
            "The recipient did not authorize this sender admission",
        ));
    }
    Ok(())
}
