use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use nucleus::transfer::receipt::{PersonApproval, PublicPerson};

#[test]
fn client_consent_signs_only_the_public_scope_and_cannot_impersonate_another_person() {
    support::run_async_test("receipt-client-person-consent", || async {
        let fixture = Fixture::new(true).await;
        let alice = support::person(&fixture.reader, "consent.alice").await;
        fixture
            .reader
            .set_signer(alice.signer.clone())
            .await
            .unwrap();
        let mut scope = fixture.bundle.publication.scope.clone();
        scope.public_persons = vec![PublicPerson {
            role: ApprovalRole::Receiver,
            person: alice.uid.clone(),
        }];
        let private = nucleus::new_uid("r");
        let record = nucleus::new_uid("r");
        let approval = PersonApproval {
            scope: scope.id().unwrap(),
            role: ApprovalRole::Receiver,
            person: alice.uid.clone(),
            issued_at: fixture.now.timestamp(),
            key_id: String::new(),
            signature: String::new(),
        };
        let mut action = receipt_action(
            &alice.uid,
            "client:consent",
            Command::ApprovePerson {
                record: record.clone(),
                expected_version: 1,
                approval,
                private_transfer: Some(private.clone()),
            },
        );
        engine::transfer_receipts::sign_person_consent(&mut action, &alice.signer).unwrap();
        let Action::TransferReceipt {
            request: Command::ApprovePerson { approval, .. },
            ..
        } = &action
        else {
            unreachable!();
        };
        let encoded = serde_json::to_string(approval).unwrap();
        assert!(!encoded.contains(&private));
        assert!(!encoded.contains(&record));
        assert_eq!(approval.signature.len(), 88);
        let review = Document::Review(Review {
            scope: scope.clone(),
            person_approvals: vec![approval.clone()],
            approvals: Vec::new(),
            authorities: Vec::new(),
        });
        fixture
            .reader
            .verify_transfer_receipt_document(&review, fixture.now)
            .await
            .unwrap();
        let mut weak_key = [0u8; 32];
        weak_key[0] = 1;
        let mut forged_signature = [0u8; 64];
        forged_signature[0] = 1;
        engine::trust::adopt_key(
            &fixture.reader.store,
            &alice.uid,
            "weak-consent-v1",
            &STANDARD.encode(weak_key),
        )
        .await
        .unwrap();
        let mut weak_consent = review.clone();
        let Document::Review(weak_review) = &mut weak_consent else {
            unreachable!();
        };
        weak_review.person_approvals[0].key_id = "weak-consent-v1".into();
        weak_review.person_approvals[0].signature = STANDARD.encode(forged_signature);
        assert_eq!(
            fixture
                .reader
                .verify_transfer_receipt_document(&weak_consent, fixture.now)
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_invalid")
        );
        let local = store::organs::local(&fixture.reader.store.pool)
            .await
            .unwrap()
            .unwrap();
        let mut organ_scope = scope.clone();
        organ_scope.public_persons[0].person = local.uid.clone();
        let masquerade = Document::Review(Review {
            scope: organ_scope.clone(),
            approvals: Vec::new(),
            authorities: Vec::new(),
            person_approvals: vec![PersonApproval {
                scope: organ_scope.id().unwrap(),
                person: local.uid,
                role: ApprovalRole::Receiver,
                issued_at: fixture.now.timestamp(),
                key_id: "not-a-person".into(),
                signature: "a".repeat(88),
            }],
        });
        assert_eq!(
            fixture
                .reader
                .verify_transfer_receipt_document(&masquerade, fixture.now)
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_invalid")
        );
        let mut changed = action.clone();
        let Action::TransferReceipt { person, .. } = &mut changed else {
            unreachable!();
        };
        *person = Some(nucleus::new_uid("r"));
        assert_eq!(
            engine::transfer_receipts::sign_person_consent(&mut changed, &alice.signer)
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let ordinary = Approval {
            role: ApprovalRole::Receiver,
            issued_at: approval.issued_at,
            key_id: approval.key_id.clone(),
            signature: String::new(),
        };
        assert_ne!(
            approval.signing_bytes().unwrap(),
            ordinary.signing_bytes(&scope).unwrap()
        );
        let mut wrong_purpose = review;
        let Document::Review(review) = &mut wrong_purpose else {
            unreachable!();
        };
        review.person_approvals[0].signature = alice
            .signer
            .sign_bytes(&ordinary.signing_bytes(&scope).unwrap());
        assert_eq!(
            fixture
                .reader
                .verify_transfer_receipt_document(&wrong_purpose, fixture.now)
                .await
                .unwrap_err()
                .code(),
            Some("transfer_receipt_invalid")
        );
    });
}
