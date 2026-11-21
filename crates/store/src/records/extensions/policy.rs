use nucleus::{
    identity::roots,
    transfer::{outcome, public_history, receipt, receipt_source},
    trust_analysis::{claims, claims::recipes, contacts, query, serving},
    visibility::plan,
};
use serde_json::{Value, from_str};
use sqlx::{Sqlite, Transaction, query_scalar};

use super::{protocol, require_active_on, validate_extension_name};
use crate::StoreError;

const DELIVERED_ARTIFACT: &str =
    "SELECT EXISTS(SELECT 1 FROM record_artifact_delivery WHERE message_uid=?)";
const SAVED_OUTCOME: &str =
    "SELECT EXISTS(SELECT 1 FROM transfer_outcome_record WHERE record_uid=?)";

fn is_attachment(namespace: &str) -> bool {
    namespace == "lince.message-content" || namespace.starts_with("lince.message-data.")
}

pub(super) async fn check_set(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    namespace: &str,
    fields: &Value,
) -> Result<(), StoreError> {
    if namespace == plan::PRIVATE_NAMESPACE {
        return Err(protocol(
            "Change audience plans through reviewed visibility actions",
        ));
    }
    require_active_on(tx, uid).await?;
    validate_extension_name(namespace, "namespace", true)?;
    let (saved, message) = match namespace {
        public_history::NAMESPACE => {
            return Err(protocol(
                "Change public-history terms through a reviewed Transfer revision",
            ));
        }
        serving::NAMESPACE => (
            "SELECT review_json FROM record_evidence_serving WHERE record_uid=?",
            "Review a new evidence serving permission instead of replacing it",
        ),
        roots::NAMESPACE => (
            "SELECT review_json FROM organ_root_recognition WHERE record_uid=?",
            "Append a reviewed root recognition instead of replacing its decision",
        ),
        query::NAMESPACE => (
            "SELECT policy_json FROM trust_query_record WHERE record_uid=?",
            "Save a new reader policy instead of replacing its original fields",
        ),
        recipes::NAMESPACE => (
            "SELECT recipe_json FROM profile_claim_recipe_record WHERE record_uid=?",
            "Review a new publication recipe instead of changing its authorization",
        ),
        claims::NAMESPACE => (
            "SELECT document_json FROM profile_claim_record WHERE record_uid=?",
            "Append a reviewed signed claim instead of replacing its fields",
        ),
        contacts::NAMESPACE => (
            "SELECT document_json FROM contact_disclosure_record WHERE record_uid=?",
            "Append a reviewed signed contact disclosure instead of rewriting evidence",
        ),
        receipt::NAMESPACE => (
            "SELECT document_json FROM transfer_receipt_version WHERE record_uid=? ORDER BY version DESC LIMIT 1",
            "Use verified receipt controls to append signed evidence",
        ),
        receipt_source::NAMESPACE => (
            "SELECT payload_json FROM transfer_receipt_source_record WHERE record_uid=?",
            "Use reviewed source controls to append signed snapshots",
        ),
        outcome::NAMESPACE => (
            "SELECT payload_json FROM transfer_outcome_record WHERE record_uid=?",
            "Append a signed outcome correction instead of rewriting evidence",
        ),
        _ if is_attachment(namespace) => {
            return reject_if_saved(
                tx,
                uid,
                DELIVERED_ARTIFACT,
                "Send a new reviewed artifact instead of changing delivered attachments",
            )
            .await;
        }
        _ => return Ok(()),
    };
    let original: Option<String> = query_scalar(saved)
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?;
    let original = original
        .as_deref()
        .map(from_str::<Value>)
        .transpose()
        .map_err(|error| protocol(error.to_string()))?;
    // The first outcome sets its fields before the signed outcome row exists.
    if original.as_ref() == Some(fields) || (namespace == outcome::NAMESPACE && original.is_none())
    {
        Ok(())
    } else {
        Err(protocol(message))
    }
}

pub(super) async fn check_delete(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    namespace: &str,
) -> Result<(), StoreError> {
    if namespace == plan::PRIVATE_NAMESPACE {
        return Err(protocol(
            "Stop an audience plan through its reviewed action",
        ));
    }
    require_active_on(tx, uid).await?;
    validate_extension_name(namespace, "namespace", true)?;
    let message = match namespace {
        public_history::NAMESPACE => {
            "Remove public-history terms through a reviewed Transfer revision"
        }
        roots::NAMESPACE => "Root recognition retains its original reviewed decision",
        serving::NAMESPACE => {
            "Delete the serving permission Record to stop; retain its reviewed decision"
        }
        query::NAMESPACE => {
            "Retain the saved reader policy; delete its Record or save a new policy"
        }
        claims::NAMESPACE => "Retain the original signed claim and append a withdrawal",
        recipes::NAMESPACE => "Publication recipes retain their original authorization",
        contacts::NAMESPACE => {
            "Retain the original signed contact disclosure and append a withdrawal"
        }
        receipt::NAMESPACE => "Receipt evidence is retained through its signed version history",
        receipt_source::NAMESPACE => "Original source snapshots retain their signed payloads",
        outcome::NAMESPACE => {
            return reject_if_saved(
                tx,
                uid,
                SAVED_OUTCOME,
                "Original outcome evidence must be retained",
            )
            .await;
        }
        _ if is_attachment(namespace) => {
            return reject_if_saved(
                tx,
                uid,
                DELIVERED_ARTIFACT,
                "Retain the original delivery attachment",
            )
            .await;
        }
        _ => return Ok(()),
    };
    Err(protocol(message))
}

async fn reject_if_saved(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    select: &'static str,
    message: &'static str,
) -> Result<(), StoreError> {
    if query_scalar(select).bind(uid).fetch_one(&mut **tx).await? {
        Err(protocol(message))
    } else {
        Ok(())
    }
}
