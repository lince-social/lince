use super::*;
use serde_json::json;

#[test]
fn independent_bindings_and_scalar_preferences_merge_without_text_or_list_splicing() {
    let base = json!({"overrides":{},"focus":{"click":true,"pointer_follow":false},"presets":[[1,1],[1,2]]});
    let local = json!({"overrides":{"item.grow-width":"Mod+F2"},"focus":{"click":true,"pointer_follow":false},"presets":[[1,1],[1,3]]});
    let file = json!({"overrides":{"desktop.operation":"Mod+P"},"focus":{"click":true,"pointer_follow":true},"presets":[[1,1],[1,2]]});
    let proposal = Proposal::new(&base, &local, &file).unwrap();
    assert!(proposal.resolved());
    assert_eq!(proposal.value, Some(json!({"overrides":{"item.grow-width":"Mod+F2","desktop.operation":"Mod+P"},"focus":{"click":true,"pointer_follow":true},"presets":[[1,1],[1,3]]})));
}

#[test]
fn removed_override_and_explicit_unbound_are_different_choices() {
    let base = json!({"overrides":{"command":"Mod+H"}});
    let local = json!({"overrides":{"command":null}});
    let file = json!({"overrides":{}});
    let mut proposal = Proposal::new(&base, &local, &file).unwrap();
    assert_eq!(proposal.conflicts.len(), 1);
    assert_eq!(proposal.conflicts[0].path, vec!["overrides", "command"]);
    assert!(!proposal.resolved());
    proposal.choose(0, Choice::Local).unwrap();
    assert_eq!(proposal.value, Some(local));
    proposal.choose(0, Choice::File).unwrap();
    assert_eq!(proposal.value, Some(file));
}

#[test]
fn conflicting_preset_and_rule_order_lists_are_atomic_fields() {
    let base = json!({"presets":[[1,1],[1,2]],"rules":["A","B"]});
    let local = json!({"presets":[[1,1],[1,3]],"rules":["B","A"]});
    let file = json!({"presets":[[2,3],[1,2]],"rules":["A","C","B"]});
    let mut proposal = Proposal::new(&base, &local, &file).unwrap();
    assert_eq!(proposal.conflicts.len(), 2);
    let preset = proposal.conflicts.iter().position(|conflict| conflict.path == ["presets"]).unwrap();
    let rules = proposal.conflicts.iter().position(|conflict| conflict.path == ["rules"]).unwrap();
    proposal.choose(preset, Choice::Local).unwrap();
    assert!(!proposal.resolved());
    proposal.choose(rules, Choice::File).unwrap();
    assert!(proposal.resolved());
    assert_eq!(proposal.value, Some(json!({"presets":[[1,1],[1,3]],"rules":["A","C","B"]})));
}

#[test]
fn identical_edits_are_not_conflicts_and_review_work_is_bounded() {
    let edited = json!({"chord":"Mod+Shift+H"});
    let proposal = Proposal::new(&json!({"chord":"Mod+H"}), &edited, &edited).unwrap();
    assert!(proposal.resolved());
    assert_eq!(proposal.value, Some(edited));
    assert!(Proposal::new(&json!({}), &json!({"large":"x".repeat(super::super::super::LIMIT + 1)}), &json!({})).is_err());
    let mut nested = json!(1);
    for _ in 0..18 { nested = json!({"next":nested}); }
    assert!(Proposal::new(&nested, &nested, &nested).is_err());
}
