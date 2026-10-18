use crate::{Engine, EngineError, sync::OpBatch};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub root: Option<String>,
    operations: Vec<(String, String, String)>,
}

fn invalid() -> EngineError {
    EngineError::Consequence("This saved delivery needs a new sharing review.".into())
}

impl Scope {
    pub fn new(root: Option<&str>, batch: &OpBatch) -> Result<Self, EngineError> {
        let scope = Self {
            root: root.map(str::to_owned),
            operations: batch
                .ops
                .iter()
                .map(|op| (op.tbl.clone(), op.uid.clone(), op.field.clone()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self
            .root
            .as_ref()
            .is_some_and(|root| root.is_empty() || root.len() > 256)
            || self.operations.len() > 2048
            || self.operations.iter().any(|(table, uid, field)| {
                table.is_empty()
                    || table.len() > 64
                    || uid.is_empty()
                    || uid.len() > 256
                    || field.len() > 1024
            })
            || serde_json::to_vec(self)?.len() > 262144
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn parse(bytes: &str) -> Result<Self, EngineError> {
        if bytes.len() > 262144 {
            return Err(invalid());
        }
        let scope: Self = serde_json::from_str(bytes).map_err(|_| invalid())?;
        scope.validate()?;
        Ok(scope)
    }
}

impl Engine {
    pub(super) async fn outgoing_mail_policy_hash(
        &self,
        to: &str,
        scope: &Scope,
    ) -> Result<String, EngineError> {
        let pool = &self.store.pool;
        let contact = store::organs::contact(pool, to).await?;
        let local = store::organs::local(pool).await?.ok_or_else(invalid)?.uid;
        let grant = match scope.root.as_deref() {
            Some(root) => store::replica::state(pool, root, to).await?,
            None => None,
        };
        let mut records = BTreeSet::new();
        for (table, uid, _) in &scope.operations {
            records.extend(store::visibility::records_of_op(pool, table, uid).await?);
        }
        let mut hidden = self
            .outgoing_hidden_from_organ(to)
            .await?
            .into_iter()
            .filter(|uid| records.contains(uid))
            .collect::<Vec<_>>();
        hidden.sort();
        if to != local
            && (contact
                .as_ref()
                .is_some_and(|contact| contact.trust == "blocked")
                || scope.root.is_some() && grant.as_deref() != Some(store::replica::ACCEPTED)
                || !hidden.is_empty())
        {
            return Err(EngineError::Forbidden("Saved mail is held because current sharing permissions differ. Review the content and resend it with current permissions.".into()));
        }
        let mut selection = Vec::new();
        if scope.root.is_none()
            && let Some(raw) = contact
                .as_ref()
                .and_then(|contact| contact.share_protein.as_deref())
        {
            selection = crate::share::selected(self, to, raw)
                .await?
                .into_iter()
                .filter(|uid| records.contains(uid))
                .collect();
            selection.sort();
        }
        let policy=contact.map(|contact| if scope.root.is_some() {
            json!({"trust":contact.trust,"mode":contact.mode})
        } else {
            json!({"trust":contact.trust,"mode":contact.mode,"sync_out":contact.sync_out,"scope_version":contact.scope_version,"scope_fields":contact.scope_fields,"share_protein":contact.share_protein})
        });
        Ok(nucleus::fact::sha256_hex(&serde_json::to_vec(&(
            "lince.mailbox-permission-scope.v1",
            to,
            scope,
            policy,
            selection,
            hidden,
            grant,
        ))?))
    }

    pub(crate) async fn queued_mail_policy_hash(
        &self,
        uid: &str,
        to: &str,
    ) -> Result<String, EngineError> {
        let scope = store::mailbox::outbox::scope(&self.store.pool, uid, to)
            .await?
            .ok_or_else(invalid)?;
        self.outgoing_mail_policy_hash(to, &Scope::parse(&scope)?)
            .await
    }
}
