use engine::{Engine, actions::Action};
use nucleus::{
    DecimalValue,
    karma::{Cadence, Consequence},
};

pub(crate) async fn create_recurrence(
    engine: &Engine,
    target: String,
    concept: Option<&str>,
    amount: &str,
    cadence: Cadence,
    anchor: String,
    note: Option<&str>,
) -> String {
    engine
        .act(
            Action::CreateRecurrence {
                target,
                consequences: vec![Consequence::CaptureEntry {
                    amount: DecimalValue::parse_inferred(amount).expect("valid fixture amount"),
                    concept: concept.map(str::to_owned),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: note.map(str::to_owned),
                cadence,
                anchor_at: Some(anchor),
                request_id: Some("rule-1".into()),
            },
            None,
        )
        .await
        .expect("create fixture rule")
        .created
        .expect("created fixture rule UID")
}
