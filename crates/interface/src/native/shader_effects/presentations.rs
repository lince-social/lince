// non-lean-yet
// Scope: struct LabelTargets, struct FloatingScratch, struct SurfaceScratch, fn update.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 158, 321, 328,
//   329.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (27, 30, 346,
//   347).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   164.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (146, 150, 285, 382, 408).

use super::*;

#[derive(Default)]
struct LabelTargets {
    current: bool,
    slots: Vec<Entity>,
    labels: Vec<(Entity, f32)>,
}

#[derive(Default)]
struct FloatingScratch {
    owners: Vec<(Entity, Entity, Entity)>,
    targets: HashMap<Entity, LabelTargets>,
}

#[derive(Default)]
struct SurfaceScratch {
    backgrounds: Vec<(Entity, Entity)>,
    owners: Vec<Entity>,
    text_owners: Vec<Entity>,
}

pub(super) fn update(world: &mut World) {
    let _publication = nucleus::performance::Span::new("shader-presentation-publication");
    world
        .run_system_cached(update_floating)
        .expect("publish current floating shader presentation");
    world
        .run_system_cached(update_surfaces)
        .expect("publish current Surface shader presentation");
}

fn update_floating(
    world: &mut World,
    floating: &mut QueryState<(Entity, &'static Floating)>,
    slots: &mut QueryState<(Entity, &'static EffectSlot)>,
    labels: &mut QueryState<(
        Entity,
        &'static RecordStateLabel,
        Option<&'static ComputedNode>,
    )>,
    rotations: &mut QueryState<
        &'static Transform,
        With<crate::native::topology::presentation::WorldCamera>,
    >,
    cameras: &mut QueryState<
        (&'static Camera, &'static GlobalTransform),
        With<crate::native::topology::presentation::WorldCamera>,
    >,
    mut scratch: Local<FloatingScratch>,
) {
    let _publication = nucleus::performance::Span::new("shader-floating-presentation");
    let off = disabled(world);
    let FloatingScratch { owners, targets } = &mut *scratch;
    owners.clear();
    owners.extend(
        floating
            .iter(world)
            .map(|(entity, floating)| (entity, floating.record, floating.preview)),
    );
    if owners.is_empty() {
        *scratch = FloatingScratch::default();
        return;
    }
    for target in targets.values_mut() {
        target.current = false;
        target.slots.clear();
        target.labels.clear();
    }
    for &(_, record, _) in owners.iter() {
        targets.entry(record).or_default().current = true;
    }
    targets.retain(|_, target| target.current);
    for (entity, slot) in slots.iter(world) {
        if let Some(target) = targets.get_mut(&slot.0) {
            target.slots.push(entity);
        }
    }
    for (entity, label, node) in labels.iter(world) {
        if let Some(target) = targets.get_mut(&label.0) {
            let height = node.map_or(18.0, |node| {
                node.size().y * node.inverse_scale_factor() + 4.0
            });
            target.labels.push((entity, height));
        }
    }
    let camera_rotation = rotations
        .iter(world)
        .next()
        .map(|transform| transform.rotation.as_dquat());
    for (entity, record, preview) in owners.drain(..) {
        let Some(item) = world
            .get::<crate::native::canvas::CanvasItem>(record)
            .copied()
        else {
            world.despawn(entity);
            continue;
        };
        let Some(settings) = current_settings(world, record) else {
            world.despawn(entity);
            continue;
        };
        let (size, gap) = (settings.size, settings.gap);
        let spatial = crate::native::topology::spatial(world, record);
        let offset = spatial.rotation()
            * DVec3::new(
                0.0,
                f64::from(settings.elevation),
                -f64::from(item.size.y) * 0.5 + f64::from(size) * 0.5,
            );
        let mut placement = spatial;
        placement.elevation += offset.y;
        placement.depth = Some(1.0);
        if settings.billboard
            && root(world, record)
                .and_then(|root| world.get::<crate::native::topology::view::View>(root))
                .is_some_and(|view| view.spatial)
            && let Some(rotation) = camera_rotation
        {
            placement.rotation = (rotation
                * bevy::math::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2))
            .to_array();
        }
        let item = crate::native::canvas::CanvasItem {
            position: item.position + DVec2::new(offset.x, offset.z),
            size: Vec2::splat(size),
        };
        if world
            .get::<crate::native::canvas::CanvasItem>(entity)
            .is_none_or(|current| current.position != item.position || current.size != item.size)
        {
            world.entity_mut(entity).insert(item);
        }
        if world.get::<crate::native::topology::Spatial>(entity) != Some(&placement) {
            world.entity_mut(entity).insert(placement);
        }
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let size = px(size);
            if node.width != size {
                node.width = size;
            }
            if node.height != size {
                node.height = size;
            }
        }
        let settings = current_settings(world, record).unwrap();
        let distant = placement::distant(world, entity, record, settings, cameras);
        let frozen = settings.frozen || distant && settings.distance == DistancePolicy::Thumbnail;
        let motion = settings.motion;
        let data = world.get::<RecordData>(record).unwrap();
        let selected = data
            .extension
            .as_ref()
            .and_then(|extension| extension.selected(settings.effect_id.as_deref()));
        let current = world.get::<shader::EffectPreview>(preview);
        let quantity = current
            .filter(|current| current.quantity != data.quantity)
            .map(|_| data.quantity.clone());
        let effect = selected
            .filter(|effect| current.is_some_and(|current| &current.effect != *effect))
            .cloned();
        let disabled =
            off || selected.is_none() || distant && settings.distance == DistancePolicy::Hide;
        let target = &targets[&record];
        let label = (!target.slots.is_empty() || !target.labels.is_empty()).then(|| {
            labels::floating(
                world,
                entity,
                preview,
                selected,
                data.quantity.as_deref(),
                frozen,
            )
        });
        if let Some(mut floating) = world.get_mut::<Floating>(entity)
            && floating.distant != distant
        {
            floating.distant = distant;
        }
        if let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview) {
            if let Some(quantity) = quantity {
                shader.quantity = quantity;
            }
            if let Some(effect) = effect {
                shader.effect = effect;
                nucleus::performance::add("shader-floating-definition-copies", 1);
            }
            if shader.disabled != disabled {
                shader.disabled = disabled;
            }
            if shader.motion != motion {
                shader.motion = motion;
            }
            if shader.frozen != frozen {
                shader.frozen = frozen;
            }
        }
        let Some((label, prepared)) = label else {
            if world.get::<labels::FloatingText>(entity).is_some() {
                world.entity_mut(entity).remove::<labels::FloatingText>();
            }
            continue;
        };
        if let Some(prepared) = prepared {
            world.entity_mut(entity).insert(prepared);
        }
        let label_height = if label.is_empty() {
            0.0
        } else {
            target
                .labels
                .iter()
                .map(|(_, height)| *height)
                .fold(18.0_f32, f32::max)
        };
        for &slot in &target.slots {
            if let Some(mut node) = world.get_mut::<Node>(slot) {
                let height = px(size + gap + label_height);
                if node.height != height {
                    node.height = height;
                }
            }
        }
        for &(entity, _) in &target.labels {
            if let Some(mut text) = world.get_mut::<Text>(entity)
                && text.0.as_str() != label.as_ref()
            {
                text.0.clear();
                text.0.push_str(&label);
            }
            let display = if label.is_empty() {
                Display::None
            } else {
                Display::Flex
            };
            if let Some(mut node) = world.get_mut::<Node>(entity)
                && node.display != display
            {
                node.display = display;
            }
        }
    }
}

fn current_settings(world: &World, record: Entity) -> Option<&FloatingSettings> {
    let data = world.get::<RecordData>(record)?;
    let config = crate::native::protein_area::configuration_ref(world, data.binding.area)?;
    config
        .shader_effects
        .then(|| placement::effective_ref(world, &data.binding, &config.shader_placement))
}

fn update_surfaces(
    world: &mut World,
    backgrounds: &mut QueryState<(Entity, &'static Background)>,
    surfaces: &mut QueryState<Entity, With<Surface>>,
    mut scratch: Local<SurfaceScratch>,
) {
    let _publication = nucleus::performance::Span::new("shader-surface-presentation");
    let off = disabled(world);
    let SurfaceScratch {
        backgrounds: previous,
        owners,
        text_owners,
    } = &mut *scratch;
    previous.clear();
    previous.extend(
        backgrounds
            .iter(world)
            .map(|(entity, background)| (entity, background.preview)),
    );
    text_owners.clear();
    for (entity, preview) in previous.drain(..) {
        if world.get::<Surface>(entity).is_none() {
            if let Ok(preview) = world.get_entity_mut(preview) {
                preview.despawn();
            }
            world.entity_mut(entity).remove::<Background>();
            text_owners.push(entity);
        }
    }
    owners.clear();
    owners.extend(surfaces.iter(world));
    for entity in owners.drain(..) {
        let surface = world.get::<Surface>(entity).unwrap();
        let data = surface
            .record
            .as_deref()
            .and_then(|uid| world.resource::<Feeds>().get(&surface.source, uid))
            .and_then(|feed| feed.row.as_ref());
        let selected = data
            .and_then(|row| world.resource::<feeds::CheckedRows>().extension(row))
            .and_then(|extension| extension.selected(surface.effect_id.as_deref()));
        let valid_reference = surface.record.is_none()
            || if surface.quantity_only {
                data.is_some()
            } else {
                selected.is_some()
            };
        let effect = if surface.record.is_some() && !surface.quantity_only {
            selected.unwrap_or_else(|| feeds::unavailable_surface())
        } else {
            &surface.effect
        };
        let quantity = if surface.record.is_some() {
            data.and_then(|row| row.quantity.as_deref())
        } else {
            model::quantity_text(&surface.quantity)
        };
        let preview = world
            .get::<Background>(entity)
            .map(|background| background.preview)
            .filter(|preview| world.get_entity(*preview).is_ok());
        let mut quantity = preview
            .and_then(|preview| world.get::<shader::EffectPreview>(preview))
            .is_none_or(|preview| preview.quantity.as_deref() != quantity)
            .then(|| quantity.map(str::to_owned));
        let mut effect = preview
            .and_then(|preview| world.get::<shader::EffectPreview>(preview))
            .is_none_or(|preview| &preview.effect != effect)
            .then(|| effect.clone());
        let context = preview
            .and_then(|preview| world.get::<numeric::Context>(preview))
            .is_none_or(|context| {
                context.record != surface.record || context.source != surface.source
            })
            .then(|| numeric::Context {
                record: surface.record.clone(),
                source: surface.source.clone(),
            });
        let enabled = !surface.disabled
            && !off
            && valid_reference
            && (!surface.room_ambient
                || crate::native::topology::rooms::ambient_enabled(world, entity));
        let (strength, glow, motion, frozen) = (
            surface.strength,
            surface.glow,
            surface.motion,
            surface.frozen,
        );
        let preview = preview.unwrap_or_else(|| {
            let preview = shader::effect(
                world,
                entity,
                effect.take().expect("new surface effect"),
                quantity.take().expect("new surface quantity"),
            );
            let output = shader::background_output(world, preview);
            world.entity_mut(preview).insert((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                ZIndex(-1),
                bevy::picking::Pickable::IGNORE,
            ));
            world.entity_mut(output).insert((
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                bevy::picking::Pickable::IGNORE,
            ));
            world.entity_mut(entity).insert(Background { preview });
            preview
        });
        if let Some(context) = context {
            world.entity_mut(preview).insert(context);
        }
        let radius = world
            .get::<Node>(entity)
            .map(|node| node.border_radius)
            .unwrap_or_default();
        world
            .get_mut::<Node>(preview)
            .unwrap()
            .map_unchanged(|node| &mut node.border_radius)
            .set_if_neq(radius);
        let shadow = (enabled && glow > 0.0).then(|| ShadowStyle {
            color: Color::srgba(0.2, 0.45, 1.0, strength.min(0.35)),
            x_offset: px(0),
            y_offset: px(0),
            spread_radius: px(0),
            blur_radius: px(glow / 3.0),
        });
        let same_shadow =
            world
                .get::<BoxShadow>(preview)
                .is_some_and(|current| match shadow.as_ref() {
                    Some(shadow) => current.0.as_slice() == std::slice::from_ref(shadow),
                    None => current.0.is_empty(),
                });
        if !same_shadow {
            world
                .entity_mut(preview)
                .insert(BoxShadow(shadow.into_iter().collect()));
            nucleus::performance::add("shader-surface-shadow-updates", 1);
        }
        let output = shader::output(world, preview);
        world
            .get_mut::<Node>(output)
            .unwrap()
            .map_unchanged(|node| &mut node.border_radius)
            .set_if_neq(radius);
        if let Some(mut shader) = world.get_mut::<shader::EffectPreview>(preview) {
            if let Some(effect) = effect {
                shader.effect = effect;
            }
            if let Some(quantity) = quantity {
                shader.quantity = quantity;
            }
            if shader.strength != strength.min(0.65) {
                shader.strength = strength.min(0.65);
            }
            if shader.motion != motion {
                shader.motion = motion;
            }
            if shader.frozen != frozen {
                shader.frozen = frozen;
            }
            if shader.disabled == enabled {
                shader.disabled = !enabled;
            }
        }
        text_owners.push(entity);
    }
    refresh_text_protection(world, text_owners);
    if text_owners.is_empty() {
        *scratch = SurfaceScratch::default();
    }
}
