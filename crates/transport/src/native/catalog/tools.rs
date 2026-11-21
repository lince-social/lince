//! Tool descriptions and JSON schemas published to Fiote clients.

use fiote::provider::ToolDefinition;
use serde_json::json;

use crate::native::{Kind, assistant};

impl Kind {
    pub(in crate::native) fn definition(self, pinned: Option<&str>) -> ToolDefinition {
        let (name, description, schema) = match self {
            Kind::Fiotes => (
                "lince_fiotes",
                "Discover Fiotes eligible for native handoff, including their descriptions and connection state. Read every next_offset. The intake is selected by direct children then oldest creation time; explicit recipient choices remain unchanged.",
                json!({"type":"object","properties":{"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":assistant::MAX_DISCOVERY_PAGE}},"additionalProperties":false}),
            ),
            Kind::Delegate => (
                "lince_delegate",
                "Hand off this request to an enabled Fiote after reading its description. Use its stable Record UID and a stable request_id. Optional request is the delegated task; when omitted, the original request is forwarded. The saved source Message and attachments are preserved. Returns a durable receipt and child thread, not proof of completion. Inspect lince_handoffs before retrying uncertain work. Self-handoff and cycles are refused.",
                json!({"type":"object","properties":{"request_id":{"type":"string","maxLength":128},"target":{"type":"string"},"request":{"type":"string","maxLength":65536}},"required":["request_id","target"],"additionalProperties":false}),
            ),
            Kind::Handoffs => (
                "lince_handoffs",
                "Inspect this conversation's durable handoff receipts and actual child reply state. Follow the returned thread/message/reply Record references to read output. A parent stopping does not stop an independently running child.",
                json!({"type":"object","properties":{},"additionalProperties":false}),
            ),
            Kind::Message => (
                "lince_message",
                "Write an attributed reply in this connection's thread. Start creates a writing message and returns its UID. Update supplies the complete text generated so far; only changes to your own text are applied through CRDT, preserving concurrent user edits. Finish or interrupt saves the final text and state. Reuse the same request_id to retry. Message UIDs must have been started by this connection.",
                json!({"type":"object","properties":{"request_id":{"type":"string"},"operation":{"type":"string","enum":["start","update","finish","interrupt"]},"message_uid":{"type":"string"},"text":{"type":"string"}},"required":["request_id","operation","text"],"additionalProperties":false}),
            ),
            Kind::Release => (
                "lince_release_reads",
                "Release collaborative read snapshots you no longer need. Released read_ids cannot be used for later changes. Existing write retry receipts are retained. At most 32 snapshots may be open at once.",
                json!({"type":"object","properties":{"read_ids":{"type":"array","items":{"type":"string"},"maxItems":32}},"required":["read_ids"],"additionalProperties":false}),
            ),
            Kind::Describe => (
                "lince_describe",
                "Discover Lince's native API. With no action, returns the Record fields, action names and usage. With an action name, returns its exact JSON schema, including referenced types. Protein and Actions are the same API the interface uses.",
                json!({"type":"object","properties":{"action":{"type":"string"}},"additionalProperties":false}),
            ),
            Kind::Query => (
                "lince_query",
                "Run a Protein query with this session's access. Read saved Proteins, Records, assertions, people, threads, facts and other sources. Results are bounded; narrow the query or select fewer fields if too large. Use lince_read_record before editing. Use lince_describe for the Protein schema.",
                json!({"type":"object","properties":{"protein":{"type":"string","description":"A Protein query encoded as JSON, using the schema from lince_describe."},"saved":{"type":"string"}},"additionalProperties":false}),
            ),
            Kind::Read => (
                "lince_read_record",
                "Read a Record through Protein and open its collaborative document. Returns a read_id for subsequent text or property changes. Read IDs are scoped to this turn. Use stable Record UIDs. Concurrent text edits merge; reread after each committed edit to see the merged result.",
                json!({"type":"object","properties":{"record_uid":{"type":"string"},"extensions":{"type":"array","items":{"type":"string"},"maxItems":8,"description":"Extension namespaces to read and guard, required before set-extension."}},"required":["record_uid"],"additionalProperties":false}),
            ),
            Kind::Edit => (
                "lince_edit_text",
                "Apply precise replacements to the head or body from a lince_read_record snapshot through the normal collaborative Action. Each before passage must appear exactly once in that snapshot; use an empty before only for an empty field. Edits in one call refer to the same snapshot and must not overlap. Concurrent user insertions are preserved. Use a stable request_id to retry the same operation.",
                json!({"type":"object","properties":{"read_id":{"type":"string"},"request_id":{"type":"string"},"edits":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","properties":{"field":{"type":"string","enum":["head","body"]},"before":{"type":"string"},"after":{"type":"string"}},"required":["field","before","after"],"additionalProperties":false}}},"required":["read_id","request_id","edits"],"additionalProperties":false}),
            ),
            Kind::Action => (
                "lince_action",
                "Execute a native Lince Action under the same session checks as the interface. Discover its schema first. Supply read_ids for existing Records you change; scalar/property changes reject stale reads. Use lince_edit_text for existing titles/descriptions. Use preview-area-transition then apply-area-transition for atomic state/quantity changes. Retries with the same request_id and arguments return the original result within this turn. Never retry an uncertain operation with a new ID without checking its effect.",
                json!({"type":"object","properties":{"request_id":{"type":"string"},"action":{"type":"string","description":"A native Action encoded as JSON, using the schema from lince_describe."},"read_ids":{"type":"array","items":{"type":"string"},"maxItems":32}},"required":["request_id","action","read_ids"],"additionalProperties":false}),
            ),
            Kind::Instructions => (
                "lince_instructions",
                "Reload the complete pinned Fiote and ancestor instructions after context compaction. These persist outside conversation summaries. Call this before continuing work after compaction.",
                json!({"type":"object","properties":{},"additionalProperties":false}),
            ),
            Kind::Sync => (
                "lince_sync_status",
                "Inspect Lince's existing sync activity. To discover configured file sync, query visible Organ Records with include.extension.namespace = lince.file_sync and use the file-sync-status Action. A saved database change does not imply its file export or remote delivery has completed.",
                json!({"type":"object","properties":{},"additionalProperties":false}),
            ),
        };
        ToolDefinition {
            name: name.into(),
            description: if matches!(self, Self::Instructions) {
                pinned.map_or_else(|| description.into(), |instructions| format!("{description}\n\nPersistent session instructions (retain across compaction):\n{instructions}"))
            } else {
                description.into()
            },
            schema,
        }
    }
}
