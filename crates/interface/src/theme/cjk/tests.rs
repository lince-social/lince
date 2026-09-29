use super::*;
use bevy::text::{FontCx, LayoutCx};
use std::sync::atomic::{AtomicUsize, Ordering};

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<bevy::text::RemSize>()
        .add_plugins(super::super::TypographyPlugin)
        .add_systems(
            PostUpdate,
            bevy::text::load_font_assets_into_font_collection,
        )
        .add_systems(
            PostUpdate,
            bevy::ui::widget::update_editable_text_styles.in_set(bevy::ui::UiSystems::Content),
        );
    app.world_mut().resource_mut::<FontCx>().collection =
        fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
    app.update();
    app.world_mut()
        .resource_mut::<FontCx>()
        .set_sans_serif_family("Lato")
        .unwrap();
    app
}

fn glyphs(world: &mut World, entity: Entity) {
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            let mut input = world.get_mut::<EditableText>(entity).unwrap();
            let value = input.editor.raw_text().to_owned();
            let shaped = input.editor.layout(&mut fonts.context, &mut layout.0);
            let mut count = 0;
            for line in shaped.lines() {
                for run in line.runs() {
                    for cluster in run.clusters() {
                        for glyph in cluster.glyphs() {
                            assert_ne!(glyph.id, 0, "Missing glyph in {value}");
                            count += 1;
                        }
                    }
                }
            }
            assert!(count > 0);
        });
    });
}

#[test]
fn latin_text_never_loads_the_cjk_font() {
    let mut app = app();
    let count = app.world().resource::<Assets<Font>>().len();
    app.world_mut()
        .spawn(Text::new("Português, Français, Ελληνικά, Русский"));
    let editor = app
        .world_mut()
        .spawn((Node::default(), EditableText::new("ordinary text")))
        .id();
    for _ in 0..8 {
        app.world_mut()
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("café and more ASCII text");
        app.update();
        assert!(app.world().resource::<Fallback>().0.is_none());
        assert_eq!(app.world().resource::<Assets<Font>>().len(), count);
        assert!(
            app.world_mut()
                .resource_mut::<FontCx>()
                .collection
                .family_id(FAMILY)
                .is_none()
        );
    }
}

#[test]
fn a_text_span_loads_one_shared_font_without_copying_its_bytes() {
    let mut app = app();
    let count = app.world().resource::<Assets<Font>>().len();
    let root = app.world_mut().spawn(Text::new("Label: ")).id();
    app.world_mut()
        .spawn((TextSpan::new("中文"), ChildOf(root)));
    app.update();
    let handle = app.world().resource::<Fallback>().0.clone().unwrap();
    let data = &app
        .world()
        .resource::<Assets<Font>>()
        .get(&handle)
        .unwrap()
        .data;
    assert_eq!(data.as_ref().as_ptr(), DATA.as_ptr());
    let editor = app
        .world_mut()
        .spawn((
            Node::default(),
            EditableText::new("ABC 中文 日本語 かな カナ 한글 ㄱ 가 ㄅ ，。「」"),
        ))
        .id();
    app.update();
    glyphs(app.world_mut(), editor);
    let changed = app
        .world()
        .entity(editor)
        .get_ref::<TextFont>()
        .unwrap()
        .last_changed();
    for _ in 0..8 {
        app.update();
        assert_eq!(app.world().resource::<Assets<Font>>().len(), count + 1);
        assert_eq!(
            app.world().resource::<Fallback>().0.as_ref().unwrap().id(),
            handle.id()
        );
        assert_eq!(
            app.world()
                .entity(editor)
                .get_ref::<TextFont>()
                .unwrap()
                .last_changed(),
            changed
        );
    }
}

#[test]
fn late_text_changes_wake_the_host_and_refresh_existing_fonts_once() {
    let mut app = app();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    app.insert_resource(crate::wake::WakeSignal::new(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    }));
    let editor = app
        .world_mut()
        .spawn((Node::default(), EditableText::new("plain")))
        .id();
    app.update();
    let old_tick = app
        .world()
        .entity(editor)
        .get_ref::<TextFont>()
        .unwrap()
        .last_changed();
    app.add_systems(
        PostUpdate,
        (move |mut inputs: Query<&mut EditableText>, mut sent: Local<bool>| {
            if !*sent {
                inputs
                    .get_mut(editor)
                    .unwrap()
                    .editor
                    .set_text("plain 中文");
                *sent = true;
            }
        })
        .after(bevy::ui::UiSystems::Content),
    );
    app.update();
    assert!(app.world().resource::<Fallback>().0.is_some());
    assert!(
        app.world_mut()
            .resource_mut::<FontCx>()
            .collection
            .family_id(FAMILY)
            .is_none()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    app.update();
    assert_ne!(
        app.world()
            .entity(editor)
            .get_ref::<TextFont>()
            .unwrap()
            .last_changed(),
        old_tick
    );
    glyphs(app.world_mut(), editor);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn fallback_survives_an_unrelated_font_unloading() {
    let mut app = app();
    let editor = app
        .world_mut()
        .spawn((Node::default(), EditableText::new("中文 日本語 한글")))
        .id();
    let temporary = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(
            include_bytes!("../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        ));
    app.update();
    glyphs(app.world_mut(), editor);
    app.world_mut()
        .resource_mut::<Assets<Font>>()
        .remove(temporary.id());
    app.update();
    glyphs(app.world_mut(), editor);
}

#[test]
fn preedit_loads_the_font_without_committing_or_moving_the_selection() {
    let mut app = app();
    let editor = app
        .world_mut()
        .spawn((Node::default(), EditableText::new("A")))
        .id();
    app.update();
    app.world_mut()
        .resource_scope(|world, mut fonts: Mut<FontCx>| {
            world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                world
                    .get_mut::<EditableText>(editor)
                    .unwrap()
                    .editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .set_compose("猫한글かな", Some((3, 6)));
            });
        });
    let input = app.world().get::<EditableText>(editor).unwrap();
    let selection = [
        input.editor.raw_selection().anchor().index(),
        input.editor.raw_selection().focus().index(),
    ];
    let compose = input.editor.raw_compose().clone();
    let raw = input.editor.raw_text().to_owned();
    assert_eq!(input.value().to_string(), "A");
    assert!(app.world().resource::<Fallback>().0.is_none());
    app.update();
    glyphs(app.world_mut(), editor);
    let input = app.world().get::<EditableText>(editor).unwrap();
    assert_eq!(input.value().to_string(), "A");
    assert_eq!(input.editor.raw_text(), raw);
    assert_eq!(input.editor.raw_compose(), &compose);
    assert_eq!(
        [
            input.editor.raw_selection().anchor().index(),
            input.editor.raw_selection().focus().index(),
        ],
        selection
    );
    app.world_mut()
        .resource_scope(|world, mut fonts: Mut<FontCx>| {
            world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                let mut input = world.get_mut::<EditableText>(editor).unwrap();
                let mut driver = input.editor.driver(&mut fonts.context, &mut layout.0);
                driver.finish_compose();
                driver.select_byte_range(0, 3);
            });
        });
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .unwrap()
            .editor
            .selected_text(),
        Some("猫")
    );
}
