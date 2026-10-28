use super::*;

fn invocation() -> Invocation {
    Invocation { command: "item.cycle-width".into(), context: Context {
        owner: 1, canvas: "canvas-fixture".into(), workspace: 7, output: Some(2),
        item: Some(Item::Application { placement: [3; 16], surface: 9 }),
    }, arguments: Arguments::None }
}

#[test]
fn invocation_validates_typed_targets_and_rejects_shell_text_and_absent_identity() {
    let valid = invocation();
    assert!(valid.validate().is_ok());
    for command in ["", "sh -c true", "$(program)", "window/close", "focus.left\0"] {
        let mut invalid = valid.clone();
        invalid.command = command.into();
        assert_eq!(invalid.validate().unwrap_err().code, Error::InvalidArguments);
    }
    let mut invalid = valid.clone();
    invalid.context.workspace = 0;
    assert!(invalid.validate().is_err());
    invalid = valid;
    invalid.arguments = Arguments::Workspace { id: 0 };
    assert!(invalid.validate().is_err());
}

#[test]
fn diagnostic_messages_remain_bounded_at_utf8_boundaries() {
    let error = Failure::new(Error::Denied, "é".repeat(10000));
    assert_eq!(error.message.len(), 4096);
    assert_eq!(error.message.chars().count(), 2048);
    let boundary = Failure::new(Error::Denied, format!("{}😀", "a".repeat(4095)));
    assert_eq!(boundary.message.len(), 4095);
    assert_eq!(Failure::new(Error::Busy, "Try again").message, "Try again");
}
