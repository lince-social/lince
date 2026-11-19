use super::*;

#[derive(Resource)]
struct Presentation {
    fields: QueryState<(
        Entity,
        &'static Unsaved,
        Option<&'static BorderToken>,
        Option<&'static crate::native::record_binding::TextBinding>,
        Option<&'static crate::native::record_view::RecordEditor>,
        Option<&'static EditableText>,
    )>,
    prepared: Vec<(Entity, bool, bool, Token)>,
    colors: Vec<(Entity, Token)>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            fields: world.query(),
            prepared: Vec::new(),
            colors: Vec::new(),
        }
    }
}

impl Presentation {
    fn prepare(&mut self, world: &World) {
        let _prepare = performance::Span::new(Stage::SaveFeedbackPrepare);
        self.prepared.clear();
        self.prepared.extend(self.fields.iter(world).map(
            |(entity, unsaved, border, binding, record, text)| {
                let dirty = match (binding, record, text) {
                    (Some(binding), _, Some(text)) => binding.editor_unsaved(text),
                    (_, Some(record), Some(text)) => {
                        record.pending.is_some() || text.value() != record.confirmed.as_str()
                    }
                    _ => unsaved.0,
                };
                (
                    entity,
                    dirty,
                    binding.is_some_and(|binding| {
                        crate::native::record_binding::failed(world, &binding.record)
                    }),
                    border.map_or(Token::Accent, |border| border.0),
                )
            },
        ));
        performance::add(
            Count::SaveFeedbackFieldsPrepared,
            self.prepared.len() as u64,
        );
    }

    fn publish(&mut self, world: &mut World, now: Instant) {
        let _publish = performance::Span::new(Stage::SaveFeedbackPublish);
        let mut deadline = None;
        let styled_fields = !self.colors.is_empty();
        self.colors.clear();
        for &(entity, dirty, failed, base_token) in &self.prepared {
            let (previous, styled) = {
                let mut feedback = world.get_mut::<SaveFeedback>(entity).unwrap();
                let styled = feedback.bypass_change_detection().styled_border.take();
                (*feedback, styled)
            };
            let mut next = previous;
            if let Some(border) = styled {
                next.border = Some(border);
            }
            let (alert, _) = next.update(dirty, failed, now);
            next.color_owned = true;
            if dirty && alert.is_none() {
                let at = next.since.unwrap() + SAVE_WARNING_AFTER;
                deadline = Some(deadline.map_or(at, |previous: Instant| previous.min(at)));
            }
            self.colors.push((entity, alert.unwrap_or(base_token)));
            let border = if alert.is_some() {
                let current = world.get::<Node>(entity).map(|node| node.border);
                if next.border.is_none() {
                    next.border = current;
                }
                current.map(|current| {
                    let mut border = next.border.unwrap_or(current);
                    if world
                        .get::<crate::native::sand::Borderless>(entity)
                        .is_some()
                    {
                        border = UiRect::bottom(px(1));
                    } else {
                        for side in [
                            &mut border.left,
                            &mut border.right,
                            &mut border.top,
                            &mut border.bottom,
                        ] {
                            if matches!(*side, Val::Auto | Val::Px(0.0) | Val::Percent(0.0)) {
                                *side = px(1);
                            }
                        }
                    }
                    border
                })
            } else {
                next.border.take()
            };
            if previous != next {
                *world.get_mut::<SaveFeedback>(entity).unwrap() = next;
                performance::count(Count::SaveFeedbackStateWrites);
            }
            if let Some(border) = border
                && let Some(mut node) = world.get_mut::<Node>(entity)
                && node.border != border
            {
                node.border = border;
                performance::count(Count::SaveFeedbackBorderWrites);
            }
        }
        if !self.colors.is_empty() {
            world
                .run_system_cached_with(style_borders, self.colors.as_slice())
                .expect("style save feedback");
        } else {
            if styled_fields {
                drop(
                    world
                        .unregister_system_cached(style_borders)
                        .expect("release save feedback style"),
                );
            }
            self.prepared.shrink_to_fit();
            self.colors.shrink_to_fit();
        }
        schedule(world, now, deadline);
    }
}

fn schedule(world: &mut World, now: Instant, deadline: Option<Instant>) {
    world.init_resource::<SaveWake>();
    let previous = world.resource::<SaveWake>().0;
    let mut scheduled = previous.filter(|at| *at > now);
    let wake = deadline
        .filter(|deadline| scheduled.is_none_or(|at| *deadline < at))
        .and_then(|deadline| {
            world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned()
                .map(|wake| (deadline, wake))
        });
    if let Some((deadline, _)) = &wake {
        scheduled = Some(*deadline);
    }
    if previous != scheduled {
        world.resource_mut::<SaveWake>().0 = scheduled;
        performance::count(Count::SaveFeedbackWakeWrites);
    }
    if let Some((deadline, wake)) = wake {
        wake.after(deadline.saturating_duration_since(now));
    }
}

pub(super) fn borders(world: &mut World, now: Instant) {
    let _borders = performance::Span::new(Stage::SaveFeedback);
    world.init_resource::<Presentation>();
    world.resource_scope(|world, mut presentation: Mut<Presentation>| {
        presentation.prepare(world);
        presentation.publish(world, now);
    });
}
