use super::*;

#[derive(Component)]
struct Labels {
    stamp: (u64, i64, Settings),
    values: HashMap<String, String>,
}

pub(super) fn refresh(world: &mut World, owner: Entity, now: i64) {
    let view = world.get::<View>(owner).unwrap();
    let settings = &world.get::<TimeSettings>(owner).unwrap().0;
    let tick = now.div_euclid(1000);
    if world.get::<Labels>(owner).is_some_and(|labels| {
        labels.stamp.0 == view.revision && labels.stamp.1 == tick && labels.stamp.2 == *settings
    }) {
        return;
    }
    let values = view
        .entries
        .iter()
        .map(|entry| (entry.id.clone(), entry.time_label(settings, now)))
        .collect();
    let stamp = (view.revision, tick, settings.clone());
    world.entity_mut(owner).insert(Labels { stamp, values });
}

pub(super) fn label<'a>(world: &'a World, owner: Entity, entry: &Entry) -> &'a str {
    &world.get::<Labels>(owner).unwrap().values[&entry.id]
}

pub(super) fn wrap(world: &World, text: &str, font: f32, width: f32) -> String {
    let face = world
        .get_resource::<crate::native::theme::Typography>()
        .and_then(|typography| world.get_resource::<Assets<Font>>()?.get(&typography.0))
        .and_then(|font| ttf_parser::Face::parse(font.data.as_ref(), 0).ok());
    model::wrap_label(text, width, &|ch| {
        face.as_ref()
            .and_then(|face| {
                face.glyph_index(ch)
                    .and_then(|glyph| face.glyph_hor_advance(glyph))
            })
            .map_or(
                font * if ch as u32 >= 0x1100 { 1.0 } else { 0.62 },
                |width| f32::from(width) / f32::from(face.as_ref().unwrap().units_per_em()) * font,
            )
    })
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passage_of_time_and_record_changes_refresh_the_shared_cache() {
        let mut world = World::new();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(&mut world, owner);
        let entry = serde_json::from_value(serde_json::json!({
            "uid":"need", "record_uid":"record", "head":"Task", "quantity":"-1", "category":"timed",
            "time":{"from_ms":0,"until_ms":600000},
            "origin":{"kind":"need","allocation":{"cursor_offset_ms":0,"duration_ms":600000}}, "preview":false
        })).unwrap();
        world.get_mut::<View>(owner).unwrap().entries = vec![entry];
        refresh(&mut world, owner, 0);
        let initial = world.get::<Labels>(owner).unwrap().values["need"].clone();
        world.clear_trackers();
        refresh(&mut world, owner, 500);
        assert!(
            !world
                .entity(owner)
                .get_ref::<Labels>()
                .unwrap()
                .is_changed()
        );
        refresh(&mut world, owner, 60_000);
        assert_ne!(world.get::<Labels>(owner).unwrap().values["need"], initial);
        {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.entries[0].origin["allocation"]["duration_ms"] = serde_json::json!(1200000);
            view.revision += 1;
        }
        refresh(&mut world, owner, 60_000);
        let entry = &world.get::<View>(owner).unwrap().entries[0];
        assert_eq!(
            label(&world, owner, entry),
            entry.time_label(&world.get::<TimeSettings>(owner).unwrap().0, 60_000)
        );
    }
}
