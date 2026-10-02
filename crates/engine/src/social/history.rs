use crate::{
    Engine, EngineError,
    sync::{OpBatch, WireOp},
};
use serde::{Deserialize, Serialize};
use store::sqlx::Row;

pub const TABLE: &str = "own_conversation";
use nucleus::social::CONVERSATION_VECTOR_PREFIX as VECTOR_PREFIX;

pub(crate) fn bound_sync_page(batch: &mut OpBatch, budget: usize) -> Result<bool, EngineError> {
    let mut bytes = 1024usize;
    let mut count = 0;
    for op in &batch.ops {
        let encoded = serde_json::to_vec(op)?.len().saturating_add(1);
        if bytes.saturating_add(encoded) > budget {
            if count == 0 {
                return Err(super::invalid(
                    "A retained operation exceeds the complete sync frame limit",
                ));
            }
            break;
        }
        bytes += encoded;
        count += 1;
    }
    let truncated = count < batch.ops.len();
    batch.ops.truncate(count);
    Ok(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_sync_frames_count_escaping_and_keep_operation_boundaries() {
        let operation = WireOp {
            tbl: "record".into(),
            uid: nucleus::new_uid("r"),
            field: "body".into(),
            kind: "update".into(),
            value: Some("\"\\\n".repeat(512)),
            hlc: 1,
            actor_cell: nucleus::new_uid("r"),
            organ_uid: nucleus::new_uid("r"),
            fact: None,
        };
        let mut batch = OpBatch {
            from_organ: operation.organ_uid.clone(),
            ops: vec![operation.clone(); 20],
        };
        assert!(bound_sync_page(&mut batch, 8192).unwrap());
        assert!(!batch.ops.is_empty());
        assert!(batch.ops.len() < 20);
        assert!(serde_json::to_vec(&batch).unwrap().len() < 8192);
        for returned in &batch.ops {
            assert_eq!(returned.value, operation.value);
        }
        assert!(bound_sync_page(&mut batch, 128).is_err());
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationPage {
    root: String,
    batch: OpBatch,
}

fn decode(op: &WireOp) -> Result<ConversationPage, EngineError> {
    if op.kind != "snapshot" || op.field != "lince.own-conversation.1" {
        return Err(super::invalid("Invalid own-conversation operation"));
    }
    let body = op
        .value
        .as_deref()
        .ok_or_else(|| super::invalid("Missing conversation operations"))?;
    if body.len() > 2 * 1024 * 1024 {
        return Err(super::invalid(
            "The conversation operation page is too large",
        ));
    }
    let page: ConversationPage = serde_json::from_str(body)?;
    if page.root != op.uid
        || !nucleus::valid_uid(&page.root, "r")
        || page.batch.from_organ != op.organ_uid
        || page.batch.ops.is_empty()
        || page.batch.ops.len() > 200
        || page.batch.ops.iter().any(|inner| {
            inner.tbl == TABLE
                || inner.actor_cell != op.actor_cell
                || inner.organ_uid != op.organ_uid
                || inner.hlc > op.hlc
        })
    {
        return Err(super::invalid(
            "Invalid conversation root, origin or page bounds",
        ));
    }
    Ok(page)
}

impl Engine {
    pub(crate) async fn export_own_conversations(
        &self,
        organ: &str,
        vector: &[store::sync_ops::VectorEntry],
    ) -> Result<Vec<WireOp>, EngineError> {
        let roots = store::sqlx::query("SELECT s.replica_root,s.actor_cell FROM sync_op s JOIN record r ON r.uid=s.replica_root LEFT JOIN json_each(?) v ON json_extract(v.value,'$.actor_cell')=?||s.replica_root||'/'||s.actor_cell WHERE s.organ_uid=? AND r.kind IN ('conversation','message_draft') GROUP BY s.replica_root,s.actor_cell HAVING MAX(s.hlc)>COALESCE(MAX(json_extract(v.value,'$.max_hlc')),0) ORDER BY s.replica_root,s.actor_cell LIMIT 8")
            .bind(serde_json::to_string(vector)?).bind(VECTOR_PREFIX).bind(organ)
            .fetch_all(&self.store.pool).await?;
        let mut output = Vec::new();
        let mut bytes = 0;
        for row in roots {
            let root: String = row.get("replica_root");
            let actor: String = row.get("actor_cell");
            let key = format!("{VECTOR_PREFIX}{root}/{actor}");
            let covered = vector
                .iter()
                .find(|entry| entry.actor_cell == key)
                .map_or(0, |entry| entry.max_hlc);
            let rows = store::sync_ops::own_conversation_ops_after(
                &self.store.pool,
                organ,
                &root,
                &actor,
                covered,
            )
            .await?;
            if rows.is_empty() {
                continue;
            }
            let mut ops = self.hydrate_ops(rows).await?;
            let mut value;
            loop {
                value = serde_json::to_string(&ConversationPage {
                    root: root.clone(),
                    batch: OpBatch {
                        from_organ: organ.into(),
                        ops: ops.clone(),
                    },
                })?;
                if value.len() <= 2 * 1024 * 1024 {
                    break;
                }
                if ops.len() == 1 {
                    return Err(super::invalid(
                        "A retained conversation operation exceeds the sync page bound",
                    ));
                }
                ops.truncate((ops.len() / 2).max(1));
            }
            let wrapper = WireOp {
                tbl: TABLE.into(),
                uid: root,
                field: "lince.own-conversation.1".into(),
                kind: "snapshot".into(),
                value: Some(value),
                hlc: ops.iter().map(|op| op.hlc).max().unwrap(),
                actor_cell: actor,
                organ_uid: organ.into(),
                fact: None,
            };
            let encoded = serde_json::to_vec(&wrapper)?.len();
            bytes += encoded;
            if bytes > 8 * 1024 * 1024 || output.len() >= 8 {
                break;
            }
            output.push(wrapper);
        }
        Ok(output)
    }

    pub(crate) async fn import_own_conversation(
        &self,
        from: &str,
        op: &WireOp,
    ) -> Result<usize, EngineError> {
        if store::organs::local(&self.store.pool)
            .await?
            .is_none_or(|organ| organ.uid != from)
            || op.organ_uid != from
        {
            return Err(EngineError::Forbidden("Conversation history wrappers are only accepted from this Organ's authorized devices".into()));
        }
        let page = decode(op)?;
        if let Some(root) = store::records::get(&self.store.pool, &page.root).await?
            && !matches!(root.kind.as_str(), "conversation" | "message_draft")
        {
            return Err(super::invalid(
                "The history wrapper targets a different Record kind",
            ));
        }
        self.import_grant_batch(&page.root, &page.batch).await
    }

    pub(crate) async fn own_conversation_saved(&self, op: &WireOp) -> Result<bool, EngineError> {
        let page = decode(op)?;
        if !self.plain_batch_is_saved(&page.batch).await? {
            return Ok(false);
        }
        for inner in &page.batch.ops {
            let root =
                store::replica::root_for_op(&self.store.pool, &inner.tbl, &inner.uid).await?;
            if root.as_deref() != Some(&page.root) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
