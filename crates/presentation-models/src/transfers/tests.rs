// non-lean-yet

use super::*;
use serde_json::json;

#[test]
fn malformed_draft_payload_returns_an_error() {
    let mut form = composer("ana", "Donation");
    form.data = json!(["not a draft"]);

    assert_eq!(
        form.payload(),
        Err("Transfer draft must be an object".into())
    );
}

#[test]
fn counterparty_prefill_requires_one_distinct_other_person() {
    for (invitees, receiver) in [
        (json!(["beto", "beto"]), "beto"),
        (json!(["beto", "clara"]), ""),
    ] {
        let mut form = composer("ana", "Donation");
        form.step = Some(2);
        form.data["invitees"] = invitees;

        fields::composer_fields(&mut form);

        assert_eq!(
            form.data["promises"][0]["item"]["exchange"]["receiver"],
            receiver
        );
    }
}
