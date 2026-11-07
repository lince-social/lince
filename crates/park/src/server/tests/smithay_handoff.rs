use super::*;
use smithay::{
    utils::user_data::UserDataMap,
    wayland::{
        compositor::{CachedState, SurfaceAttributes},
        shm::{ShmBufferUserData, handoff::SharedPool, with_buffer_contents},
    },
};
use std::{
    io,
    os::unix::fs::{FileExt, MetadataExt},
};

#[derive(Debug)]
struct RestoreClient;
impl smithay::reexports::wayland_server::backend::ClientData for RestoreClient {}

#[test]
fn smithay_shm_restores_an_orphaned_pool_with_shared_backing_and_fresh_typed_buffers() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [4, 4]);
    harness.sync();
    let source = harness
        .server
        .state
        .buffers
        .values()
        .next()
        .unwrap()
        .buffer();
    let (pool, layout) =
        SharedPool::from_buffer(source.data::<ShmBufferUserData>().unwrap()).unwrap();
    assert!(pool.same_pool(&SharedPool::from_pool(&pool.pool_data())));
    let (descriptor, size) = pool.checkpoint().unwrap();
    assert_eq!(size.get(), 64);
    let file = std::fs::File::from(descriptor.try_clone().unwrap());
    file.write_all_at(&[19, 23, 29, 31], 0).unwrap();
    let restored = SharedPool::restore(descriptor, size).unwrap();
    let (restored_descriptor, restored_size) = restored.checkpoint().unwrap();
    let restored_file = std::fs::File::from(restored_descriptor);
    assert_eq!(
        (
            file.metadata().unwrap().dev(),
            file.metadata().unwrap().ino()
        ),
        (
            restored_file.metadata().unwrap().dev(),
            restored_file.metadata().unwrap().ino()
        )
    );
    assert_eq!(restored_size, size);
    let server = Server::new([640, 480]).unwrap();
    let mut handle = server.display.handle();
    let (stream, _peer) = UnixStream::pair().unwrap();
    let client = handle
        .insert_client(stream, Arc::new(RestoreClient))
        .unwrap();
    let first = client
        .create_resource::<WlBuffer, _, State>(
            &handle,
            1,
            restored.buffer_data(&server.state.shm, layout).unwrap(),
        )
        .unwrap();
    let second = client
        .create_resource::<WlBuffer, _, State>(
            &handle,
            1,
            restored.buffer_data(&server.state.shm, layout).unwrap(),
        )
        .unwrap();
    for buffer in [&first, &second] {
        let pixels = with_buffer_contents(buffer, |ptr, length, data| {
            assert_eq!(length, 64);
            assert_eq!(data.stride, 16);
            unsafe { std::slice::from_raw_parts(ptr, 4).to_vec() }
        })
        .unwrap();
        assert_eq!(pixels, [19, 23, 29, 31]);
    }
    file.write_all_at(&[37, 41, 43, 47], 0).unwrap();
    for buffer in [source, &first, &second] {
        assert_eq!(
            with_buffer_contents(buffer, |ptr, _, _| unsafe {
                std::slice::from_raw_parts(ptr, 4).to_vec()
            })
            .unwrap(),
            [37, 41, 43, 47]
        );
    }
    let mut invalid = layout;
    invalid.stride = i32::MAX;
    assert!(restored.buffer_data(&server.state.shm, invalid).is_err());
    invalid = layout;
    invalid.offset = 64;
    assert!(restored.buffer_data(&server.state.shm, invalid).is_err());
    invalid = layout;
    invalid.format = smithay::reexports::wayland_server::protocol::wl_shm::Format::Rgb565;
    assert!(restored.buffer_data(&server.state.shm, invalid).is_err());
}

#[test]
fn smithay_cache_checkpoint_keeps_current_pending_and_synchronized_commits_separate() {
    let mut harness = Harness::new();
    let parent = harness.window();
    let qh = harness.queue.handle();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let _subsurface = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &parent.surface, &qh, ());
    child.set_buffer_scale(2);
    child.commit();
    harness.sync();
    child.set_buffer_scale(3);
    harness.sync();
    let surface = harness
        .server
        .state
        .surfaces
        .iter()
        .find(|surface| surface.id().protocol_id() == child.id().protocol_id())
        .unwrap()
        .clone();
    with_states(&surface, |states| {
        let cache = states.cached_state.get::<SurfaceAttributes>();
        let snapshot = cache
            .handoff_map(|attributes| Ok::<_, io::Error>(attributes.buffer_scale))
            .unwrap();
        assert_eq!(snapshot.current, 1);
        assert_eq!(snapshot.pending, 3);
        assert_eq!(snapshot.cache.len(), 1);
        assert_eq!(snapshot.cache[0].1, 2);
        let capacity = snapshot.cache.capacity();
        let restored = CachedState::from_handoff(snapshot);
        let repeated = restored
            .handoff_map(|scale| Ok::<_, io::Error>(*scale))
            .unwrap();
        assert_eq!(repeated.current, 1);
        assert_eq!(repeated.pending, 3);
        assert_eq!(repeated.cache[0].1, 2);
        assert_eq!(repeated.cache.capacity(), capacity);
        drop(cache);
        let mut visited = false;
        states
            .cached_state
            .handoff_visit(|cache| {
                if cache.is::<Mutex<CachedState<SurfaceAttributes>>>() {
                    visited = true;
                }
                Ok(())
            })
            .unwrap();
        assert!(visited);
    });
    parent.surface.commit();
    harness.sync();
    with_states(&surface, |states| {
        let snapshot = states
            .cached_state
            .get::<SurfaceAttributes>()
            .handoff_map(|attributes| Ok::<_, io::Error>(attributes.buffer_scale))
            .unwrap();
        assert_eq!(snapshot.current, 3);
        assert_eq!(snapshot.pending, 3);
        assert!(snapshot.cache.is_empty());
    });
}

#[test]
fn smithay_user_data_capture_requires_the_owner_of_non_send_state() {
    let data = Arc::new(UserDataMap::new());
    data.insert_if_missing(|| std::rc::Rc::new(73_u32));
    let mut visited = false;
    data.handoff_visit(|value| {
        assert_eq!(**value.downcast_ref::<std::rc::Rc<u32>>().unwrap(), 73);
        visited = true;
        Ok(())
    })
    .unwrap();
    assert!(visited);
    let other = data.clone();
    let error = std::thread::spawn(move || {
        other.handoff_visit(|_| panic!("Non-Send data escaped its owner"))
    })
    .join()
    .unwrap()
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

static NAMED_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

fn named_commit_hook(_: &mut State, _: &DisplayHandle, _: &WlSurface) {
    NAMED_HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn smithay_checkpoint_identifies_known_stateless_hooks_and_distinguishes_captures() {
    use smithay::wayland::compositor::{self, handoff};
    let mut harness = Harness::new();
    let window = harness.window();
    let surface = harness.server.state.surfaces.iter().next().unwrap().clone();
    let original = handoff::surface_hooks(&surface).unwrap();
    let xdg_commit = xdg::handoff::toplevel_commit_identity::<State>();
    assert_eq!(xdg_commit.capture_size, 0);
    assert!(original.post_commit.contains(&xdg_commit));
    let named = handoff::commit_callback_identity::<State, _>(named_commit_hook);
    assert_eq!(named.capture_size, 0);
    let hook_id = compositor::add_post_commit_hook::<State, _>(&surface, named_commit_hook);
    let captured = Arc::new(AtomicUsize::new(0));
    let target = captured.clone();
    let closure = move |_: &mut State, _: &DisplayHandle, _: &WlSurface| {
        target.fetch_add(1, Ordering::SeqCst);
    };
    let captured_identity = handoff::commit_callback_identity::<State, _>(closure.clone());
    assert_ne!(named, captured_identity);
    assert!(captured_identity.capture_size > 0);
    let captured_id = compositor::add_post_commit_hook::<State, _>(&surface, closure);
    let hooks = handoff::surface_hooks(&surface).unwrap();
    assert!(hooks.post_commit.contains(&named));
    assert!(hooks.post_commit.contains(&captured_identity));
    let before = NAMED_HOOK_CALLS.load(Ordering::SeqCst);
    window.surface.commit();
    harness.sync();
    assert_eq!(NAMED_HOOK_CALLS.load(Ordering::SeqCst), before + 1);
    assert_eq!(captured.load(Ordering::SeqCst), 1);
    compositor::remove_post_commit_hook(&surface, hook_id);
    compositor::remove_post_commit_hook(&surface, captured_id);
    assert_eq!(
        handoff::surface_hooks(&surface).unwrap().post_commit,
        original.post_commit
    );
    let recreated = compositor::add_post_commit_hook::<State, _>(&surface, named_commit_hook);
    assert!(
        handoff::surface_hooks(&surface)
            .unwrap()
            .post_commit
            .contains(&named)
    );
    window.surface.commit();
    harness.sync();
    assert_eq!(NAMED_HOOK_CALLS.load(Ordering::SeqCst), before + 2);
    assert_eq!(captured.load(Ordering::SeqCst), 1);
    compositor::remove_post_commit_hook(&surface, recreated);
}

#[test]
fn smithay_restores_fresh_typed_surface_region_and_scale_data_without_creation_events() {
    use smithay::wayland::compositor::{self, RegionUserData, SurfaceUserData};
    let mut server = Server::new([640, 480]).unwrap();
    let mut handle = server.display.handle();
    let (stream, _peer) = UnixStream::pair().unwrap();
    let client = handle
        .insert_client(stream, Arc::new(RestoreClient))
        .unwrap();
    let surface = client
        .create_resource::<WlSurface, _, State>(&handle, 6, SurfaceUserData::for_handoff::<State>())
        .unwrap();
    assert!(
        server
            .state
            .compositor
            .register_handoff_surface::<()>(&surface)
            .is_err()
    );
    server
        .state
        .compositor
        .register_handoff_surface::<State>(&surface)
        .unwrap();
    assert!(
        server
            .state
            .compositor
            .register_handoff_surface::<State>(&surface)
            .is_err()
    );
    let mut visited = 0;
    compositor::with_surface_tree_downward(
        &surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |_, _, _| {
            visited += 1;
        },
        |_, _, _| true,
    );
    assert_eq!(visited, 1);
    with_states(&surface, |states| {
        let mut cache = states.cached_state.get::<SurfaceAttributes>();
        assert_eq!(cache.pending().handoff_client_scale(), 1.0);
        cache.pending().restore_client_scale(1.75).unwrap();
        assert_eq!(cache.pending().handoff_client_scale(), 1.75);
        assert_eq!(cache.current().handoff_client_scale(), 1.0);
        assert!(cache.pending().restore_client_scale(f64::NAN).is_err());
        assert!(cache.pending().restore_client_scale(0.0).is_err());
    });
    let mut attributes = RegionAttributes::default();
    attributes.rects.push((
        compositor::RectangleKind::Add,
        Rectangle::new((2, 3).into(), (4, 5).into()),
    ));
    let region = client.create_resource::<smithay::reexports::wayland_server::protocol::wl_region::WlRegion, _, State>(&handle, 1, RegionUserData::from_handoff(attributes)).unwrap();
    let attributes = compositor::get_region_attributes(&region);
    assert!(attributes.contains((3, 4)));
    assert!(!attributes.contains((0, 0)));
    assert!(
        compositor::handoff::surface_hooks(&surface)
            .unwrap()
            .post_commit
            .is_empty()
    );
}

#[test]
fn smithay_serial_checkpoint_keeps_wraparound_and_skips_zero_after_restore() {
    let source = smithay::utils::SerialCounter::new();
    source.restore_handoff(u32::MAX);
    assert_eq!(u32::from(source.next_serial()), u32::MAX);
    assert_eq!(source.handoff_value(), 0);
    let next = smithay::utils::SerialCounter::new();
    next.restore_handoff(source.handoff_value());
    assert_eq!(u32::from(next.next_serial()), 1);
    assert_eq!(u32::from(next.next_serial()), 2);
    assert_eq!(next.handoff_value(), 3);
}

#[test]
fn smithay_isolated_surface_and_client_checkpoint_preserve_roles_serials_and_queue_allocations() {
    use smithay::wayland::compositor::handoff;
    let mut harness = Harness::new();
    let _window = harness.window();
    let source = harness.server.state.surfaces.iter().next().unwrap().clone();
    let checkpoint = handoff::isolated_surface(&source).unwrap();
    assert_eq!(checkpoint.role.as_deref(), Some(xdg::XDG_TOPLEVEL_ROLE));
    assert!(checkpoint.next_transaction > 0);
    let client = source.client().unwrap();
    let client_state = &client.get_data::<ClientState>().unwrap().compositor;
    client_state.set_client_scale(1.25);
    let client_checkpoint = client_state.handoff_state().unwrap();
    assert!(client_checkpoint.queue.is_some());
    let restored_client = CompositorClientState::from_handoff(client_checkpoint.clone()).unwrap();
    let repeated = restored_client.handoff_state().unwrap();
    assert_eq!(repeated.scale, 1.25);
    let before = client_checkpoint.queue.as_ref().unwrap();
    let after = repeated.queue.as_ref().unwrap();
    assert_eq!(after.transaction_capacity, before.transaction_capacity);
    assert_eq!(after.seen_capacity, before.seen_capacity);
    assert_eq!(
        after.seen_surfaces.iter().collect::<BTreeSet<_>>(),
        before.seen_surfaces.iter().collect::<BTreeSet<_>>()
    );
    let mut invalid = client_checkpoint.clone();
    invalid.queue.as_mut().unwrap().transaction_capacity = usize::MAX;
    assert!(CompositorClientState::from_handoff(invalid).is_err());
    let mut invalid = client_checkpoint;
    invalid.queue = Some(handoff::EmptyQueueCheckpoint {
        transaction_capacity: 0,
        seen_capacity: 4,
        seen_surfaces: vec![7, 7],
    });
    assert!(CompositorClientState::from_handoff(invalid).is_err());
    let mut server = Server::new([640, 480]).unwrap();
    let mut handle = server.display.handle();
    let (stream, _peer) = UnixStream::pair().unwrap();
    let client = handle
        .insert_client(stream, Arc::new(RestoreClient))
        .unwrap();
    let surface = client
        .create_resource::<WlSurface, _, State>(
            &handle,
            6,
            smithay::wayland::compositor::SurfaceUserData::for_handoff::<State>(),
        )
        .unwrap();
    server
        .state
        .compositor
        .register_handoff_surface::<State>(&surface)
        .unwrap();
    assert!(handoff::restore_isolated_surface(&surface, checkpoint.clone(), |_| None).is_err());
    handoff::restore_isolated_surface(&surface, checkpoint.clone(), |role| {
        (role == xdg::XDG_TOPLEVEL_ROLE).then_some(xdg::XDG_TOPLEVEL_ROLE)
    })
    .unwrap();
    let repeated = handoff::isolated_surface(&surface).unwrap();
    assert_eq!(repeated.role, checkpoint.role);
    assert_eq!(repeated.next_transaction, checkpoint.next_transaction);
    assert!(
        handoff::restore_isolated_surface(&surface, checkpoint, |_| Some(xdg::XDG_TOPLEVEL_ROLE))
            .is_err()
    );
}

#[test]
fn smithay_checkpoint_refuses_live_transactions_and_source_resumes_when_the_blocker_clears() {
    use smithay::wayland::compositor::{self, handoff};
    let mut harness = Harness::new();
    let window = harness.window();
    let surface = harness.server.state.surfaces.iter().next().unwrap().clone();
    let blocker = compositor::Barrier::new(false);
    compositor::add_blocker(&surface, blocker.clone());
    assert_eq!(
        handoff::isolated_surface(&surface).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    window.surface.commit();
    harness.sync();
    let client = surface.client().unwrap();
    let compositor = &client.get_data::<ClientState>().unwrap().compositor;
    assert_eq!(
        compositor.handoff_state().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    blocker.signal();
    compositor.blocker_cleared(&mut harness.server.state, &harness.server.display.handle());
    harness.sync();
    assert!(compositor.handoff_state().is_ok());
    assert!(handoff::isolated_surface(&surface).is_ok());
}
