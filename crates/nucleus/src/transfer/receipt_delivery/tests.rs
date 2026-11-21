use super::*;
use crate::{
    karma::canonical_hash,
    new_uid,
    transfer::receipt::{Participation, ParticipationRequest},
};

fn private_request() -> Delivery {
    let sender = new_uid("r");
    let recipient = new_uid("r");
    let transfer = new_uid("r");
    let party = new_uid("r");
    let commitment = Participation::party_commitment(&sender, &transfer, 1, &party).unwrap();
    Delivery {
        domain: DOMAIN.into(),
        sender: sender.clone(),
        recipients: vec![recipient.clone()],
        issued_at: 1000,
        expires_at: 2000,
        artifact: Artifact::Participation(ParticipationRequest {
            public_history_terms: None,
            transfer,
            party: Some(party),
            terms: Participation {
                public_history: None,
                organ: recipient,
                origin: sender,
                agreement_revision: 1,
                agreement_commitment: canonical_hash(DOMAIN, &"agreement").unwrap(),
                party_commitment: Some(commitment),
            },
        }),
    }
}

#[test]
fn participation_delivery_is_bound_to_the_named_approver_and_a_bounded_term() {
    let delivery = private_request();
    delivery.validate(1000).unwrap();
    let bytes = serde_json::to_vec(&delivery).unwrap();
    assert_eq!(Delivery::parse(&bytes, 1000).unwrap(), delivery);
    let mut changed = delivery.clone();
    changed.recipients = vec![new_uid("r")];
    assert!(changed.validate(1000).is_err());
    changed = delivery.clone();
    changed.expires_at = changed.issued_at + MAX_SECONDS + 1;
    assert!(changed.validate(1000).is_err());
    changed = delivery.clone();
    changed.issued_at = 1301;
    assert!(changed.validate(1000).is_err());
    let mut unknown = serde_json::to_value(delivery).unwrap();
    unknown["execute"] = serde_json::json!({"action":"settle"});
    assert!(Delivery::parse(&serde_json::to_vec(&unknown).unwrap(), 1000).is_err());
    assert!(Delivery::parse(&vec![b' '; MAX_BYTES + 1], 1000).is_err());
}

#[test]
fn delivery_controls_keep_inspection_and_preview_read_only() {
    let request = Request {
        source: Source::Document {
            record: new_uid("r"),
            expected_version: 1,
        },
        thread: new_uid("r"),
        recipients: vec![new_uid("r")],
        expires_at: 2000,
    };
    assert!(
        !Command::Preview {
            request: request.clone()
        }
        .is_mutation()
    );
    assert!(
        !Command::Inspect {
            message: new_uid("r")
        }
        .is_mutation()
    );
    assert!(
        Command::Send {
            request,
            reviewed_at: 1000,
            expected_projection: canonical_hash(DOMAIN, &"review").unwrap()
        }
        .is_mutation()
    );
    assert!(
        Command::Import {
            message: new_uid("r"),
            expected_projection: canonical_hash(DOMAIN, &"artifact").unwrap()
        }
        .is_mutation()
    );
}
