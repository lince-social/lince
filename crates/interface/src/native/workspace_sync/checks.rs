use super::*;
use bevy::{
    ecs::system::RunSystemOnce,
    text::{PreeditCursor, TextEdit, TextLayoutInfo},
    ui::ContentSize,
};

pub(crate) fn password_retention(app: &mut App, root: Entity) {
    app.add_message::<crate::native::cell_bridge::CellMessage>()
        .add_plugins(WorkspaceSyncPlugin);
    let sand = app.world_mut().spawn((Node::default(), ChildOf(root))).id();
    let owner = populate(app.world_mut(), root, sand);
    let view = app.world().get::<WorkspaceSync>(owner).unwrap();
    let (password, mask) = (view.password, view.password_mask);
    let settle = |app: &mut App| {
        for _ in 0..4 {
            app.update();
        }
    };
    let ticks = |world: &World| {
        [
            world
                .entity(password)
                .get_ref::<EditableText>()
                .unwrap()
                .last_changed()
                .get(),
            world
                .entity(mask)
                .get_ref::<Text>()
                .unwrap()
                .last_changed()
                .get(),
            world
                .entity(mask)
                .get_ref::<ContentSize>()
                .unwrap()
                .last_changed()
                .get(),
            world
                .entity(mask)
                .get_ref::<TextLayoutInfo>()
                .unwrap()
                .last_changed()
                .get(),
        ]
    };
    settle(app);
    let empty = ticks(app.world());
    for _ in 0..12 {
        app.update();
        assert_eq!(
            ticks(app.world()),
            empty,
            "Unchanged password dirtied native text or layout"
        );
    }
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "");
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .queue_edit(TextEdit::Insert("aé🦊".into()));
    settle(app);
    assert_eq!(
        app.world().get::<EditableText>(password).unwrap().value(),
        "aé🦊"
    );
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "•••");
    assert!(
        !app.world()
            .get::<TextLayoutInfo>(mask)
            .unwrap()
            .glyphs
            .is_empty()
    );
    assert_eq!(
        app.world().get::<TextColor>(password).unwrap().0,
        Color::NONE
    );
    assert!(
        app.world()
            .get::<crate::native::sand_text::SandText>(password)
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<bevy::a11y::AccessibilityNode>(password)
            .unwrap()
            .role(),
        accesskit::Role::PasswordInput
    );
    let filled = ticks(app.world());
    for _ in 0..12 {
        app.update();
        assert_eq!(ticks(app.world()), filled);
    }
    {
        let mut field = app.world_mut().get_mut::<EditableText>(password).unwrap();
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Insert("rés".into()));
    }
    settle(app);
    assert_eq!(
        app.world().get::<EditableText>(password).unwrap().value(),
        "rés"
    );
    assert_eq!(
        ticks(app.world())[1..],
        filled[1..],
        "Equal-length password invalidated its mask"
    );
    for protect in [false, true] {
        app.world_mut()
            .get_mut::<EditableText>(password)
            .unwrap()
            .pending_edits = vec![
            TextEdit::Copy,
            TextEdit::Insert("x".into()),
            TextEdit::Paste,
            TextEdit::ImeCommit { value: "é".into() },
            TextEdit::Cut,
        ];
        if protect {
            app.world_mut().run_system_once(protect_passwords).unwrap();
        } else {
            maintain(app.world_mut());
        }
        let field = app.world().get::<EditableText>(password).unwrap();
        assert!(
            matches!(&field.pending_edits[..], [TextEdit::Insert(value), TextEdit::Paste, TextEdit::ImeCommit { value: commit }] if value == "x" && commit == "é")
        );
        app.world_mut()
            .get_mut::<EditableText>(password)
            .unwrap()
            .pending_edits
            .clear();
    }
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .queue_edit(TextEdit::ImeSetCompose {
            value: "é".into(),
            cursor: Some(PreeditCursor {
                anchor: 2,
                focus: 2,
            }),
        });
    settle(app);
    assert!(
        app.world()
            .get::<EditableText>(password)
            .unwrap()
            .is_composing()
    );
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "•••");
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .queue_edit(TextEdit::ImeCommit { value: "é".into() });
    settle(app);
    assert_eq!(
        app.world().get::<EditableText>(password).unwrap().value(),
        "résé"
    );
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "••••");
    {
        let mut field = app.world_mut().get_mut::<EditableText>(password).unwrap();
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Insert("🦊".repeat(40).into()));
    }
    settle(app);
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "•".repeat(32));
    let capped = ticks(app.world());
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .queue_edit(TextEdit::Insert("z".into()));
    settle(app);
    assert_eq!(ticks(app.world())[1..], capped[1..]);
    {
        let mut field = app.world_mut().get_mut::<EditableText>(password).unwrap();
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Insert("".into()));
    }
    settle(app);
    assert_eq!(
        app.world().get::<EditableText>(password).unwrap().value(),
        ""
    );
    assert_eq!(app.world().get::<Text>(mask).unwrap().0, "");
    assert!(
        app.world()
            .get::<TextLayoutInfo>(mask)
            .unwrap()
            .glyphs
            .is_empty()
    );
}
