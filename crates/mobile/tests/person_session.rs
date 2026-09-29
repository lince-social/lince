use cell::{ClientMessage, ServerMessage};
use engine::{actions::Action, private_password::PasswordInput};
use lince_mobile::session::Signing;

#[tokio::test]
async fn person_changes_are_signed_and_stopped_by_signout() {
    let directory = tempfile::tempdir().unwrap();
    let cell = cell::Cell::open_mobile(cell::CellOptions {
        data_dir: Some(directory.path().into()),
        peer_port: Some(0),
        ..Default::default()
    })
    .await
    .unwrap();
    let runtime = cell.runtime();
    let person = runtime
        .engine
        .act(
            Action::CreateUser {
                username: "mobile-owner".into(),
                name: "Mobile owner".into(),
                password: "disposable-test-password".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        Signing::open(
            runtime,
            "mobile-owner",
            PasswordInput::new(b"wrong-password".to_vec()).unwrap()
        )
        .await
        .is_err()
    );
    let (mut session, mut signing) = Signing::open(
        runtime,
        "mobile-owner",
        PasswordInput::new(b"disposable-test-password".to_vec()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(signing.login.person_uid(), person);
    let request = ClientMessage::Act {
        id: "signed-mobile-change".into(),
        action: Action::CreateRecordDraft {
            draft: engine::record_creation::Draft {
                head: "Written by a Person".into(),
                ..Default::default()
            },
        },
    };
    let unsigned = session.handle(request.clone()).await;
    assert!(unsigned.iter().any(|reply| matches!(reply, ServerMessage::Error { code: Some(code), .. } if code == "action_intent_required")));
    let signed = signing.sign(request).unwrap();
    let responses = session.handle(signed.clone()).await;
    assert!(
        responses
            .iter()
            .all(|reply| !matches!(reply, ServerMessage::Error { .. })),
        "{responses:?}"
    );
    drop(signing);
    let responses = session.handle(signed).await;
    assert!(responses.iter().any(|reply| matches!(reply, ServerMessage::Error { code: Some(code), .. } if code == "session_expired")));
    cell.shutdown().await;
}
