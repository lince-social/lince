use super::*;

#[derive(Component)]
pub(super) struct EffectInput(pub Entity);

pub(super) fn select_effect(world: &mut World, owner: Entity, id: Option<String>) {
    if let Some(binding) = binding(world, owner) {
        let Some(config) = crate::native::protein_area::configuration(world, binding.area) else {
            return;
        };
        crate::native::record_presentation::remember(world, binding.area);
        let mut settings = effective(world, &binding, &config.shader_placement);
        settings.effect_id = id;
        if settings.valid() {
            world
                .get_mut::<crate::native::area::InfluenceArea>(binding.area)
                .unwrap()
                .records
                .entry(binding.uid)
                .or_default()
                .shader_placement = Some(settings);
        }
    } else {
        change(world, owner, |settings| settings.effect_id = id);
    }
}

fn binding(world: &World, owner: Entity) -> Option<RecordBinding> {
    let Target::Record(binding) = &world.get::<Editor>(owner)?.target else {
        return None;
    };
    world.get::<crate::native::area::InfluenceArea>(binding.area)?;
    Some(binding.clone())
}

pub(super) fn effective(
    world: &World,
    binding: &RecordBinding,
    defaults: &FloatingSettings,
) -> FloatingSettings {
    world
        .get::<crate::native::area::InfluenceArea>(binding.area)
        .and_then(|area| area.records.get(&binding.uid))
        .and_then(|saved| saved.shader_placement.clone())
        .unwrap_or_else(|| defaults.clone())
}

pub(super) fn change(world: &mut World, owner: Entity, mutate: impl FnOnce(&mut FloatingSettings)) {
    if let Some(binding) = binding(world, owner) {
        let Some(old) = world
            .get::<crate::native::area::InfluenceArea>(binding.area)
            .and_then(|area| area.records.get(&binding.uid))
            .and_then(|saved| saved.shader_placement.clone())
        else {
            return;
        };
        let mut settings = old.clone();
        mutate(&mut settings);
        if settings != old && settings.valid() {
            world
                .get_mut::<crate::native::area::InfluenceArea>(binding.area)
                .unwrap()
                .records
                .entry(binding.uid)
                .or_default()
                .shader_placement = Some(settings);
        }
    } else if let Some(mut config) = crate::native::protein_area::configuration(world, owner) {
        let old = config.shader_placement.clone();
        mutate(&mut config.shader_placement);
        if config.shader_placement != old && config.shader_placement.valid() {
            crate::native::protein_area::set_configuration(world, owner, Some(config));
        }
    }
}

pub(super) fn refresh(world: &mut World, owner: Entity) {
    if let Some(mut editor) = world.get_mut::<Editor>(owner) {
        editor.rebuild = true;
    }
}

#[derive(Clone)]
enum Control {
    Override,
    Reset,
    Frozen,
    Distance,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        match self {
            Self::Frozen => change(world, owner, |settings| settings.frozen = !settings.frozen),
            Self::Distance => change(world, owner, |settings| {
                settings.distance = match settings.distance {
                    DistancePolicy::Thumbnail => DistancePolicy::Hide,
                    DistancePolicy::Hide => DistancePolicy::Live,
                    DistancePolicy::Live => DistancePolicy::Thumbnail,
                }
            }),
            Self::Override | Self::Reset => {
                let Some(binding) = binding(world, owner) else {
                    return;
                };
                let Some(config) = crate::native::protein_area::configuration(world, binding.area)
                else {
                    return;
                };
                crate::native::record_presentation::remember(world, binding.area);
                world
                    .get_mut::<crate::native::area::InfluenceArea>(binding.area)
                    .unwrap()
                    .records
                    .entry(binding.uid)
                    .or_default()
                    .shader_placement = if matches!(self, Self::Override) {
                    Some(config.shader_placement)
                } else {
                    None
                };
                world.get_mut::<Editor>(owner).unwrap().rebuild = true;
            }
        }
        refresh(world, owner);
    }
}

pub(super) fn editor_controls(world: &mut World, owner: Entity, panel: Entity) {
    let Some(binding) = binding(world, owner) else {
        return;
    };
    let Some(config) = crate::native::protein_area::configuration(world, binding.area) else {
        return;
    };
    let overridden = world
        .get::<crate::native::area::InfluenceArea>(binding.area)
        .and_then(|area| area.records.get(&binding.uid))
        .is_some_and(|saved| saved.shader_placement.is_some());
    crate::native::edit_mode::label(world, panel, "Appearance in this Castle", 16.0);
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        if overridden {
            "Reset to Castle defaults"
        } else {
            "Override this Record's placement"
        },
        if overridden {
            Control::Reset
        } else {
            Control::Override
        },
    );
    if overridden {
        let settings = effective(world, &binding, &config.shader_placement);
        placement_controls(world, owner, panel, &settings);
    }
}

pub(super) fn extra_controls(
    world: &mut World,
    owner: Entity,
    panel: Entity,
    settings: &FloatingSettings,
) {
    crate::native::edit_mode::label(world, panel, "Effect ID (empty uses Record default)", 13.0);
    let entity = world
        .spawn(crate::native::sand::text_editor(
            settings.effect_id.as_deref().unwrap_or(""),
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .insert((
            Node {
                width: percent(100),
                height: px(32),
                ..default()
            },
            ChildOf(panel),
            EffectInput(owner),
        ))
        .id();
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .max_characters = Some(64);
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .allow_newlines = false;
    crate::native::accessibility::input(
        world,
        entity,
        "Effect ID (empty uses Record default)",
        false,
    );
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        if settings.frozen {
            "Snapshot: frozen (resume)"
        } else {
            "Snapshot: live (freeze)"
        },
        Control::Frozen,
    );
    let caption = match settings.distance {
        DistancePolicy::Thumbnail => "At distance: frozen thumbnail",
        DistancePolicy::Hide => "At distance: hide effect",
        DistancePolicy::Live => "At distance: keep live",
    };
    crate::native::sand_panel::button(world, panel, owner, caption, Control::Distance);
}

pub(super) fn distant(
    world: &mut World,
    owner: Entity,
    record: Entity,
    settings: &FloatingSettings,
) -> bool {
    if settings.distance == DistancePolicy::Live {
        return false;
    }
    let Some(root) = root(world, record) else {
        return false;
    };
    let mut pixels = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(settings.size, |view| settings.size * view.zoom as f32);
    if world
        .get::<crate::native::topology::view::View>(root)
        .is_some_and(|view| view.spatial)
        && let Some(position) = crate::native::topology::position(world, owner)
    {
        let point =
            (position - crate::native::topology::presentation::origin(world, root)).as_vec3();
        if let Some((camera, transform)) = world.query_filtered::<(&Camera, &GlobalTransform), With<crate::native::topology::presentation::WorldCamera>>().iter(world).next() {
            let offset = transform.up().as_vec3() * settings.size * 0.5;
            if let (Ok(top), Ok(bottom)) = (camera.world_to_viewport(transform, point + offset), camera.world_to_viewport(transform, point - offset)) { pixels = top.distance(bottom); }
        }
    }
    let was_distant = world
        .get::<Floating>(owner)
        .is_some_and(|floating| floating.distant);
    pixels < settings.min_pixels * if was_distant { 1.15 } else { 0.85 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> (World, Entity, Entity, Entity, RecordBinding) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<ShaderSettings>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
                crate::native::canvas::CanvasView::default(),
            ))
            .id();
        let mut area = crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            DVec2::ZERO,
            DVec2::splat(1000.0),
        );
        let config = Config {
            shader_effects: true,
            ..Config::records()
        };
        area.protein = Some(config.clone());
        let area = world.spawn((area, ChildOf(root))).id();
        let binding = RecordBinding {
            area,
            uid: nucleus::new_uid("r"),
            source: Source::Local,
        };
        let row = world
            .spawn((
                Node::default(),
                ChildOf(root),
                crate::native::canvas::CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::new(320.0, 200.0),
                },
                binding.clone(),
                crate::native::record_presentation::RecordPresentation {
                    hide_filled: true,
                    expanded: false,
                },
            ))
            .id();
        record(
            &mut world,
            row,
            &config,
            &serde_json::json!({"quantity":"0", "shader_effect": Extension::single(1, Effect::default())}),
            Some(binding.clone()),
        );
        let editor = open(
            &mut world,
            root,
            Target::Record(binding.clone()),
            Effect::default(),
            1,
            Some("0".into()),
        );
        (world, root, row, editor, binding)
    }

    #[test]
    fn overrides_survive_capture_and_reload_and_reset_rejects_stale_controls() {
        let (mut world, _, row, editor, binding) = scene();
        let defaults = FloatingSettings::default();
        assert_eq!(effective(&world, &binding, &defaults), defaults);
        Control::Override.apply(&mut world, editor);
        change(&mut world, editor, |settings| {
            settings.size = 180.0;
            settings.gap = 0.0;
            settings.elevation = 90.0;
            settings.billboard = true;
            settings.frozen = true;
        });
        select_effect(&mut world, editor, Some("main".into()));
        update_presentations(&mut world);
        let floating = world
            .query_filtered::<Entity, With<Floating>>()
            .single(&world)
            .unwrap();
        assert_eq!(world.get::<Node>(floating).unwrap().width, px(180));
        let settings = effective(&world, &binding, &defaults);
        assert_eq!(settings.effect_id.as_deref(), Some("main"));
        crate::native::record_presentation::remember(&mut world, binding.area);
        let area = world
            .get::<crate::native::area::InfluenceArea>(binding.area)
            .unwrap();
        assert!(area.records[&binding.uid].presentation.hide_filled);
        let restored: crate::native::area::InfluenceArea =
            serde_json::from_value(serde_json::to_value(area).unwrap()).unwrap();
        assert_eq!(
            restored.records[&binding.uid].shader_placement.as_ref(),
            Some(&settings)
        );
        world.entity_mut(binding.area).insert(restored);
        assert_eq!(effective(&world, &binding, &defaults), settings);
        Control::Reset.apply(&mut world, editor);
        change(&mut world, editor, |settings| settings.size = 180.0);
        assert_eq!(effective(&world, &binding, &defaults), defaults);
        update_presentations(&mut world);
        assert_eq!(world.get::<Node>(floating).unwrap().width, px(120));
        assert_eq!(world.query::<&Floating>().iter(&world).count(), 1);
        assert_eq!(world.query::<&EffectSlot>().iter(&world).count(), 1);
        assert_eq!(
            world
                .get::<crate::native::canvas::CanvasItem>(row)
                .unwrap()
                .position,
            DVec2::ZERO
        );
    }

    #[test]
    fn distance_hysteresis_freezes_or_hides_without_hiding_record_titles() {
        let (mut world, root, row, editor, binding) = scene();
        Control::Override.apply(&mut world, editor);
        let floating = world
            .query_filtered::<Entity, With<Floating>>()
            .single(&world)
            .unwrap();
        let preview = world.get::<Floating>(floating).unwrap().preview;
        world
            .get_mut::<crate::native::canvas::CanvasView>(root)
            .unwrap()
            .zoom = 0.1;
        update_presentations(&mut world);
        assert!(world.get::<shader::EffectPreview>(preview).unwrap().frozen);
        assert!(
            !world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        world
            .get_mut::<crate::native::canvas::CanvasView>(root)
            .unwrap()
            .zoom = 0.27;
        update_presentations(&mut world);
        assert!(world.get::<shader::EffectPreview>(preview).unwrap().frozen);
        change(&mut world, editor, |settings| {
            settings.distance = DistancePolicy::Hide
        });
        update_presentations(&mut world);
        assert!(
            world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        assert_eq!(world.get::<Node>(row).unwrap().display, Display::Flex);
        world
            .get_mut::<crate::native::canvas::CanvasView>(root)
            .unwrap()
            .zoom = 1.0;
        update_presentations(&mut world);
        assert!(
            !world
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .disabled
        );
        assert!(!world.get::<shader::EffectPreview>(preview).unwrap().frozen);
        change(&mut world, editor, |settings| {
            settings.min_pixels = f32::NAN
        });
        assert!(effective(&world, &binding, &FloatingSettings::default()).valid());
    }
}
