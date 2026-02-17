use gpui::KeyDownEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Normal,
    Insert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableVimCommand {
    None,
    Save,
    CancelEdit,
    EnterInsertMode,
    EnterNormalMode,
    MoveLeft,
    MoveDown,
    MoveUp,
    MoveRight,
    MoveHome,
    MoveEnd,
    MoveWordBackward,
    MoveWordForward,
    DeleteBackward,
    DeleteForward,
    InsertNewline,
    InsertChar(char),
}

pub fn table_vim_command(event: &KeyDownEvent, mode: EditMode) -> TableVimCommand {
    match mode {
        EditMode::Insert => insert_mode_command(event),
        EditMode::Normal => normal_mode_command(event),
    }
}

fn insert_mode_command(event: &KeyDownEvent) -> TableVimCommand {
    if (event.keystroke.modifiers.control || event.keystroke.modifiers.alt)
        && event.keystroke.key.as_str() == "q"
    {
        return TableVimCommand::CancelEdit;
    }
    if (event.keystroke.modifiers.control || event.keystroke.modifiers.alt)
        && event.keystroke.key.as_str() == "s"
    {
        return TableVimCommand::Save;
    }
    match event.keystroke.key.as_str() {
        "enter" => return TableVimCommand::InsertNewline,
        "escape" => return TableVimCommand::EnterNormalMode,
        "backspace" => return TableVimCommand::DeleteBackward,
        "delete" => return TableVimCommand::DeleteForward,
        "left" | "arrowleft" => return TableVimCommand::MoveLeft,
        "down" | "arrowdown" => return TableVimCommand::MoveDown,
        "up" | "arrowup" => return TableVimCommand::MoveUp,
        "right" | "arrowright" => return TableVimCommand::MoveRight,
        "home" => return TableVimCommand::MoveHome,
        "end" => return TableVimCommand::MoveEnd,
        _ => {}
    }

    if let Some(key_char) = &event.keystroke.key_char {
        if key_char.chars().count() == 1
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.platform
        {
            if let Some(ch) = key_char.chars().next() {
                return TableVimCommand::InsertChar(ch);
            }
        }
    }

    TableVimCommand::None
}

fn normal_mode_command(event: &KeyDownEvent) -> TableVimCommand {
    if (event.keystroke.modifiers.control || event.keystroke.modifiers.alt)
        && event.keystroke.key.as_str() == "q"
    {
        return TableVimCommand::CancelEdit;
    }
    if (event.keystroke.modifiers.control || event.keystroke.modifiers.alt)
        && event.keystroke.key.as_str() == "s"
    {
        return TableVimCommand::Save;
    }
    match event.keystroke.key.as_str() {
        "enter" => TableVimCommand::MoveDown,
        "escape" => TableVimCommand::EnterNormalMode,
        "h" | "left" | "arrowleft" => TableVimCommand::MoveLeft,
        "j" | "down" | "arrowdown" => TableVimCommand::MoveDown,
        "k" | "up" | "arrowup" => TableVimCommand::MoveUp,
        "l" | "right" | "arrowright" => TableVimCommand::MoveRight,
        "i" => TableVimCommand::EnterInsertMode,
        "x" => TableVimCommand::DeleteForward,
        "b" | "B" => TableVimCommand::MoveWordBackward,
        "w" | "W" => TableVimCommand::MoveWordForward,
        "0" | "home" => TableVimCommand::MoveHome,
        "$" | "end" => TableVimCommand::MoveEnd,
        _ => TableVimCommand::None,
    }
}
