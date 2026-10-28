use super::*;
use lince_interface::host::Direction;

fn update(fixture: &mut Fixture, limits: &[(usize, [u32; 2], [u32; 2])]) {
    for (index, minimum, maximum) in limits {
        let window = &fixture.clients[*index].0;
        window
            .role
            .set_min_size(minimum[0] as i32, minimum[1] as i32);
        window
            .role
            .set_max_size(maximum[0] as i32, maximum[1] as i32);
        window.surface.commit();
    }
    fixture.sync();
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    fixture.configure();
}

fn propose(fixture: &mut Fixture, start: [f64; 2], delta: [f64; 2], name: &str) {
    propose_at(fixture, 0, start, delta, name);
}

fn propose_at(fixture: &mut Fixture, index: usize, start: [f64; 2], delta: [f64; 2], name: &str) {
    fixture.resize_delta(index, start, delta);
    fixture.flush();
    fixture.configure();
    fixture.render(name);
}

fn exercise(spatial: bool, multiple: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut fixture = projected::fixture(
            &runtime,
            &directory
                .path()
                .join(format!("limits-{spatial}-{multiple}-{}", size[0])),
            size,
            spatial,
            multiple,
        );
        let name = |phase: &str| format!("limits-{phase}-{spatial}-{multiple}");
        fixture.activate(2);
        fixture.action(DesktopAction::Move(Direction::Down));
        fixture.activate(0);
        fixture.action(DesktopAction::CycleRowHeight);
        fixture.configure();
        let identities = fixture.identities();
        let original = fixture.sizes();
        assert!(original[0][1] > 260);
        assert_eq!(original[0][1], original[1][1]);
        let row_start = fixture.relative_origin(0, 2)[1];
        let gap = row_start - f64::from(original[0][1]);
        assert!(gap >= 0.0);
        let maximum = original[0][0] + 40;
        update(
            &mut fixture,
            &[
                (0, [120, 110], [maximum, 180]),
                (1, [220, 150], [1200, 260]),
                (2, [80, 80], [1200, 160]),
            ],
        );
        let mut baseline = original.clone();
        baseline[0][1] = 180;
        baseline[1][1] = 260;
        baseline[2][1] = 160;
        assert_eq!(fixture.sizes(), baseline);
        assert!((fixture.relative_origin(0, 2)[1] - row_start).abs() < 0.01);
        fixture.render(&name("bounded"));

        let start = fixture.begin_modifier_resize(0, [0.9, 0.5]);
        let fixed = fixture.geometry(2);
        propose(&mut fixture, start, [72.0, 0.0], &name("width-maximum"));
        baseline[0][0] = maximum;
        assert_eq!(fixture.sizes(), baseline);
        assert_eq!(fixture.resizing(), [true, false, false]);
        assert_eq!(fixture.geometry(2), fixed);
        fixture.end_modifier_resize(true);
        assert_eq!(fixture.sizes(), baseline);

        let committed = fixture.geometry(0);
        let start = fixture.begin_modifier_resize(0, [0.9, 0.5]);
        propose(
            &mut fixture,
            start,
            [120.0 - f64::from(maximum) - 24.0, 0.0],
            &name("width-minimum-cancel"),
        );
        let mut minimum = baseline.clone();
        minimum[0][0] = 120;
        assert_eq!(fixture.sizes(), minimum);
        assert_eq!(fixture.resizing(), [true, false, false]);
        fixture.end_modifier_resize(false);
        assert_eq!(fixture.sizes(), baseline);
        assert_eq!(fixture.geometry(0), committed);

        for commit in [false, true] {
            let start = fixture.begin_modifier_resize(0, [0.5, 0.99]);
            let reference = fixture.geometry(0).mapping;
            let before = fixture.origin_in(2, reference)[1];
            let committed = fixture.geometry(2);
            propose(&mut fixture, start, [0.0, 32.0], &name("row-capped"));
            assert_eq!(fixture.sizes(), baseline);
            assert_eq!(fixture.resizing(), [true, false, false]);
            assert!((fixture.origin_in(2, reference)[1] - before - 32.0).abs() < 0.01);
            fixture.end_modifier_resize(commit);
            assert_eq!(fixture.sizes(), baseline);
            if !commit {
                assert_eq!(fixture.geometry(2), committed);
            }
        }
        assert!((fixture.relative_origin(0, 2)[1] - row_start - 32.0).abs() < 0.01);

        fixture.activate(1);
        fixture.configure();
        fixture.render(&name("row-minimum-before"));
        let start = fixture.begin_modifier_resize(1, [0.5, 0.99]);
        propose_at(&mut fixture, 1, start, [0.0, -240.0], &name("row-minimum"));
        baseline[0][1] = 150;
        baseline[1][1] = 150;
        assert_eq!(fixture.sizes(), baseline);
        assert_eq!(fixture.resizing(), [true, true, false]);
        assert!((fixture.relative_origin(1, 2)[1] - 150.0 - gap).abs() < 0.01);
        fixture.end_modifier_resize(true);
        assert_eq!(fixture.sizes(), baseline);

        fixture.activate(0);
        fixture.configure();
        fixture.render(&name("row-minimum-committed"));
        let start = fixture.begin_modifier_resize(0, [0.9, 0.5]);
        propose(
            &mut fixture,
            start,
            [120.0 - f64::from(maximum) - 24.0, 0.0],
            &name("width-minimum-commit"),
        );
        baseline[0][0] = 120;
        assert_eq!(fixture.sizes(), baseline);
        fixture.end_modifier_resize(true);
        assert_eq!(fixture.sizes(), baseline);

        update(&mut fixture, &[(0, [1100, 820], [1200, 900])]);
        baseline[0] = [1100, 820];
        baseline[1][1] = 260;
        assert_eq!(fixture.sizes(), baseline);
        let usable = fixture.viewport().usable.unwrap();
        assert!(usable.size()[0] < 1100.0 && usable.size()[1] < 820.0);
        fixture.render(&name("oversized"));
        let committed = fixture.geometry(0);
        let point = fixture.point(0, [0.7, 0.5]);
        let mapping = committed.mapping;
        let surface = mapping.surface_point(point).unwrap();
        let uv: [f64; 2] = std::array::from_fn(|axis| {
            (surface[axis] - mapping.surface.origin()[axis]) / mapping.surface.size()[axis]
        });
        assert!(uv[1] > 0.2 && uv[1] < 0.8);
        for commit in [false, true] {
            let start = fixture.begin_modifier_resize(0, uv);
            propose(
                &mut fixture,
                start,
                [if uv[0] < 0.5 { 16.0 } else { -16.0 }, 0.0],
                &name("oversized-clamp"),
            );
            assert_eq!(fixture.sizes(), baseline);
            assert_eq!(fixture.resizing(), [true, false, false]);
            assert_eq!(fixture.geometry(0), committed);
            fixture.end_modifier_resize(commit);
            assert_eq!(fixture.sizes(), baseline);
        }
        fixture.activate(2);
        fixture.configure();
        fixture.render(&name("other-row"));
        assert_eq!(fixture.sizes(), baseline);
        fixture.activate(0);
        fixture.configure();
        fixture.render(&name("oversized-return"));
        assert_eq!(fixture.sizes(), baseline);
        fixture.normal_context_clicks(0);
        assert_eq!(fixture.identities(), identities);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn flat_tiled_limits_preserve_row_intent_client_sizes_and_oversized_input() {
    exercise(false, false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn spatial_tiled_limits_preserve_row_intent_client_sizes_and_oversized_input() {
    exercise(true, false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn secondary_flat_tiled_limits_preserve_row_intent_and_live_companion() {
    exercise(false, true);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn secondary_spatial_tiled_limits_preserve_row_intent_and_live_companion() {
    exercise(true, true);
}
