use super::*;
use bevy::text::{FontCx, LayoutCx};

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<FontCx>()
        .add_plugins(IthkuilPlugin)
        .add_systems(
            PostUpdate,
            bevy::text::load_font_assets_into_font_collection,
        );
    app.world_mut().resource_mut::<FontCx>().collection =
        fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
    app
}

#[test]
fn native_labels_share_font_bytes_and_keep_text_storage_unchanged_between_frames() {
    let mut app = app();
    let source = ScriptText::parse("adnila").unwrap();
    let expected = source.0.encoding().to_owned();
    let entity = app
        .world_mut()
        .spawn((Text::new("adnila"), source.clone()))
        .id();
    let span = app
        .world_mut()
        .spawn((TextSpan::new("adnila"), TextFont::default(), source))
        .id();
    app.update();
    assert_eq!(app.world().get::<Text>(entity).unwrap().0, expected);
    assert_eq!(app.world().get::<TextSpan>(span).unwrap().0, expected);
    assert_eq!(
        app.world().get::<AccessibleLabel>(entity).unwrap().0,
        "adnila"
    );
    let handle = app.world().resource::<ScriptFont>().0.as_ref().unwrap();
    let font = app.world().resource::<Assets<Font>>().get(handle).unwrap();
    assert!(std::ptr::eq(
        font.data.as_ref().as_ptr(),
        lince_ithkuil::FONT_BYTES.as_ptr()
    ));
    assert_eq!(app.world().resource::<Assets<Font>>().len(), 1);
    let pointer = app.world().get::<Text>(entity).unwrap().0.as_ptr();
    let text_tick = app
        .world()
        .entity(entity)
        .get_ref::<Text>()
        .unwrap()
        .last_changed();
    let font_tick = app
        .world()
        .entity(entity)
        .get_ref::<TextFont>()
        .unwrap()
        .last_changed();
    for _ in 0..100 {
        app.update();
        assert_eq!(app.world().get::<Text>(entity).unwrap().0.as_ptr(), pointer);
        assert_eq!(
            app.world()
                .entity(entity)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            text_tick
        );
        assert_eq!(
            app.world()
                .entity(entity)
                .get_ref::<TextFont>()
                .unwrap()
                .last_changed(),
            font_tick
        );
    }
}

#[test]
fn ordinary_labels_and_editable_buffers_keep_their_original_text_and_font() {
    let mut app = app();
    let label = app.world_mut().spawn(Text::new("Save")).id();
    let input = app
        .world_mut()
        .spawn((
            Text::new("adnila"),
            EditableText::new("adnila"),
            ScriptText::parse("adnila").unwrap(),
        ))
        .id();
    app.update();
    assert_eq!(app.world().get::<Text>(label).unwrap().0, "Save");
    assert_eq!(app.world().get::<Text>(input).unwrap().0, "adnila");
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .editor
            .raw_text(),
        "adnila"
    );
    assert!(app.world().resource::<ScriptFont>().0.is_none());
}

#[test]
fn labels_follow_updated_script_and_keep_external_font_size_changes() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Text::default(),
            TextFont::from_font_size(30.0),
            ScriptText::parse("adnila").unwrap(),
        ))
        .id();
    app.update();
    app.world_mut()
        .get_mut::<TextFont>(entity)
        .unwrap()
        .font_size = FontSize::Px(36.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(ScriptText::parse("amäla").unwrap());
    app.update();
    assert_eq!(
        app.world().get::<Text>(entity).unwrap().0,
        render("amäla").unwrap().encoding()
    );
    assert_eq!(
        app.world().get::<TextFont>(entity).unwrap().font_size,
        FontSize::Px(36.0)
    );
    assert_eq!(
        app.world().get::<AccessibleLabel>(entity).unwrap().0,
        "amäla"
    );
    assert_eq!(app.world().resource::<Assets<Font>>().len(), 1);
}

#[test]
fn native_glyphs_shape_in_the_bevy_text_engine_without_system_fonts() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((Text::default(), ScriptText::parse("aţčäla").unwrap()))
        .id();
    app.update();
    let value = app.world().get::<Text>(entity).unwrap().0.clone();
    let mut layout = LayoutCx::default();
    let mut input = EditableText::new(&value);
    let mut fonts = app.world_mut().resource_mut::<FontCx>();
    fonts
        .set_sans_serif_family(lince_ithkuil::FONT_FAMILY)
        .unwrap();
    let shaped = input.editor.layout(&mut fonts.context, &mut layout.0);
    let mut count = 0;
    for line in shaped.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                for glyph in cluster.glyphs() {
                    assert_ne!(glyph.id, 0);
                    count += 1;
                }
            }
        }
    }
    assert!(count > 0);
}
