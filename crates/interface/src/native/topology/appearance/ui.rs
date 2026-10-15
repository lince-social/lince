use super::*;
use crate::native::actions::Action;

#[derive(Component)]
struct Field {
    owner: Entity,
    key: usize,
}

#[derive(Component)]
struct Status(Entity);

#[derive(Component)]
struct ImportPath(Entity);

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Image",
        author: "The image-rs developers",
        license: include_str!("../../../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
];

#[derive(Clone, Copy)]
enum Control {
    EnableMaterial,
    DisableMaterial,
    ResetMaterial,
    ToggleLit,
    Texture,
    Cast,
    Receive,
    EnableLight,
    DisableLight,
    ResetLight,
    Shadows,
    Drive,
    Apply,
    Import,
    Retry,
}

fn editable(world: &World, owner: Entity) -> bool {
    world.get::<ChildOf>(owner).is_some_and(|parent| {
        world
            .get::<crate::native::edit_mode::EditMode>(parent.parent())
            .is_some_and(|mode| mode.enabled)
    }) && world
        .get::<crate::native::canvas::CanvasItem>(owner)
        .is_some()
}

fn material(world: &World, owner: Entity) -> model::Material {
    world
        .get::<Material>(owner)
        .map(|value| value.0.clone())
        .unwrap_or(model::Material {
            lit: true,
            ..default()
        })
}

fn light(world: &World, owner: Entity) -> model::Light {
    world
        .get::<Light>(owner)
        .map(|value| value.0.clone())
        .unwrap_or_default()
}

fn values(material: &model::Material, light: &model::Light) -> Vec<String> {
    vec![
        material.color.map(|value| value.to_string()).join(", "),
        material.repeat.map(|value| value.to_string()).join(", "),
        material.roughness.to_string(),
        material.metallic.to_string(),
        material.emissive.to_string(),
        light.position.map(|value| value.to_string()).join(", "),
        light.color.map(|value| value.to_string()).join(", "),
        light.intensity.to_string(),
        light.range.to_string(),
    ]
}

fn refresh(world: &mut World, owner: Entity) {
    let values = values(&material(world, owner), &light(world, owner));
    let inputs: Vec<_> = world
        .query::<(Entity, &Field)>()
        .iter(world)
        .filter(|(_, field)| field.owner == owner)
        .map(|(entity, field)| (entity, field.key))
        .collect();
    for (entity, key) in inputs {
        if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(entity) {
            text.editor.set_text(&values[key]);
        }
    }
}

fn number_list<const N: usize>(text: &str) -> Result<[f64; N], String> {
    text.split([',', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f64>()
                .map_err(|_| "Enter finite numeric values separated by commas.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| format!("Enter exactly {N} numbers."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_inspector_constructs_all_fields_and_applies_accessible_emission_input() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        let world = app.world_mut();
        world
            .get_mut::<crate::native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        let owner = world
            .spawn((
                crate::native::canvas::CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                ChildOf(root),
            ))
            .id();
        let panel = world.spawn((Node::default(), ChildOf(root))).id();
        controls(world, panel, owner);
        assert_eq!(
            world
                .query::<&Field>()
                .iter(world)
                .filter(|field| field.owner == owner)
                .count(),
            9
        );
        assert_eq!(
            world
                .query::<&ImportPath>()
                .iter(world)
                .filter(|path| path.0 == owner)
                .count(),
            1
        );
        let input = world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| field.owner == owner && field.key == 4)
            .unwrap()
            .0;
        assert_eq!(
            world
                .get::<bevy::a11y::AccessibilityNode>(input)
                .unwrap()
                .label(),
            Some("Emission (0–32)")
        );
        Control::EnableMaterial.apply(world, owner);
        world
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .editor
            .set_text("2");
        Control::Apply.apply(world, owner);
        assert_eq!(world.get::<Material>(owner).unwrap().0.emissive, 2.0);
        assert_eq!(world.get::<Node>(input).unwrap().width, percent(100));
    }

    #[test]
    fn appearance_controls_require_edit_mode_and_apply_fields_atomically() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        let mut world = app.world_mut();
        let owner = world
            .spawn((
                crate::native::canvas::CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                ChildOf(root),
            ))
            .id();
        Control::EnableMaterial.apply(&mut world, owner);
        assert!(world.get::<Material>(owner).is_none());
        world
            .get_mut::<crate::native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        Control::EnableMaterial.apply(&mut world, owner);
        Control::EnableLight.apply(&mut world, owner);
        assert!(world.get::<Material>(owner).unwrap().0.lit);
        assert!(world.get::<Light>(owner).unwrap().0.enabled);
        let editor = crate::native::sand::text_editor(
            "NaN",
            world.resource::<crate::native::theme::Typography>(),
            0,
        );
        let input = world.spawn((editor, Field { owner, key: 8 })).id();
        let material = world.get::<Material>(owner).unwrap().0.clone();
        let light = world.get::<Light>(owner).unwrap().0.clone();
        assert!(apply_fields(&mut world, owner).is_err());
        assert_eq!(world.get::<Material>(owner).unwrap().0, material);
        assert_eq!(world.get::<Light>(owner).unwrap().0, light);
        world
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .editor
            .set_text("2000");
        apply_fields(&mut world, owner).unwrap();
        assert_eq!(world.get::<Light>(owner).unwrap().0.range, 2000.0);
        Control::DisableMaterial.apply(&mut world, owner);
        Control::DisableLight.apply(&mut world, owner);
        assert!(world.get::<Material>(owner).is_none());
        assert!(!world.get::<Light>(owner).unwrap().0.enabled);
        Control::ResetLight.apply(&mut world, owner);
        assert_eq!(
            world.get::<Light>(owner).unwrap().0,
            model::Light::default()
        );
    }
}

fn apply_fields(world: &mut World, owner: Entity) -> Result<(), String> {
    let mut fields = values(&material(world, owner), &light(world, owner));
    for (field, text) in world
        .query::<(&Field, &bevy::text::EditableText)>()
        .iter(world)
    {
        if field.owner == owner {
            fields[field.key] = text.editor.text().to_string();
        }
    }
    let mut material = material(world, owner);
    let mut light = light(world, owner);
    material.color = number_list::<4>(&fields[0])?.map(|value| value as f32);
    material.repeat = number_list::<2>(&fields[1])?.map(|value| value as f32);
    material.roughness = number_list::<1>(&fields[2])?[0] as f32;
    material.metallic = number_list::<1>(&fields[3])?[0] as f32;
    material.emissive = number_list::<1>(&fields[4])?[0] as f32;
    light.position = number_list::<3>(&fields[5])?;
    light.color = number_list::<3>(&fields[6])?.map(|value| value as f32);
    light.intensity = number_list::<1>(&fields[7])?[0] as f32;
    light.range = number_list::<1>(&fields[8])?[0] as f32;
    if !material.valid() || !light.valid() {
        return Err(
            "Settings exceed the limits shown beside the fields; nothing was applied.".into(),
        );
    }
    if world.get::<Material>(owner).is_some() {
        world.entity_mut(owner).insert(Material(material));
    }
    if world.get::<Light>(owner).is_some() {
        world.entity_mut(owner).insert(Light(light));
    }
    Ok(())
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if !editable(world, owner) {
            return;
        }
        let mut material = material(world, owner);
        let mut light = light(world, owner);
        match self {
            Self::Import => {
                let path = world
                    .query::<(&ImportPath, &bevy::text::EditableText)>()
                    .iter(world)
                    .find(|(input, _)| input.0 == owner)
                    .map(|(_, text)| text.editor.text().to_string());
                if let Some(path) = path
                    && let Err(error) =
                        textures::import(world, owner, std::path::PathBuf::from(path.trim()))
                {
                    crate::native::notifications::report(world, "Sand texture", &error);
                }
            }
            Self::Retry => {
                textures::retry(world, &material.texture);
            }
            Self::DisableMaterial => {
                world.entity_mut(owner).remove::<Material>();
            }
            Self::EnableMaterial | Self::ResetMaterial => {
                world.entity_mut(owner).insert(Material(model::Material {
                    lit: true,
                    ..default()
                }));
                refresh(world, owner);
            }
            Self::ToggleLit => {
                material.lit = !material.lit;
                world.entity_mut(owner).insert(Material(material));
            }
            Self::Texture => {
                material.texture = match material.texture {
                    model::Texture::Plain => model::Texture::Grain,
                    model::Texture::Grain => model::Texture::Tiles,
                    model::Texture::Tiles => model::Texture::Brushed,
                    _ => model::Texture::Plain,
                };
                world.entity_mut(owner).insert(Material(material));
            }
            Self::Cast => {
                material.cast_shadows = !material.cast_shadows;
                world.entity_mut(owner).insert(Material(material));
            }
            Self::Receive => {
                material.receive_shadows = !material.receive_shadows;
                world.entity_mut(owner).insert(Material(material));
            }
            Self::EnableLight => {
                light.enabled = true;
                world.entity_mut(owner).insert(Light(light));
            }
            Self::DisableLight => {
                light.enabled = false;
                world.entity_mut(owner).insert(Light(light));
            }
            Self::ResetLight => {
                world
                    .entity_mut(owner)
                    .insert(Light(model::Light::default()));
                refresh(world, owner);
            }
            Self::Shadows => {
                light.shadows = match light.shadows {
                    model::ShadowQuality::Disabled => model::ShadowQuality::Low,
                    model::ShadowQuality::Low => model::ShadowQuality::Medium,
                    model::ShadowQuality::Medium => model::ShadowQuality::High,
                    model::ShadowQuality::High => model::ShadowQuality::Disabled,
                };
                world.entity_mut(owner).insert(Light(light));
            }
            Self::Drive => {
                light.drive = match light.drive {
                    LightDrive::Constant => LightDrive::Pending,
                    LightDrive::Pending => LightDrive::Completed,
                    LightDrive::Completed => LightDrive::Progress,
                    LightDrive::Progress => LightDrive::Constant,
                };
                world.entity_mut(owner).insert(Light(light));
            }
            Self::Apply => {
                if let Err(error) = apply_fields(world, owner) {
                    crate::native::notifications::report(world, "Sand appearance", &error);
                }
            }
        }
    }
}

pub(crate) fn controls(world: &mut World, panel: Entity, owner: Entity) {
    crate::native::edit_mode::label(world, panel, "Material and attached light", 15.0);
    let text = crate::native::edit_mode::label(world, panel, "", 13.0);
    world.entity_mut(text).insert(Status(owner));
    for (name, action) in [
        ("Enable material", Control::EnableMaterial),
        ("Disable material", Control::DisableMaterial),
        ("Reset material", Control::ResetMaterial),
        ("Toggle lit / unlit", Control::ToggleLit),
        ("Next texture preset", Control::Texture),
        ("Toggle cast shadows", Control::Cast),
        ("Toggle receive shadows", Control::Receive),
        ("Enable attached light", Control::EnableLight),
        ("Disable attached light", Control::DisableLight),
        ("Reset attached light", Control::ResetLight),
        ("Next shadow quality", Control::Shadows),
        ("Next light driver", Control::Drive),
    ] {
        crate::native::sand_panel::button(world, panel, owner, name, action);
    }
    let values = values(&material(world, owner), &light(world, owner));
    for (key, label) in [
        "Backing RGBA (each 0–1)",
        "Texture repeat XY (0.25–64)",
        "Roughness (0.089–1)",
        "Metalness (0–1)",
        "Emission (0–32)",
        "Light local XYZ (±100000)",
        "Light RGB (each 0–1)",
        "Light intensity (0–100000 lm)",
        "Light range (1–100000)",
    ]
    .iter()
    .enumerate()
    {
        crate::native::edit_mode::label(world, panel, label, 13.0);
        let editor = crate::native::sand::text_editor(
            &values[key],
            world.resource::<crate::native::theme::Typography>(),
            0,
        );
        let input = world
            .spawn((editor, Field { owner, key }, ChildOf(panel)))
            .id();
        {
            let mut node = world.get_mut::<Node>(input).unwrap();
            node.width = percent(100);
            node.min_height = px(32);
        }
        world
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .max_characters = Some(160);
        crate::native::accessibility::input(world, input, label, false);
    }
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        "Apply material and light fields",
        Control::Apply,
    );
    crate::native::edit_mode::label(
        world,
        panel,
        "Local texture (PNG / JPEG / WebP, ≤8 MiB)",
        13.0,
    );
    let editor = crate::native::sand::text_editor(
        "",
        world.resource::<crate::native::theme::Typography>(),
        0,
    );
    let input = world
        .spawn((editor, ImportPath(owner), ChildOf(panel)))
        .id();
    {
        let mut node = world.get_mut::<Node>(input).unwrap();
        node.width = percent(100);
        node.min_height = px(32);
    }
    world
        .get_mut::<bevy::text::EditableText>(input)
        .unwrap()
        .max_characters = Some(4096);
    crate::native::accessibility::input(world, input, "Local material texture path", false);
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        "Browse texture…",
        crate::native::file_explorer::BrowseFor {
            input,
            extensions: vec!["png".into(), "jpg".into(), "jpeg".into(), "webp".into()],
            directories: false,
        },
    );
    crate::native::sand_panel::button(world, panel, owner, "Import texture", Control::Import);
    crate::native::sand_panel::button(world, panel, owner, "Retry missing texture", Control::Retry);
    crate::native::sand_panel::credits(world, panel, panel, CREDITS);
    crate::native::edit_mode::label(
        world,
        panel,
        "Limits: 64 material surfaces, 8 lights, 2 shadow lights; shadow maps share the highest active quality. Drivers use this Sand's shader task/progress mapping. Text stays unlit.",
        12.0,
    );
}

pub(super) fn update(world: &mut World) {
    let labels: Vec<_> = world
        .query::<(Entity, &Status)>()
        .iter(world)
        .map(|(entity, status)| (entity, status.0))
        .collect();
    for (entity, owner) in labels {
        let material = world.get::<Material>(owner).map(|value| &value.0);
        let light = world.get::<Light>(owner).map(|value| &value.0);
        let mut text = format!(
            "Material: {}\nTexture: {:?}; cast: {}; receive: {}\nLight: {}; shadows: {:?}; driver: {:?}",
            material.map_or("disabled", |value| if value.lit { "lit" } else { "unlit" }),
            material.map(|value| &value.texture),
            material.is_some_and(|value| value.cast_shadows),
            material.is_some_and(|value| value.receive_shadows),
            if light.is_some_and(|value| value.enabled) {
                "enabled"
            } else {
                "disabled"
            },
            light.map(|value| value.shadows),
            light.map(|value| value.drive)
        );
        let status = textures::status(world, material.map(|value| &value.texture));
        if !status.is_empty() {
            text.push('\n');
            text.push_str(&status);
        }
        if let Some(mut current) = world.get_mut::<Text>(entity) {
            if current.0 != text {
                current.0 = text;
            }
        }
    }
}
