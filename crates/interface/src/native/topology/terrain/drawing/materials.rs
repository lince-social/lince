// non-lean-yet
// Scope: struct Materials.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;

#[derive(Default)]
pub(crate) struct Materials {
    pattern: Option<Vec4>,
    colors: HashMap<(Entity, u64), (Vec4, Vec4)>,
}

impl Materials {
    pub(super) fn definition(&mut self, world: &World, key: Key) -> GroundMaterial {
        let pattern = *self.pattern.get_or_insert_with(|| {
            nucleus::performance::add("terrain-ground-style-preparations", 1);
            let settings = world.get_resource::<crate::native::tokens::ThemeSettings>();
            let setting = |token, fallback| {
                settings.map_or(fallback, |settings| {
                    settings
                        .resolve(token, None, &Default::default())
                        .0
                        .number()
                })
            };
            Vec4::new(
                setting(crate::native::tokens::Token::GridSpacing, 32.0).max(1.0),
                setting(crate::native::tokens::Token::GridThickness, 1.0),
                setting(crate::native::tokens::Token::CanvasPattern, 100.0) / 100.0,
                50.0,
            )
        });
        let (background, ink) = *self.colors.entry((key.0, key.1)).or_insert_with(|| {
            nucleus::performance::add("terrain-ground-workspace-styles", 1);
            let colors = crate::native::canvas_background::for_workspace(world, key.0, key.1);
            let rgb = |color: [u8; 3]| {
                let linear = Color::srgb_u8(color[0], color[1], color[2]).to_linear();
                Vec4::new(linear.red, linear.green, linear.blue, linear.alpha)
            };
            (rgb(colors.background), rgb(colors.grid))
        });
        let minimum = key.minimum();
        GroundMaterial {
            background,
            ink,
            pattern,
            phase: Vec4::new(
                minimum.x.rem_euclid(f64::from(pattern.x)) as f32,
                minimum.y.rem_euclid(f64::from(pattern.x)) as f32,
                0.0,
                0.0,
            ),
        }
    }
}
