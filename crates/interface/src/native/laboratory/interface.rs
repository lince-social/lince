use super::driver::{Selector, UiCondition};
use crate::native::{actions::Action, edit_mode::EditAction};
use bevy::{a11y::AccessibilityNode, math::DVec2, prelude::*};

pub(super) fn fast_cases() -> Vec<super::Case> {
    let mut cases = Vec::new();
    laboratory_cases(&mut cases);
    let mut records = Vec::new();
    crate::native::protein_area::tests::laboratory_cases(&mut records);
    let mut selected = records.into_iter().filter(|case| {
        case.name
            .ends_with("::record_source_stays_stable_while_paused_and_typing_resumes_at_selection")
    });
    cases.push(
        selected
            .next()
            .expect("Missing native Record selection regression"),
    );
    assert!(
        selected.next().is_none(),
        "Duplicate native Record selection regression"
    );
    crate::native::record_binding::laboratory_cases(&mut cases);
    cases
}

pub fn run_interface_checks() -> std::io::Result<Vec<super::CaseResult>> {
    let (sender, incoming) = std::sync::mpsc::sync_channel(1);
    let wake = crate::native::wake::WakeSignal::new(move || {
        let _ = sender.try_send(());
    });
    let mut run = super::BehaviorRun::start_interface(Some(wake))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut reported = 0;
    loop {
        run.poll();
        for result in &run.results[reported..] {
            eprintln!(
                "{} {} ({:.1} ms)",
                if result.error.is_none() {
                    "PASS"
                } else {
                    "FAIL"
                },
                result.name,
                result.milliseconds
            );
        }
        reported = run.results.len();
        if run.finished() {
            if run.results.len() != run.total || run.total == 0 {
                return Err(std::io::Error::other(
                    "Laboratory interface coverage is incomplete",
                ));
            }
            return Ok(std::mem::take(&mut run.results));
        }
        if incoming
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .is_err()
        {
            run.cancel();
            return Err(std::io::Error::other(
                "Laboratory interface checks timed out",
            ));
        }
    }
}

#[test]
fn fast_interface_rules() {
    let results = run_interface_checks().unwrap();
    assert!(
        results[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(results.get(results.len() - 3).is_some_and(|result| {
        result
            .name
            .ends_with("::record_source_stays_stable_while_paused_and_typing_resumes_at_selection")
    }));
    assert!(results.get(results.len() - 2).is_some_and(|result| {
        result
            .name
            .ends_with("::splice_preserves_unicode_mutations_and_cursors")
    }));
    assert!(results.last().is_some_and(|result| {
        result
            .name
            .ends_with("::journal_updates_remain_incremental_across_acknowledgements")
    }));
    let expected = fast_cases();
    assert_eq!(results.len(), expected.len());
    assert!(
        results
            .iter()
            .zip(&expected)
            .all(|(result, case)| result.name == case.name)
    );
    let catalogue = super::catalogue();
    assert!(results.iter().all(|result| {
        catalogue
            .iter()
            .filter(|case| case.name == result.name)
            .count()
            == 1
    }));
    let failures: Vec<_> = results
        .iter()
        .filter(|result| result.error.is_some())
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
}

fn fixture(size: UVec2, scale: f32) -> (App, Entity) {
    let mut app = App::new();
    super::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
        bevy::picking::DefaultPickingPlugins,
        bevy::window::WindowPlugin {
            primary_window: Some(Window {
                resolution: (
                    (size.as_vec2() * scale).x as u32,
                    (size.as_vec2() * scale).y as u32,
                )
                    .into(),
                visible: false,
                focused: true,
                ..default()
            }),
            ..default()
        },
        bevy::image::ImagePlugin::default(),
        bevy::text::TextPlugin,
        bevy::transform::TransformPlugin,
        bevy::input_focus::InputFocusPlugin,
        bevy::ui_widgets::ButtonPlugin,
        bevy::ui::UiPlugin,
        crate::native::theme::ThemePlugin,
        crate::native::sand::SandPlugin,
        crate::native::workspace::WorkspacePlugin,
        crate::native::edit_mode::EditModePlugin,
    ))
    .add_plugins((
        bevy::mesh::MeshPlugin,
        bevy::camera::visibility::VisibilityPlugin,
        crate::native::icons::IconPlugin,
    ))
    .init_resource::<Assets<TextureAtlasLayout>>()
    .insert_resource(UiScale(scale));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: bevy::camera::ComputedCameraValues {
                target_info: Some(bevy::camera::RenderTargetInfo {
                    physical_size: (size.as_vec2() * scale).as_uvec2(),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
    ));
    let root = app
        .world_mut()
        .spawn((
            crate::native::container::BoxRoot,
            Node {
                width: px(size.x),
                height: px(size.y),
                ..default()
            },
        ))
        .id();
    app.update();
    let toggle = app
        .world()
        .get::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .toggle;
    let toolbar = app.world().get::<ChildOf>(toggle).unwrap().parent();
    app.world_mut()
        .entity_mut(toolbar)
        .insert(Visibility::Visible);
    (app, root)
}

fn named(world: &mut World, parent: Entity, name: &str) -> Entity {
    let matches: Vec<_> = world
        .query::<(Entity, &AccessibilityNode)>()
        .iter(world)
        .filter(|(entity, node)| {
            if node.label() != Some(name) {
                return false;
            }
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if entity == parent {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(matches.len(), 1, "Expected one {name:?}: {matches:?}");
    matches[0]
}

fn glyphs(world: &World, parent: Entity, expected: &str) -> bool {
    world.get::<Children>(parent).is_some_and(|children| {
        children.iter().any(|entity| {
            world
                .get::<Text>(entity)
                .is_some_and(|text| text.0 == expected)
                && world
                    .get::<ComputedNode>(entity)
                    .is_some_and(|node| node.size().min_element() > 0.0)
                && world
                    .get::<bevy::text::TextLayoutInfo>(entity)
                    .is_some_and(|layout| {
                        layout.glyphs.iter().any(|glyph| {
                            world
                                .resource::<Assets<Image>>()
                                .get(glyph.atlas_info.texture)
                                .is_some_and(|image| {
                                    let rect = glyph.atlas_info.rect;
                                    (rect.min.y as u32..rect.max.y as u32).any(|y| {
                                        (rect.min.x as u32..rect.max.x as u32).any(|x| {
                                            image
                                                .get_color_at(x, y)
                                                .is_ok_and(|color| !color.is_fully_transparent())
                                        })
                                    })
                                })
                        })
                    })
                && world.get::<TextColor>(entity).is_some_and(|color| {
                    !color.0.is_fully_transparent()
                        && world
                            .get::<BackgroundColor>(parent)
                            .is_none_or(|background| background.0 != color.0)
                })
        })
    })
}

fn settle(app: &mut App) {
    let mut previous = Vec::new();
    let mut stable = 0;
    for _ in 0..16 {
        app.update();
        let next: Vec<_> = app
            .world_mut()
            .query::<(
                Entity,
                &ComputedNode,
                &UiGlobalTransform,
                Option<&bevy::text::TextLayoutInfo>,
            )>()
            .iter(app.world())
            .map(|(entity, node, transform, text)| {
                (
                    entity,
                    node.size(),
                    transform.translation,
                    text.map_or(0, |text| text.glyphs.len()),
                )
            })
            .collect();
        stable = if next == previous { stable + 1 } else { 0 };
        if stable >= 2 && next.iter().any(|(_, _, _, glyphs)| *glyphs > 0) {
            return;
        }
        previous = next;
    }
    panic!("Native UI layout did not settle within 16 frames");
}

fn click(app: &mut App, root: Entity, name: &str, condition: UiCondition) {
    use super::driver::{Limits, Runner, Selector, Step, UiAction, UiBackend};
    let mut runner = Runner::new(Limits {
        timeout: std::time::Duration::from_secs(2),
        max_polls: 16,
    });
    let step = Step::action(
        name,
        Selector::new(name).with_role("Button"),
        UiAction::Click,
        condition,
    );
    let report = runner.run(&mut UiBackend::new(app, root), &step);
    assert!(report.is_success(), "{report:?}");
}

#[cfg_attr(test, test)]
fn protein_area_filter_visible_and_selectable() {
    let (mut app, root) = fixture(UVec2::new(1000, 800), 1.0);
    EditAction::Open.apply(app.world_mut(), root);
    let mut area = crate::native::area::InfluenceArea::new(
        crate::native::area::AreaShape::Square,
        DVec2::ZERO,
        DVec2::splat(320.0),
    );
    area.protein = Some(crate::native::protein_area::Config::default());
    let area = crate::native::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    crate::native::protein_area::ConfigureProtein.apply(app.world_mut(), area);
    EditAction::Close.apply(app.world_mut(), root);
    let editor = app
        .world_mut()
        .query::<(Entity, &crate::native::protein_area::QueryEditor)>()
        .iter(app.world())
        .find(|(_, link)| link.0 == area)
        .unwrap()
        .0;
    {
        let mut node = app.world_mut().get_mut::<Node>(editor).unwrap();
        node.width = px(720);
        node.height = px(680);
    }
    settle(&mut app);
    click(
        &mut app,
        editor,
        "Add a condition",
        UiCondition::Visible(Selector::new("Condition").with_role("Button")),
    );
    settle(&mut app);
    let toggle = named(app.world_mut(), editor, "Condition");
    assert!(
        glyphs(app.world(), toggle, "Text contains"),
        "The closed Protein condition caption has no visible glyphs"
    );
    click(
        &mut app,
        editor,
        "Condition",
        UiCondition::Visible(Selector::new("Kind").with_role("Button")),
    );
    settle(&mut app);
    let menu = app
        .world()
        .get::<crate::native::dropdown::Dropdown>(toggle)
        .unwrap()
        .menu;
    let choice = named(app.world_mut(), menu, "Kind");
    assert!(
        glyphs(app.world(), choice, "Kind"),
        "The Protein filter choice has no visible glyphs"
    );
    click(
        &mut app,
        editor,
        "Kind",
        UiCondition::Absent(Selector::new("Kind").with_role("Button")),
    );
    settle(&mut app);
    let toggle = named(app.world_mut(), editor, "Condition");
    assert!(glyphs(app.world(), toggle, "Kind"));
    let query = &app
        .world()
        .get::<crate::native::protein_castle::ProteinCastle>(editor)
        .unwrap()
        .draft
        .query;
    assert!(
        query["where"][0]["all"][0].get("kind_eq").is_some(),
        "{query}"
    );
}

#[cfg_attr(test, test)]
fn edit_panel_usable_bounds() {
    use crate::host::{OutputId, OutputViewport, Rectangle};
    for (size, scale) in [(UVec2::new(360, 640), 1.0), (UVec2::new(1000, 1200), 1.25)] {
        let (mut app, root) = fixture(size, scale);
        app.world_mut()
            .entity_mut(root)
            .insert(crate::native::layout::desktop::HostViewport(
                OutputViewport {
                    id: OutputId(1),
                    bounds: Rectangle::new([0.0; 2], size.as_dvec2().to_array()).unwrap(),
                    usable: Some(
                        Rectangle::new([0.0, 40.0], [f64::from(size.x), f64::from(size.y) - 40.0])
                            .unwrap(),
                    ),
                    physical_size: (size.as_vec2() * scale).as_uvec2().to_array(),
                    raster: crate::host::raster::RasterMapping {
                        origin: [0.0; 2],
                        scale: [f64::from(scale); 2],
                    },
                    workspace: 1,
                    rotation: Default::default(),
                },
            ));
        EditAction::Open.apply(app.world_mut(), root);
        settle(&mut app);
        let mode = app
            .world()
            .get::<crate::native::edit_mode::EditMode>(root)
            .unwrap();
        let panel = crate::native::inspection::bounds(app.world(), mode.panel).unwrap();
        let toggle = crate::native::inspection::bounds(app.world(), mode.toggle).unwrap();
        assert!(
            panel.min.x.abs() < 0.1 && (panel.min.y - 40.0).abs() < 0.1,
            "Panel inset: {panel:?}"
        );
        assert!(
            (panel.max.y - (toggle.min.y - 8.0)).abs() < 0.1,
            "Panel does not fill usable height: {panel:?}, controls {toggle:?}"
        );
        assert!(
            (panel.width() - 420.0_f32.min(size.x as f32)).abs() < 0.1,
            "Panel width: {panel:?}"
        );
        assert!(panel.max.x <= size.x as f32 + 0.1 && panel.max.y <= toggle.min.y);
    }
}

#[cfg_attr(test, test)]
fn shared_dropdown_captions_follow_available_width_and_theme() {
    let (mut app, root) = fixture(UVec2::new(1000, 800), 1.0);
    let panel = app
        .world_mut()
        .spawn((
            Node {
                width: px(120),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let toggle = crate::native::dropdown::spawn(
        app.world_mut(),
        panel,
        root,
        "Shared selector",
        "First option",
        vec![
            ("First option".into(), Default::default()),
            ("Second option".into(), Default::default()),
        ],
    );
    settle(&mut app);
    for (scheme, width) in [
        (crate::native::tokens::ColorScheme::Dark, 120.0),
        (crate::native::tokens::ColorScheme::Light, 300.0),
    ] {
        app.world_mut()
            .resource_mut::<crate::native::tokens::ThemeSettings>()
            .scheme = scheme;
        app.world_mut().get_mut::<Node>(panel).unwrap().width = px(width);
        settle(&mut app);
        assert!(glyphs(app.world(), toggle, "First option"));
        click(
            &mut app,
            panel,
            "Shared selector",
            UiCondition::Visible(Selector::new("Second option").with_role("Button")),
        );
        settle(&mut app);
        let menu = app
            .world()
            .get::<crate::native::dropdown::Dropdown>(toggle)
            .unwrap()
            .menu;
        for name in ["First option", "Second option"] {
            let choice = named(app.world_mut(), menu, name);
            assert!(
                glyphs(app.world(), choice, name),
                "Missing {name} at {width} in {scheme:?}"
            );
        }
        click(
            &mut app,
            panel,
            "Second option",
            UiCondition::Absent(Selector::new("Second option").with_role("Button")),
        );
        settle(&mut app);
    }
    let caption = app.world().get::<Children>(toggle).unwrap()[0];
    app.world_mut().get_mut::<Node>(caption).unwrap().width = px(0);
    app.world_mut().get_mut::<Node>(caption).unwrap().flex_grow = 0.0;
    settle(&mut app);
    assert!(!glyphs(app.world(), toggle, "First option"));
}

crate::native::laboratory_cases! {
    protein_area_filter_visible_and_selectable,
    shared_dropdown_captions_follow_available_width_and_theme,
    edit_panel_usable_bounds,
}
