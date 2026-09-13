use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::{ExtractSchedule, RenderApp},
    text::{EditableText, TextLayoutInfo},
    ui::widget::TextScroll,
    ui_render::{ExtractedUiItem, ExtractedUiNodes, RenderUiSystems},
    winit::WinitSettings,
};
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone, Default)]
struct RenderedClips(Arc<Mutex<Vec<(Entity, Option<Rect>)>>>);

fn capture_clips(nodes: Res<ExtractedUiNodes>, clips: Res<RenderedClips>) {
    let mut clips = clips.0.lock().unwrap();
    clips.clear();
    clips.extend(
        nodes
            .uinodes
            .iter()
            .filter(|node| matches!(node.item, ExtractedUiItem::Glyphs { .. }))
            .map(|node| (*node.main_entity, node.clip)),
    );
}
use lince_interface::{
    actions::Action,
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    edit_mode::EditAction,
    layout::{
        self, LayoutBox, LayoutRuntime, Overflow as LayoutOverflow, Rules, Sizing,
        panel::LayoutAction,
    },
    sand_store::{SandKind, StoredSand, spawn_sand},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    square: Entity,
    title: Entity,
    text: Entity,
    fixed: Vec2,
}

fn fit(size: Vec2) -> Rules {
    let mut rules = Rules::fixed(size);
    for axis in &mut rules.axes {
        axis.sizing = Sizing::Fit;
        axis.min = 24.0;
        axis.max = 1000.0;
    }
    rules
}

fn setup(world: &mut World) {
    world.spawn(BoxRoot);
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 20 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        let square = spawn_sand(world, root, 1, SandKind::Square, "", DVec2::ZERO);
        let title = spawn_sand(
            world,
            root,
            1,
            SandKind::EditableText,
            "A title that grows with its square",
            DVec2::ZERO,
        );
        let text = world.get::<StoredSand>(title).unwrap().content.unwrap();
        world
            .get_mut::<lince_interface::sand_text::SandText>(text)
            .unwrap()
            .offset = [0.0; 2];
        layout::configure(world, square, fit(Vec2::splat(100.0))).unwrap();
        layout::configure(world, title, fit(Vec2::splat(100.0))).unwrap();
        let mut rules = fit(Vec2::new(100.0, 24.0));
        rules.wrap = false;
        layout::configure(world, text, rules).unwrap();
        layout::attach(world, title, square).unwrap();
        world.get_mut::<LayoutBox>(title).unwrap().offset = [0.0; 2];
        world.insert_resource(Fixture {
            root,
            square,
            title,
            text,
            fixed: Vec2::ZERO,
        });
    }
    if !world.contains_resource::<Fixture>() {
        return;
    }
    let fixture = world.resource::<Fixture>();
    let (root, square, title, text) = (fixture.root, fixture.square, fixture.title, fixture.text);
    match frame {
        55 => {
            let measured = world.get::<TextLayoutInfo>(text).unwrap().size;
            assert!(
                measured.x > 200.0,
                "The real font must be measured before fitting: {measured:?}"
            );
            let outer = world.get::<CanvasItem>(square).unwrap().size;
            let inner = world.get::<CanvasItem>(title).unwrap().size;
            assert!(outer.x >= measured.x.ceil());
            assert_eq!(outer, inner);
            world
                .get_mut::<EditableText>(text)
                .unwrap()
                .editor
                .set_text(&"long title ".repeat(90));
            let mut rules = Rules::fixed(Vec2::new(160.0, 56.0));
            rules.wrap = false;
            for axis in &mut rules.axes {
                axis.overflow = LayoutOverflow::Scroll;
            }
            layout::configure(world, text, rules).unwrap();
        }
        85 => {
            let outer = world.get::<CanvasItem>(square).unwrap().size;
            assert_eq!(outer, Vec2::new(160.0, 56.0));
            assert!(world.get::<LayoutRuntime>(text).unwrap().content.x > 160.0);
            world.resource_mut::<Fixture>().fixed = outer;
            world.get_mut::<TextScroll>(text).unwrap().0.x = 100.0;
            world.get_mut::<CanvasView>(root).unwrap().zoom = 1.5;
            EditAction::Open.apply(world, root);
            LayoutAction::Open.apply(world, text);
        }
        100 => {
            assert_eq!(
                world.get::<CanvasItem>(square).unwrap().size,
                world.resource::<Fixture>().fixed
            );
            assert!(world.get::<TextScroll>(text).unwrap().0.x > 0.0);
            let clips = world.resource::<RenderedClips>().0.lock().unwrap();
            let text_clips: Vec<_> = clips.iter().filter(|(entity, _)| *entity == text).collect();
            assert!(
                !text_clips.is_empty(),
                "The scrolled text must reach the renderer"
            );
            let pixels = world
                .get::<lince_interface::topology::presentation::Surface>(title)
                .unwrap()
                .pixels
                .as_vec2();
            assert!(
                text_clips
                    .iter()
                    .all(|(_, clip)| clip
                        .is_some_and(|clip| clip.size().min_element() > 0.0
                            && clip.size().cmple(pixels).all())),
                "Rendered glyphs must stay inside the Sand texture at the current zoom"
            );
            drop(clips);
            assert_eq!(
                world
                    .query::<&layout::panel::LayoutPanel>()
                    .iter(world)
                    .count(),
                1
            );
            let panel = world
                .query_filtered::<Entity, With<layout::panel::LayoutPanel>>()
                .single(world)
                .unwrap();
            let fields: Vec<_> = world
                .query::<(&EditableText, &ChildOf, &ComputedNode)>()
                .iter(world)
                .filter(|(_, parent, _)| parent.parent() == panel)
                .map(|(_, _, node)| node.size().y)
                .collect();
            assert_eq!(fields.len(), 12);
            assert!(
                fields.iter().all(|height| *height >= 16.0),
                "Layout fields must remain readable: {fields:?}"
            );
            LayoutAction::Apply.apply(world, text);
            assert!(
                world
                    .get::<lince_interface::sand_text::SandText>(text)
                    .unwrap()
                    .validate()
            );
            LayoutAction::Close.apply(world, text);
            let mut rules = fit(Vec2::new(160.0, 56.0));
            rules.axes[0].sizing = Sizing::Fixed;
            layout::configure(world, text, rules).unwrap();
            world
                .get_mut::<EditableText>(text)
                .unwrap()
                .editor
                .set_text(&"wrapped title ".repeat(30));
        }
        130 => {
            assert_eq!(world.get::<CanvasItem>(square).unwrap().size.x, 160.0);
            assert!(world.get::<CanvasItem>(square).unwrap().size.y > 56.0);
            world
                .get_mut::<EditableText>(text)
                .unwrap()
                .editor
                .set_text("Short");
        }
        160 => {
            assert!(world.get::<CanvasItem>(square).unwrap().size.y < 56.0);
            assert_eq!(world.get::<TextScroll>(text).unwrap().0, Vec2::ZERO);
            println!(
                "Layout smoke passed: measured title growth, fixed scrolling, zoomed clipping, layout controls, wrapping and shrinking."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
    assert!(frame < 190, "Layout smoke timed out");
}

#[tokio::main]
async fn main() {
    let clips = RenderedClips::default();
    let mut app = interface_app();
    app.sub_app_mut(RenderApp)
        .insert_resource(clips.clone())
        .add_systems(
            ExtractSchedule,
            capture_clips.after(RenderUiSystems::ExtractDebug),
        );
    app.insert_resource(clips)
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Update, exercise)
        .run();
}
