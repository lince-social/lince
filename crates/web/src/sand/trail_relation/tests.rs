use std::process::Command;

fn run_node_assertions(script: &str) {
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .output()
        .expect("failed to launch node");

    if output.status.success() {
        return;
    }

    panic!(
        "trail_relation Node assertions failed (status: {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn node_script(body: &str) -> String {
    let mut script = String::from("const assert = require('node:assert/strict');\n");
    script.push_str("globalThis.window = globalThis;\n");
    script.push_str(include_str!("logic.js"));
    script.push('\n');
    script.push_str(body);
    script
}

#[test]
fn done_root_reveals_eligible_children() {
    let rows = serde_json::json!([
        {"id": 1, "quantity": -1, "parentIdsJson": "[]", "head": "Root"},
        {"id": 2, "quantity": 0, "parentIdsJson": "[1]", "head": "Child"},
        {"id": 3, "quantity": 0, "parentIdsJson": "[2]", "head": "Grandchild"},
    ]);

    let rows_json = serde_json::to_string(&rows).expect("rows json");
    let script = node_script(&format!(
        r#"
const rows = {rows_json};
const result = TrailRelationLogic.computeTrailQuantityChanges(rows, 1, 1, 1);
assert.deepStrictEqual(result, {{
  changes: [
    {{ recordId: 1, quantity: 1 }},
    {{ recordId: 2, quantity: -1 }},
  ],
  error: null,
}});

const updatedRows = rows.map((row) => {{
  if (row.id === 1) return {{ ...row, quantity: 1 }};
  if (row.id === 2) return {{ ...row, quantity: -1 }};
  return row;
}});

const visibleIds = TrailRelationLogic.visibleTrailRows(updatedRows, 1).map((row) => row.id);
assert.deepStrictEqual(visibleIds, [1, 2]);
assert.strictEqual(updatedRows.find((row) => row.id === 3).quantity, 0);
"#
    ));

    run_node_assertions(&script);
}

#[test]
fn incomplete_parents_keep_children_hidden() {
    let rows = serde_json::json!([
        {"id": 1, "quantity": -1, "parentIdsJson": "[]", "head": "Root"},
        {"id": 2, "quantity": 0, "parentIdsJson": "[]", "head": "Other parent"},
        {"id": 3, "quantity": 0, "parentIdsJson": "[1,2]", "head": "Blocked child"},
    ]);

    let rows_json = serde_json::to_string(&rows).expect("rows json");
    let script = node_script(&format!(
        r#"
const rows = {rows_json};
const result = TrailRelationLogic.computeTrailQuantityChanges(rows, 1, 1, 1);
assert.deepStrictEqual(result, {{
  changes: [{{ recordId: 1, quantity: 1 }}],
  error: null,
}});

const updatedRows = rows.map((row) => row.id === 1 ? {{ ...row, quantity: 1 }} : row);
const visibleIds = TrailRelationLogic.visibleTrailRows(updatedRows, 1).map((row) => row.id);
assert.deepStrictEqual(visibleIds, [1]);
assert.strictEqual(updatedRows.find((row) => row.id === 3).quantity, 0);
"#
    ));

    run_node_assertions(&script);
}

#[test]
fn zero_request_is_noop_for_completed_non_root_nodes() {
    let rows = serde_json::json!([
        {"id": 1, "quantity": 1, "parentIdsJson": "[]", "head": "Root"},
        {"id": 2, "quantity": -1, "parentIdsJson": "[1]", "head": "Child"},
    ]);

    let rows_json = serde_json::to_string(&rows).expect("rows json");
    let script = node_script(&format!(
        r#"
const rows = {rows_json};
const result = TrailRelationLogic.computeTrailQuantityChanges(rows, 1, 2, 0);
assert.deepStrictEqual(result, {{
  changes: [],
  error: null,
}});
"#
    ));

    run_node_assertions(&script);
}
