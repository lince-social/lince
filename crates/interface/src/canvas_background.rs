use crate::{canvas::CanvasView, container::BoxRoot, workspace::Workspaces};
use bevy::{
    asset::embedded_asset, math::DVec2, prelude::*, render::render_resource::AsBindGroup,
    shader::ShaderRef, ui::UiSystems,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasColors {
    pub background: [u8; 3],
    pub grid: [u8; 3],
}

impl Default for CanvasColors {
    fn default() -> Self {
        Self {
            background: [18, 18, 20],
            grid: [44, 44, 49],
        }
    }
}

pub fn color(rgb: [u8; 3]) -> Color {
    Color::srgb_u8(rgb[0], rgb[1], rgb[2])
}

pub(crate) fn current(world: &World, root: Entity) -> CanvasColors {
    use crate::tokens::{ThemeSettings, Token, TokenOverrides, TokenValue};
    let defaults = ThemeSettings::default();
    let settings = world.get_resource::<ThemeSettings>().unwrap_or(&defaults);
    let rgb = |token| match settings.resolve(token, None, &TokenOverrides::default()).0 {
        TokenValue::Color([r, g, b, _]) => [r, g, b],
        _ => unreachable!(),
    };
    let mut colors = CanvasColors {
        background: rgb(Token::CanvasBackground),
        grid: rgb(Token::CanvasGrid),
    };
    if let Some(space) = world.get::<Workspaces>(root).and_then(|spaces| {
        spaces
            .entries
            .iter()
            .find(|space| space.id == spaces.active)
    }) {
        if space.color_overrides[0] {
            colors.background = space.colors.background;
        }
        if space.color_overrides[1] {
            colors.grid = space.colors.grid;
        }
    }
    colors
}

#[derive(Clone, Copy, PartialEq)]
struct Drawing {
    center: DVec2,
    zoom: f64,
    viewport: Vec2,
    colors: CanvasColors,
    pattern: f32,
    spacing: f32,
    thickness: f32,
}

#[derive(Component)]
struct Grid {
    material: Handle<PatternMaterial>,
    drawing: Option<Drawing>,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct PatternMaterial {
    #[uniform(0)]
    ink: Vec4,
    #[uniform(1)]
    geometry: Vec4,
    #[uniform(2)]
    viewport: Vec4,
}

impl UiMaterial for PatternMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://lince_interface/canvas_pattern.wgsl".into()
    }
}

pub struct CanvasBackgroundPlugin;

impl Plugin for CanvasBackgroundPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<AssetServer>() {
            embedded_asset!(app, "canvas_pattern.wgsl");
            app.add_plugins(UiMaterialPlugin::<PatternMaterial>::default());
        } else {
            app.init_resource::<Assets<PatternMaterial>>();
        }
        app.add_systems(
            PostUpdate,
            draw.after(UiSystems::Propagate).before(UiSystems::Content),
        );
    }
}

fn grid_axis(center: f64, zoom: f64, extent: f32, spacing: f32) -> Option<(f32, f32)> {
    if !center.is_finite()
        || !zoom.is_finite()
        || zoom <= 0.0
        || !extent.is_finite()
        || extent <= 0.0
    {
        return None;
    }
    let spacing = f64::from(spacing);
    let step = spacing * 2_f64.powf((16.0 / (spacing * zoom)).log2().ceil().max(0.0));
    let spacing = step * zoom;
    let start = (f64::from(extent) * 0.5 - center.rem_euclid(step) * zoom).rem_euclid(spacing);
    if !start.is_finite() || !(spacing as f32).is_finite() || spacing <= 0.0 {
        return None;
    }
    Some((
        (start as f32).rem_euclid(spacing as f32) + 0.5,
        spacing as f32,
    ))
}

fn draw(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .iter(world)
        .collect();
    for root in roots {
        let Some(view) = world.get::<CanvasView>(root) else {
            continue;
        };
        let Some(target) = world.get::<ComputedUiRenderTargetInfo>(root) else {
            continue;
        };
        let drawing = Drawing {
            spacing: world
                .get_resource::<crate::tokens::ThemeSettings>()
                .map(|settings| {
                    settings
                        .resolve(crate::tokens::Token::GridSpacing, None, &Default::default())
                        .0
                        .number()
                })
                .unwrap_or(32.0),
            thickness: world
                .get_resource::<crate::tokens::ThemeSettings>()
                .map(|settings| {
                    settings
                        .resolve(
                            crate::tokens::Token::GridThickness,
                            None,
                            &Default::default(),
                        )
                        .0
                        .number()
                })
                .unwrap_or(1.0),
            center: view.center,
            zoom: view.zoom,
            viewport: target.logical_size(),
            colors: current(world, root),
            pattern: world
                .get_resource::<crate::tokens::ThemeSettings>()
                .map(|settings| {
                    settings
                        .resolve(
                            crate::tokens::Token::CanvasPattern,
                            None,
                            &Default::default(),
                        )
                        .0
                        .number()
                })
                .unwrap_or(100.0),
        };
        if world
            .get::<Grid>(root)
            .is_some_and(|grid| grid.drawing == Some(drawing))
        {
            continue;
        }
        let Some((x, spacing)) = grid_axis(
            drawing.center.x,
            drawing.zoom,
            drawing.viewport.x,
            drawing.spacing,
        ) else {
            continue;
        };
        let Some((y, _)) = grid_axis(
            drawing.center.y,
            drawing.zoom,
            drawing.viewport.y,
            drawing.spacing,
        ) else {
            continue;
        };
        let material = PatternMaterial {
            ink: color(drawing.colors.grid).to_linear().to_vec4(),
            geometry: Vec4::new(x, y, spacing, drawing.pattern / 100.0),
            viewport: drawing.viewport.extend(drawing.thickness).extend(0.0),
        };
        if let Some(grid) = world.get::<Grid>(root) {
            let handle = grid.material.clone();
            *world
                .resource_mut::<Assets<PatternMaterial>>()
                .get_mut(&handle)
                .unwrap() = material;
        } else {
            let handle = world
                .resource_mut::<Assets<PatternMaterial>>()
                .add(material);
            world.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ZIndex(-1),
                MaterialNode(handle.clone()),
                Pickable::IGNORE,
                ChildOf(root),
            ));
            world.entity_mut(root).insert(Grid {
                material: handle,
                drawing: None,
            });
        }
        world
            .entity_mut(root)
            .insert(BackgroundColor(color(drawing.colors.background)));
        world.get_mut::<Grid>(root).unwrap().drawing = Some(drawing);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn grid_follows_pan_and_zoom_at_distant_coordinates() {
        assert_eq!(grid_axis(0.0, 1.0, 800.0, 32.0), Some((16.5, 32.0)));
        assert_eq!(grid_axis(-7.0, 1.0, 800.0, 32.0), Some((23.5, 32.0)));
        assert_eq!(grid_axis(-7.0, 2.0, 800.0, 32.0), Some((30.5, 64.0)));
        assert_eq!(
            grid_axis(1e12 - 7.0, 2.0, 800.0, 32.0),
            grid_axis(-7.0, 2.0, 800.0, 32.0)
        );
        for zoom in [0.1, 0.5, 1.0, 3.0] {
            let (start, spacing) = grid_axis(-1e12, zoom, 3840.0, 32.0).unwrap();
            assert!(spacing >= 16.0);
            assert!(start >= 0.5 && start < spacing + 0.5);
        }
        assert!(grid_axis(f64::NAN, 1.0, 800.0, 32.0).is_none());
        assert!(grid_axis(0.0, 0.0, 800.0, 32.0).is_none());
    }

    crate::laboratory_cases! {
        grid_follows_pan_and_zoom_at_distant_coordinates,
    }
}
