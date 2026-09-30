use serde_json::json;

use super::routes::invocation;
use crate::scenario::{Event, Input, Scenario};

pub fn nested_parents(replicated: bool) -> Scenario {
    let mut case = super::independent_donation(replicated);
    case.name = format!("nested-parent-agreement-{replicated}");
    let original = std::mem::take(&mut case.inputs);
    let mut append = |cell: &str, id: &str, event: Event| {
        case.inputs.push(Input {
            id: id.into(),
            cell: cell.into(),
            at_ms: case.start_ms + case.inputs.len() as i64 * 20,
            event,
        });
    };
    for mut input in original {
        if input.id == "routes" {
            for (id, parent, visibility) in [
                ("parent", None, "public"),
                ("branch", Some("$parent"), "hidden"),
            ] {
                append(
                    "a",
                    id,
                    Event::Action {
                        invocation: invocation(
                            id,
                            json!({
                                "action":"create-transfer-draft", "creator":"$ana", "request_id":id, "slug":id,
                                "head":if id == "branch" { "Private required branch" } else { "Apple delivery plan" },
                                "agreement":"full", "visibility":visibility, "parent":parent, "promises":[]
                            }),
                        ),
                    },
                );
            }
            if let Event::Action { invocation } = &mut input.event
                && let engine::actions::Action::CreateTransferDraft { parent, .. } =
                    &mut invocation.action
            {
                *parent = Some("$branch".into());
            }
        }
        let id = input.id.clone();
        append(&input.cell, &input.id, input.event);
        if id == "routes" {
            append(
                "a",
                "parent-policy",
                Event::Action {
                    invocation: invocation(
                        "parent-policy",
                        json!({
                            "action":"configure-transfer-delivery", "transfer":"$parent", "recipient_person":"$beto", "recipient_organ":"$cell:b:organ",
                            "person":"$ana", "request_id":"parent-policy", "mode":if replicated {"replicated"} else {"hosted"}
                        }),
                    ),
                },
            );
        }
        if matches!(
            id.as_str(),
            "routes" | "agreement-b-2" | "application-confirmed"
        ) {
            let label = match id.as_str() {
                "routes" => "waiting",
                "agreement-b-2" => "agreed",
                _ => "settled",
            };
            let enqueue = format!("parent-enqueue-{label}");
            append(
                "a",
                &enqueue,
                Event::Action {
                    invocation: invocation(
                        &enqueue,
                        json!({
                            "action":"enqueue-transfer-delivery", "transfer":"$parent", "delivery":"$parent-policy", "person":"$ana", "request_id":enqueue
                        }),
                    ),
                },
            );
            append(
                "a",
                &format!("parent-send-{label}"),
                Event::TransferDelivery {
                    peer: "b".into(),
                    delay_ms: 1,
                    copies: 1,
                    duplicate_spacing_ms: 0,
                    drop: false,
                },
            );
        }
    }
    case
}
