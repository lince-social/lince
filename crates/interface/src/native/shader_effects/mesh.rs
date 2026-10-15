use super::*;
use model::spatial::{
    DEFORM_SPHERE, Deformation, GENERATED_TRIANGLE, GeneratedGeometry, MESH_SHADE, MeshSettings,
    Mode, Quality, Shape, VOLUME_CLOUD, VolumeSettings,
};

#[derive(Clone)]
enum Control {
    Surface,
    SurfacePreset,
    Mesh,
    Shape,
    Add,
    Remove,
    Limit,
    Color,
    Rotate,
    Generate,
    RemoveGeometry,
    Vertices,
    Volume,
    Cloud,
    Quality,
    Density,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<Editor>(owner).is_none() {
            return;
        }
        capture(world, owner);
        let Some(editor) = world.get::<Editor>(owner) else {
            return;
        };
        if editor.pending.is_some() {
            return;
        }
        if matches!(self, Self::Rotate) {
            shader::spatial::rotate(world, editor.preview);
            return;
        }
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        match self {
            Self::Surface => editor.effect.mode = Mode::Surface,
            Self::SurfacePreset => {
                let defaults = Effect::default();
                editor.effect.mode = Mode::Surface;
                editor.effect.programs = defaults.programs;
                editor.effect.rules = defaults.rules;
                editor.effect.fallback = defaults.fallback;
                editor.selected = editor.effect.fallback.clone();
            }
            Self::Volume | Self::Cloud => {
                if !matches!(editor.effect.mode, Mode::Volume(_)) {
                    editor.effect.mode = Mode::Volume(VolumeSettings::default());
                }
                if matches!(self, Self::Cloud) {
                    for source in editor.effect.programs.values_mut() {
                        *source = VOLUME_CLOUD.into();
                    }
                }
            }
            Self::Quality | Self::Density => {
                if let Mode::Volume(settings) = &mut editor.effect.mode {
                    if matches!(self, Self::Quality) {
                        settings.quality = match settings.quality {
                            Quality::Low => Quality::Medium,
                            Quality::Medium => Quality::High,
                            Quality::High => Quality::Low,
                        };
                    } else {
                        settings.density = [0.0, 0.5, 1.0, 2.0, 4.0, 8.0]
                            .into_iter()
                            .find(|density| *density > settings.density)
                            .unwrap_or(0.0);
                    }
                }
            }
            Self::Mesh | Self::Color => {
                if !matches!(editor.effect.mode, Mode::Mesh(_)) {
                    editor.effect.mode = Mode::Mesh(MeshSettings::default());
                }
                if matches!(self, Self::Color) {
                    for source in editor.effect.programs.values_mut() {
                        *source = MESH_SHADE.into();
                    }
                }
            }
            _ => {
                if let Mode::Mesh(settings) = &mut editor.effect.mode {
                    match self {
                        Self::Generate => {
                            settings.geometry = Some(GeneratedGeometry {
                                source: GENERATED_TRIANGLE.into(),
                                vertices: 6,
                            })
                        }
                        Self::RemoveGeometry => settings.geometry = None,
                        Self::Vertices => {
                            if let Some(stage) = &mut settings.geometry {
                                stage.vertices = [6, 96, 768, 3072, 4095]
                                    .into_iter()
                                    .find(|vertices| *vertices > stage.vertices)
                                    .unwrap_or(6);
                            }
                        }
                        Self::Shape => {
                            settings.shape = match settings.shape {
                                Shape::Sphere => Shape::Plane,
                                Shape::Plane => Shape::Cuboid,
                                Shape::Cuboid => Shape::Sphere,
                            }
                        }
                        Self::Add => {
                            settings.deformation = Some(Deformation {
                                source: DEFORM_SPHERE.into(),
                                max_displacement: 0.25,
                            })
                        }
                        Self::Remove => settings.deformation = None,
                        Self::Limit => {
                            if let Some(stage) = &mut settings.deformation {
                                stage.max_displacement = [0.1, 0.25, 0.5]
                                    .into_iter()
                                    .find(|value| *value > stage.max_displacement)
                                    .unwrap_or(0.1);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        editor.rebuild = true;
        render_editor(world, owner);
    }
}

pub(super) fn controls(world: &mut World, owner: Entity, panel: Entity, effect: &Effect) {
    if !matches!(
        world.get::<Editor>(owner).unwrap().target,
        Target::Record(_)
    ) {
        return;
    }
    crate::native::edit_mode::label(
        world,
        panel,
        match &effect.mode {
            Mode::Surface => "Authoring mode: surface",
            Mode::Mesh(_) => "Authoring mode: mesh",
            Mode::Volume(_) => "Authoring mode: volume",
        },
        16.0,
    );
    for (caption, action) in [
        ("Use surface mode", Control::Surface),
        ("Use mesh mode", Control::Mesh),
        ("Use volume mode", Control::Volume),
        ("Surface energy bundle preset", Control::SurfacePreset),
        ("Mesh normal-color bundle preset", Control::Color),
        ("Volume cloud bundle preset", Control::Cloud),
    ] {
        crate::native::sand_panel::button(world, panel, owner, caption, action);
    }
    if let Mode::Volume(settings) = &effect.mode {
        crate::native::edit_mode::label(
            world,
            panel,
            "volume_sample supplies color and density in a local unit cube. The renderer owns bounded raymarch steps, opacity, camera rays and scene-depth occlusion. Mode changes retain programs and bindings. The cloud bundle preset replaces all programs with compatible fields. Invalid edits preserve the working effect.",
            12.0,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!(
                "Volume quality: {:?} · {} steps (change)",
                settings.quality,
                settings.quality.steps()
            ),
            Control::Quality,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Volume density: {} (change)", settings.density),
            Control::Density,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Rotate 3D preview",
            Control::Rotate,
        );
        return;
    }
    let Mode::Mesh(settings) = &effect.mode else {
        return;
    };
    crate::native::edit_mode::label(
        world,
        panel,
        "MeshPoint provides local_position, world_position, normal and uv. Mesh mode also accepts UV shade programs. Mode changes retain your programs; use a matching preset or callback. The working effect stays visible until the complete replacement is ready.",
        12.0,
    );
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        &format!("Mesh shape: {:?} (change)", settings.shape),
        Control::Shape,
    );
    crate::native::sand_panel::button(world, panel, owner, "Rotate 3D preview", Control::Rotate);
    if let Some(stage) = &settings.geometry {
        crate::native::edit_mode::label(
            world,
            panel,
            "Generated triangles replace the selected shape. Positions stay within the local unit cube; vertex deformation may expand that bound by its displacement limit.",
            12.0,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Use built-in shape geometry",
            Control::RemoveGeometry,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Generated vertices: {} (change)", stage.vertices),
            Control::Vertices,
        );
        input(
            world,
            panel,
            owner,
            InputKey::GeometrySource,
            "WGSL: fn generate_vertex(index, vertex_count, inputs) → MeshVertex",
            &stage.source,
            true,
        );
    } else {
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Generate triangle geometry with WGSL",
            Control::Generate,
        );
    }
    if let Some(stage) = &settings.deformation {
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Remove vertex deformation",
            Control::Remove,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!("Displacement limit: {} (change)", stage.max_displacement),
            Control::Limit,
        );
        input(
            world,
            panel,
            owner,
            InputKey::DeformationSource,
            "WGSL: fn deform_vertex(vertex, inputs) → MeshVertex",
            &stage.source,
            true,
        );
    } else {
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Add vertex deformation",
            Control::Add,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_controls_preserve_inputs_and_invalid_stage_drafts_and_ignore_stale_actions() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        world.init_resource::<Feeds>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let mut effect = Effect::default();
        effect.inputs.progress = Some(model::QuantityRange {
            start: "-2".into(),
            end: "0".into(),
        });
        effect.bindings.push(model::bindings::NumericBinding {
            slot: 0,
            label: "Quantity".into(),
            source: default(),
        });
        let owner = open(
            &mut world,
            root,
            Target::Record(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: nucleus::new_uid("r"),
                source: Source::Local,
            }),
            effect.clone(),
            1,
            Some("-1".into()),
        );
        world
            .entity_mut(owner)
            .insert(crate::native::workspace::WorkspaceMember(1));
        for control in [
            Control::Mesh,
            Control::Add,
            Control::Limit,
            Control::Shape,
            Control::Color,
            Control::Generate,
            Control::Vertices,
        ] {
            control.apply(&mut world, owner);
        }
        Command::Preset(model::REWARD.into()).apply(&mut world, owner);
        let edited = &world.get::<Editor>(owner).unwrap().effect;
        assert_eq!(edited.inputs, effect.inputs);
        assert_eq!(edited.bindings, effect.bindings);
        let Mode::Mesh(settings) = &edited.mode else {
            panic!("Mesh mode was reset by a color preset");
        };
        assert_eq!(settings.shape, Shape::Plane);
        assert_eq!(settings.deformation.as_ref().unwrap().max_displacement, 0.5);
        assert_eq!(settings.geometry.as_ref().unwrap().vertices, 96);
        let source = world
            .query::<(Entity, &Input)>()
            .iter(&world)
            .find(|(_, input)| {
                input.editor == owner && matches!(input.key, InputKey::DeformationSource)
            })
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(source)
            .unwrap()
            .editor
            .set_text("fn deform_vertex(");
        capture(&mut world, owner);
        let geometry = world
            .query::<(Entity, &Input)>()
            .iter(&world)
            .find(|(_, input)| {
                input.editor == owner && matches!(input.key, InputKey::GeometrySource)
            })
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(geometry)
            .unwrap()
            .editor
            .set_text("fn generate_vertex(");
        capture(&mut world, owner);
        assert!(
            world
                .get::<Editor>(owner)
                .unwrap()
                .effect
                .validate()
                .is_err()
        );
        let saved = capture_editor(&mut world, owner).unwrap();
        let saved: SavedEditor =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert!(saved.valid());
        let Mode::Mesh(settings) = &saved.effect.mode else {
            panic!("Mesh draft lost its mode");
        };
        assert_eq!(
            settings.deformation.as_ref().unwrap().source,
            "fn deform_vertex("
        );
        assert_eq!(
            settings.geometry.as_ref().unwrap().source,
            "fn generate_vertex("
        );
        for control in [
            Control::Volume,
            Control::Cloud,
            Control::Quality,
            Control::Density,
        ] {
            control.apply(&mut world, owner);
        }
        let volume = &world.get::<Editor>(owner).unwrap().effect;
        assert_eq!(volume.inputs, effect.inputs);
        assert_eq!(volume.bindings, effect.bindings);
        assert!(volume.validate().is_ok());
        let Mode::Volume(settings) = &volume.mode else {
            panic!("Volume mode was not selected");
        };
        assert_eq!(settings.quality, Quality::High);
        assert_eq!(settings.density, 8.0);
        Command::AddProgram.apply(&mut world, owner);
        assert!(
            world
                .get::<Editor>(owner)
                .unwrap()
                .effect
                .validate()
                .is_ok()
        );
        let saved = capture_editor(&mut world, owner).unwrap();
        let saved: SavedEditor =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert_eq!(
            saved.effect.mode,
            Mode::Volume(VolumeSettings {
                quality: Quality::High,
                density: 8.0
            })
        );
        let restored_effect = saved.effect.clone();
        world.despawn(owner);
        Control::Shape.apply(&mut world, owner);
        saved.restore(&mut world, root);
        let restored = world.query::<&Editor>().single(&world).unwrap();
        assert_eq!(restored.effect, restored_effect);
        assert_eq!(restored.effect.inputs, effect.inputs);
        assert_eq!(restored.effect.bindings, effect.bindings);
        let restored_owner = world.query::<(Entity, &Editor)>().single(&world).unwrap().0;
        for control in [Control::Color, Control::SurfacePreset] {
            control.apply(&mut world, restored_owner);
            let edited = &world.get::<Editor>(restored_owner).unwrap().effect;
            assert!(edited.validate().is_ok());
            assert_eq!(edited.inputs, effect.inputs);
            assert_eq!(edited.bindings, effect.bindings);
        }
    }
}
