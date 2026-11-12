use bevy::{
    app::{Main, MainScheduleOrder},
    ecs::{
        schedule::{
            InternedScheduleLabel, Schedule, ScheduleLabel, Schedules, SingleThreadedExecutor,
        },
        system::{IntoSystem, System},
    },
    prelude::*,
};
use lince_interface::host::performance::{self, Stage};
use std::time::{Duration, Instant};

pub(super) fn install(app: &mut App) -> Result<(), &'static str> {
    #[cfg(test)]
    if std::env::var_os("LINCE_PARK_TEST_ORIGINAL_MAIN").is_some() {
        return Ok(());
    }
    let mut schedules = app.world_mut().resource_mut::<Schedules>();
    let original = schedules.get(Main).ok_or("Main schedule is missing")?;
    let graph = original.graph();
    if original.systems().is_ok() || graph.systems.is_initialized() {
        return Err("Main schedule has already initialized");
    }
    let expected = IntoSystem::into_system(Main::run_main).system_type();
    let systems = graph.systems.iter().collect::<Vec<_>>();
    if systems.len() != 1 || systems[0].1.system_type() != expected {
        return Err("Main has a custom dispatcher");
    }
    if !systems[0].2.is_empty()
        || graph
            .system_sets
            .iter()
            .any(|(_, _, conditions)| !conditions.is_empty())
    {
        return Err("Main has custom run conditions");
    }
    let mut observed = Schedule::new(Main);
    observed.set_executor(SingleThreadedExecutor::new());
    observed.add_systems(run);
    schedules.insert(observed);
    drop(schedules);
    app.add_systems(
        PostUpdate,
        text_work
            .in_set(bevy::ui::UiSystems::Content)
            .after(bevy::text::detect_text_needs_rerender)
            .before(bevy::ui::widget::measure_text_system),
    );
    Ok(())
}

fn text_work(
    blocks: Query<
        (
            Ref<Text>,
            Ref<bevy::ui::ContentSize>,
            &bevy::text::ComputedTextBlock,
            Ref<ComputedUiRenderTargetInfo>,
            &ComputedNode,
            &InheritedVisibility,
        ),
        With<Node>,
    >,
    spans: Query<&TextSpan>,
    rem: Option<Res<bevy::text::RemSize>>,
) {
    use performance::Count;
    let mut counts = [0u64; 8];
    for (text, size, computed, target, node, visible) in &blocks {
        let scale = (target.scale_factor() - node.inverse_scale_factor().recip()).abs() > 1e-5;
        let style = computed.needs_rerender(false, false);
        let units = !style
            && computed.needs_rerender(
                target.is_changed(),
                rem.as_ref().is_some_and(|rem| rem.is_changed()),
            );
        if !(size.is_added() || text.is_changed() || scale || style || units) {
            continue;
        }
        let bytes = text.0.len()
            + computed
                .entities()
                .iter()
                .filter_map(|span| spans.get(span.entity).ok())
                .map(|span| span.0.len())
                .sum::<usize>();
        counts[0] += 1;
        counts[1] += bytes as u64;
        counts[2] += u64::from(size.is_added());
        counts[3] += u64::from(text.is_changed());
        counts[4] += u64::from(style);
        counts[5] += u64::from(scale);
        counts[6] += u64::from(units);
        counts[7] += u64::from(!visible.get() || node.is_empty());
    }
    for (count, value) in [
        Count::TextMeasureCandidates,
        Count::TextMeasureBytes,
        Count::TextMeasureNew,
        Count::TextMeasureChanged,
        Count::TextMeasureStyle,
        Count::TextMeasureScale,
        Count::TextMeasureUnits,
        Count::TextMeasureInvisible,
    ]
    .into_iter()
    .zip(counts)
    {
        performance::add(count, value);
    }
}

fn stages(label: InternedScheduleLabel) -> (Stage, Stage) {
    if label == First.intern() {
        (Stage::NativePhaseFirst, Stage::NativePhaseFirstCpu)
    } else if label == PreUpdate.intern() {
        (Stage::NativePhasePreUpdate, Stage::NativePhasePreUpdateCpu)
    } else if label == RunFixedMainLoop.intern() {
        (Stage::NativePhaseFixedLoop, Stage::NativePhaseFixedLoopCpu)
    } else if label == Update.intern() {
        (Stage::NativePhaseUpdate, Stage::NativePhaseUpdateCpu)
    } else if label == SpawnScene.intern() {
        (
            Stage::NativePhaseSpawnScene,
            Stage::NativePhaseSpawnSceneCpu,
        )
    } else if label == PostUpdate.intern() {
        (
            Stage::NativePhasePostUpdate,
            Stage::NativePhasePostUpdateCpu,
        )
    } else if label == Last.intern() {
        (Stage::NativePhaseLast, Stage::NativePhaseLastCpu)
    } else {
        (Stage::NativePhaseCustom, Stage::NativePhaseCustomCpu)
    }
}

fn measured(world: &mut World, label: InternedScheduleLabel, stages: (Stage, Stage)) {
    let wall = Instant::now();
    let cpu = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    let executed = world.try_run_schedule(label).is_ok();
    let after = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    let elapsed = wall.elapsed();
    if !executed {
        performance::count(performance::Count::NativePhaseMissing);
        return;
    }
    performance::record(stages.0, elapsed);
    let nanoseconds = (i128::from(after.tv_sec) - i128::from(cpu.tv_sec)) * 1_000_000_000
        + i128::from(after.tv_nsec)
        - i128::from(cpu.tv_nsec);
    if let Ok(nanoseconds) = u64::try_from(nanoseconds) {
        performance::record(stages.1, Duration::from_nanos(nanoseconds));
    } else {
        performance::count(performance::Count::TimingRejected);
    }
}

fn run(world: &mut World, mut initialized: Local<bool>) {
    if !*initialized {
        world.resource_scope(|world, order: Mut<MainScheduleOrder>| {
            for &label in &order.startup_labels {
                measured(
                    world,
                    label,
                    (Stage::NativePhaseStartup, Stage::NativePhaseStartupCpu),
                );
            }
        });
        *initialized = true;
    }
    world.resource_scope(|world, order: Mut<MainScheduleOrder>| {
        for &label in &order.labels {
            measured(world, label, stages(label));
        }
    });
}

pub(super) fn render_diagnostics(
    diagnostics: Option<Res<bevy::diagnostic::DiagnosticsStore>>,
    mut seen: Local<std::collections::BTreeMap<String, Instant>>,
) {
    if !performance::enabled() {
        return;
    }
    let Some(diagnostics) = diagnostics else {
        return;
    };
    for diagnostic in diagnostics.iter() {
        let path = diagnostic.path().to_string();
        if !path.starts_with("render/") {
            continue;
        }
        let Some(measurement) = diagnostic.measurement() else {
            continue;
        };
        if seen
            .get(&path)
            .is_some_and(|previous| *previous >= measurement.time)
        {
            continue;
        }
        if seen.len() >= 512 && !seen.contains_key(&path) {
            performance::count(performance::Count::TimingCapacity);
            continue;
        }
        seen.insert(path.clone(), measurement.time);
        performance::record_render_diagnostic(&path, &diagnostic.suffix, measurement.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default, PartialEq, Debug)]
    struct Events(Vec<(&'static str, usize)>);

    #[derive(Component)]
    struct Pending(usize);

    #[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
    struct Extra;

    #[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
    struct Missing;

    fn fixture() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Events>()
            .add_systems(
                Startup,
                |mut commands: Commands, mut events: ResMut<Events>| {
                    commands.spawn(Pending(0));
                    events.0.push(("startup", 0));
                },
            )
            .add_systems(
                PreUpdate,
                |mut commands: Commands,
                 mut events: ResMut<Events>,
                 pending: Query<(Entity, &Pending)>| {
                    let generation = pending.iter().map(|(_, pending)| pending.0).sum::<usize>();
                    for (entity, _) in &pending {
                        commands.entity(entity).despawn();
                    }
                    commands.spawn(Pending(generation + 1));
                    events.0.push(("pre", generation));
                },
            )
            .add_systems(
                Update,
                |mut events: ResMut<Events>, pending: Query<&Pending>| {
                    events
                        .0
                        .push(("update", pending.iter().map(|pending| pending.0).sum()));
                },
            )
            .add_systems(
                Extra,
                |mut events: ResMut<Events>, pending: Query<&Pending>| {
                    events
                        .0
                        .push(("extra", pending.iter().map(|pending| pending.0).sum()));
                },
            )
            .add_systems(
                PostUpdate,
                |mut events: ResMut<Events>, pending: Query<&Pending>| {
                    events
                        .0
                        .push(("post", pending.iter().map(|pending| pending.0).sum()));
                },
            )
            .add_systems(
                Last,
                |mut events: ResMut<Events>, pending: Query<&Pending>| {
                    events
                        .0
                        .push(("last", pending.iter().map(|pending| pending.0).sum()));
                },
            );
        let mut order = app.world_mut().resource_mut::<MainScheduleOrder>();
        order.insert_after(Update, Extra);
        order.insert_after(Extra, Missing);
        app
    }

    #[test]
    fn observation_preserves_startup_deferred_commands_and_dynamic_order() {
        let mut baseline = fixture();
        let mut observed = fixture();
        install(&mut observed).unwrap();
        for index in 0..3 {
            if index == 1 {
                for app in [&mut baseline, &mut observed] {
                    let mut order = app.world_mut().resource_mut::<MainScheduleOrder>();
                    order.labels.retain(|label| *label != Extra.intern());
                    order.insert_after(PostUpdate, Extra);
                }
            }
            baseline.update();
            observed.update();
            assert_eq!(
                baseline.world().resource::<Events>(),
                observed.world().resource::<Events>()
            );
        }
        let events = &observed.world().resource::<Events>().0;
        assert_eq!(
            events.iter().filter(|event| event.0 == "startup").count(),
            1
        );
        assert_eq!(
            events,
            &[
                ("startup", 0),
                ("pre", 0),
                ("update", 1),
                ("extra", 1),
                ("post", 1),
                ("last", 1),
                ("pre", 1),
                ("update", 2),
                ("post", 2),
                ("extra", 2),
                ("last", 2),
                ("pre", 2),
                ("update", 3),
                ("post", 3),
                ("extra", 3),
                ("last", 3),
            ]
        );
    }

    #[test]
    fn initialized_or_custom_main_is_preserved_without_instrumentation() {
        use bevy::ecs::schedule::ConditionWithAccess;

        let mut initialized = fixture();
        initialized.update();
        assert!(install(&mut initialized).is_err());
        initialized.update();
        assert_eq!(initialized.world().resource::<Events>().0.len(), 11);
        let mut custom = fixture();
        custom.add_systems(Main, |mut events: ResMut<Events>| {
            events.0.push(("custom", 0))
        });
        assert!(install(&mut custom).is_err());
        custom.update();
        assert_eq!(
            custom
                .world()
                .resource::<Events>()
                .0
                .iter()
                .filter(|event| event.0 == "custom")
                .count(),
            1
        );
        let mut conditional = fixture();
        let schedule = conditional.get_schedule_mut(Main).unwrap();
        let key = schedule.graph().systems.iter().next().unwrap().0;
        schedule
            .graph_mut()
            .systems
            .get_conditions_mut(key)
            .unwrap()
            .push(ConditionWithAccess::new(Box::new(IntoSystem::into_system(
                || false,
            ))));
        assert!(install(&mut conditional).is_err());
        conditional.update();
        assert!(conditional.world().resource::<Events>().0.is_empty());
    }
}
