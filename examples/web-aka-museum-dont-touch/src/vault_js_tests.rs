use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const VAULT_JS: &str = include_str!("../static/presentation/board/vault.js");

const PRELUDE: &str = r#"
import assert from "node:assert/strict";
import { LOCKED_LABEL, VAULT_MARKER, isLocked } from "./vault.mjs";
"#;

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn stage_and_run(label: &str, body: &str) {
    if !node_available() {
        eprintln!("SKIP vault_js test `{label}`: node is not available on PATH");
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "lince-vault-js-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create vault js test dir");

    let modules = [("vault", VAULT_JS)];
    for (name, source) in modules {
        let rewritten = source.replace(".js\"", ".mjs\"");
        fs::write(dir.join(format!("{name}.mjs")), rewritten).expect("stage vault module");
    }

    let script = format!("{PRELUDE}\n{body}\n");
    let test_path = dir.join("test.mjs");
    fs::write(&test_path, script).expect("write vault js test");

    let output = Command::new("node")
        .arg(&test_path)
        .current_dir(&dir)
        .output()
        .expect("failed to launch node");

    let _ = fs::remove_dir_all(&dir);

    if output.status.success() {
        return;
    }

    panic!(
        "vault js assertions failed (status: {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn a_locked_description_is_recognised_before_it_is_rendered() {
    stage_and_run(
        "vault-locked-description",
        r#"
const envelope =
  VAULT_MARKER + " m=19456,t=2,p=1 c2FsdHNhbHRzYWx0c2E= " +
  "bm9uY2Vub25jZW5vbmNlbm9uY2Vub24= Y2lwaGVydGV4dA==";

assert.equal(isLocked(envelope), true);
assert.equal(isLocked("an ordinary description"), false);
assert.equal(isLocked(VAULT_MARKER + " not an envelope"), false);
assert.equal(isLocked(null), false);
assert.ok(LOCKED_LABEL.length > 0);
"#,
    );
}
