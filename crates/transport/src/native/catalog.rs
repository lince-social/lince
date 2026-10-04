use std::{collections::BTreeSet, sync::LazyLock};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Context, error};

static ACTIONS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::to_value(schemars::schema_for!(engine::actions::Action)).expect("Action schema")
});
static PROTEIN: LazyLock<Value> = LazyLock::new(|| {
    serde_json::to_value(schemars::schema_for!(protein::Protein)).expect("Protein schema")
});

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: Option<String>,
}

pub(super) fn describe(arguments: Value, context: &Context) -> Result<Value, String> {
    let args: Arguments = serde_json::from_value(arguments).map_err(error)?;
    let variants = ACTIONS["oneOf"]
        .as_array()
        .or_else(|| ACTIONS["anyOf"].as_array())
        .ok_or("Action schema has no variants.")?;
    if let Some(name) = args.action {
        let mut schema = variants
            .iter()
            .find(|variant| variant["properties"]["action"]["const"] == name)
            .cloned()
            .ok_or("Unknown Action name. Use lince_describe without an action to list names.")?;
        let mut references = BTreeSet::new();
        collect(&schema, &mut references);
        let mut definitions = serde_json::Map::new();
        while let Some(name) = references.pop_first() {
            if definitions.contains_key(&name) {
                continue;
            }
            let value = ACTIONS["$defs"][&name].clone();
            collect(&value, &mut references);
            definitions.insert(name, value);
        }
        if !definitions.is_empty() {
            schema["$defs"] = definitions.into();
        }
        return Ok(
            json!({"schema":schema,"new_record_uid":nucleus::new_uid("r"),"new_change_id":nucleus::new_uid("op"),"new_request_id":nucleus::new_uid("request"),"new_placement_id":nucleus::new_uid("placement")}),
        );
    }
    let names: Vec<_> = variants
        .iter()
        .filter_map(|variant| variant["properties"]["action"]["const"].as_str())
        .collect();
    Ok(json!({
        "context":context,
        "record_fields":protein::record_schema::fields(),
        "protein_schema":*PROTEIN,
        "actions":names,
        "instructions":[
            "Use Protein for saved objects and typed read or preview Actions for computed results and history. The normal Session and engine decide what is visible and writable.",
            "Use stable Record UIDs. Read Records before changing them. Request individual Action schemas when needed.",
            "Action schema responses supply fresh IDs for actions that require a new Record UID or change ID. Never invent a UID for an existing Record. Protein and Action tool arguments are JSON-encoded strings.",
            "Text edits use lince_edit_text, never edit-record-text or raw CRDT bytes. Snapshot edits preserve concurrent insertions; read the merged result before claiming completion.",
            "Read IDs and request receipts last for this turn only. Query existing effects before retrying in a new turn.",
            "For external agents, a tool connection is the turn scope. Use lince_release_reads to free snapshots after edits. Reopen the connection when its operation budget is reached; inspect uncertain effects before retrying.",
            "Use lince_message for replies in the context thread. Start, update and finish the same message with your complete generated text; Lince applies only your CRDT differences and preserves human edits. Connection closure interrupts messages still writing.",
            "Change assertions and assignees with assert-record/retract-record. Assignees use the assigned-to relation; query its Concept and Person UIDs first.",
            "For board state, read the board's configured enter/leave effects. Use preview-area-transition and apply-area-transition to change assertions and exact quantity together. A stale preview must be read and reconsidered.",
            "The default Kanban columns are backlog=0, todo=-1, next=-2, wip=-3, review=-4, done=1, documented=2. Custom boards may differ.",
            "File sync is optional. Query Organ Records with include.extension.namespace=lince.file_sync, then file-sync-status. Use lince_sync_status for activity. Saved, exported and delivered are distinct outcomes.",
            "Tool availability does not grant extra permissions or signing keys. Backend refusal is final for that operation; report the reason."
            ,"Karma uses ordinary Actions: create-frequency/delete-frequency, save-karma-rule/revise-karma-field/delete-recurrence/set-recurrence-paused. Read karma_rule and frequency Protein sources for current UIDs, revisions, bindings and shared fields. Preserve field references when editing; request exact schemas before writing. Live mutation requires the person's instructions."
            ,"Karma can show a component with @record: show({\"kind\":\"record\",\"mode\":\"call\"}). Record modes are full, description and call; an explicit record property selects another bound Record. Optional start_call state {\"thread\":\"thread\",\"person\":\"me\",\"media\":\"audio\"} automatically starts the selected thread call as that Person after ordinary admission. Media is audio by default or video. The thread must belong to the bound Record; Person identity and current access are checked. Native media support is required. Without start_call, call mode only shows controls. Text uses kind=text and text; Karma/Frequency use kind=karma/frequency and search. present-component uses the same typed state. Components appear in the connected native interface's Backend components area, one instance per target and component kind. Repeated pushes reuse the component and an active or pending call; a later firing can start it again after it ends. Changed component state may replace the instance. Workspace restoration alone does not start calls. No connected native receiver is an explicit failure."
            ,"save-karma-command creates or revises a saved Signal command using a fixed shell script or executable plus literal arguments and a local execution Cell. query_command(@source) obtains a fresh exact numeric result during Rule execution; signal(@source) reads a successful saved sample without expiration. Logical &&/|| guard requests; multiplication does not suppress execution. run-karma-command is explicit authorized Run/test, with numeric=true for a numeric sample. @target: run(@source) selects a saved consequence command. inspect-karma-commands shows definition revisions and bounded results/errors/indeterminate outcomes. Inspection and editing never run commands. Failed fresh queries block their evaluation, retaining any older saved sample. Simulation uses command-response inputs and never launches processes."
            ,"preview-karma-reading evaluates a selected readable expression. inspect-karma-rule-history explains the readings, computed value, Threshold and action status actually recorded for an application; it does not recompute history using current data."
            ,"preview-karma-proposal tests unsaved Save inputs in a copy using explicit controlled inputs, selected checks and bounded horizon/evaluations/wall time. Saving alone does not fire a Rule. Report endpoint values, first broken restriction, contributing cycle Rules, stop and incomplete coverage. Live save is a separate instructed Action. An old preview receipt is refused after source data changes; request a fresh preview. Incoming HTTP work is deferred."
            ,"Transfer Conditions use agreement_level(@transfer, @person), agreement_changed_at (UTC milliseconds), agreement_age (seconds), transfer_revision, transfer_active, transfer_published and transfer_ready. Request karma_state in fields from the authorized transfer Protein source; never substitute the backing Record quantity. Use stable Transfer and Person UIDs."
            ,"Transfer consequences are @transfer: agreement(@person) for the calculated whole level 0–2, agreement(@person, 2) for a fixed level, agreement(@person, 2, 3d) for a guarded stage after the prior actual agreement change, publish(@person), and activate(@person, @promise, \"fulfillment-key\"). Retain the same explicit fulfillment key to edit/retry the intended purchase without creating another. Gate/Threshold controls firing; a fixed target zero remains valid."
            ,"assign-transfer-agreement-level, publish-transfer and activate-transfer-fulfillment use ordinary Transfer authority and terms/state guards. A queued remote command is awaiting its origin result; report commitment only after acceptance. inspect-transfer-karma reads related own agreement Rules, calculated/fixed targets, pending stages and delivery status. After a retreat, offer instructed pause/edit of selected Rules; never automatically pause them. Proposal final values include authorized Transfer state; outgoing remote work without a controlled origin adapter makes coverage incomplete."
        ],
        "canvas_component_schema": schemars::schema_for!(nucleus::canvas::Component),
        "canvas_context": "Use inspect-canvases, then canvas Registry/Inspect. Layout Actions require a connected native canvas. The source=composition contract can combine registered native Sands/Castles; native parts must advertise composable=true. Save these through create-custom-component using format=lince.canvas_component, name and component. It is a typed library definition, not native Rust code. Workspace, placement, Record and reusable-definition identities differ. Never infer native component names/settings; use the returned registry. Removing a placement preserves its Record and definition. Mutations require the inspected revision and a stable request_id; receipts identify actual application and persistence. A timeout is uncertain, so inspect effects before retrying. Native generated content must use the same Fiote balloon and bidirectional isolation as typed compositions.",
        "fiote_workflows": [
            "report-fiote-child links an existing child conversation to this connection's current parent thread with a task and reported working/waiting/stopped/finished/interrupted state. It reports a link; it does not launch another agent. Lince exposes its supplied input and known run state through native instruction/context inspection, and labels unavailable external internal memory.",
            "present-component accepts a composition with name and typed parts (id, position [x,y], size [width,height], component). Parts can be record, text, karma, frequency, transfer, calendar, area, button, or nested composition. Discover the exact schema. A button has a label and a normal Action JSON object with an action discriminator. Presentation from Fiote automatically receives a protected conversation balloon; no generated wrapper is needed. Save component persists the layout and bindings in the existing library. Closing presentation leaves committed data changes in place.",
            "create-custom-component saves a composition document with format=lince.custom_component and composition={name,parts}; head must match name. Parts optionally bind named events with events=[{event,action}], using normal static Action parameters within their own balloon. Backend and interface validate the same typed contract. Discover unsupported parts instead of inventing native Rust components.",
            "activate-fiote carries a nonzero exact value with a stable request_id. Karma uses @fiote: activate-fiote with its calculated value, once per Rule occurrence. It leaves the Fiote Record quantity unchanged. Busy activations combine into one pending run preserving their causes and values. inspect-fiote-activations shows bounded request/run status; activation grants no extra authority.",
            "Conversation is general-purpose. Create Facts only when the person requests or configures recording. Discover existing Records/Concepts and exact units, ask about ambiguous targets or interpretations, and use capture-entry for exact quantities, classify-fact for category, revise-entry/void-entry for correction. Retain the originating conversation/message as evidence in the note; source attribution is also stamped by Lince. A receipt or dictation is input to the same conversation, not a privileged or mandatory mutation path. Reuse the same request_id for retries and check uncertain effects before creating another entry.",
            "Transfer CRUD uses normal transfer Protein sources and Actions. Read current participant/terms/revision and discover exact create/revise/invitation/agreement/publication/fulfillment/settlement/loan/cancellation/correction schemas. Show Transfer or Record components for review and shared Simulation for predictions. Draft removal must satisfy discard-transfer-draft eligibility; committed or negotiated effects use cancellation/correction, retaining signed evidence. Never claim a queued remote command has committed."
        ],
        "examples":{
            "search":{"source":"record","where":[{"text_contains":"work"}],"fields":["uid","head","quantity","assertions","assignees"],"limit":20},
            "proteins":{"source":"record","where":[{"kind_eq":"protein"}],"include":{"extension":{"namespace":"lince.protein"}},"limit":20},
            "file_sync":{"source":"record","where":[{"kind_eq":"organ"}],"include":{"extension":{"namespace":"lince.file_sync"}},"limit":20},
            "karma_rules":{"source":"karma_rule","limit":20},
            "karma_frequencies":{"source":"frequency","limit":20}
        }
    }))
}

fn collect(value: &Value, references: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(name) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
            {
                references.insert(name.into());
            }
            for value in object.values() {
                collect(value, references);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, references);
            }
        }
        _ => {}
    }
}
