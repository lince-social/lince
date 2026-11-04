use super::*;

#[tokio::test]
async fn a_full_vt_queue_keeps_the_original_offset_and_defers_the_next_read() {
    let mut app = crate::native::sand_panel::tests::app();
    app.insert_resource(crate::native::wake::WakeSignal::new(|| {}));
    let parent = app.world_mut().spawn(Node::default()).id();
    let run = Run {
        command: nucleus::new_uid("r"),
        id: nucleus::new_uid("run"),
        script: "printf example".into(),
        cwd: "/tmp".into(),
        started_ms: 0,
        finished_ms: None,
        exit_code: None,
        error: None,
    };
    let owner = attach(app.world_mut(), parent, &run).unwrap();
    let (busy, _receiver) = worker::Worker::busy_for_test();
    app.world_mut()
        .get_mut::<TerminalSand>(owner)
        .unwrap()
        .worker = Some(busy);
    let request = app.world().get::<RunView>(owner).unwrap().request.clone();
    app.world_mut().get_mut::<RunView>(owner).unwrap().pending = true;
    let started = Instant::now();
    assert!(receive(
        app.world_mut(),
        &ServerMessage::Command {
            id: request,
            response: Response::Output {
                events: vec![cell::command::Event::Output {
                    data_base64: "ZXhhbXBsZQ==".into()
                }],
                next_offset: 123,
                complete: false
            }
        }
    ));
    let view = app.world().get::<RunView>(owner).unwrap();
    assert_eq!(view.offset, 0);
    assert!(view.requested && !view.pending);
    assert!(view.next >= started + Duration::from_millis(20));
    let due = Instant::now() + Duration::from_secs(60);
    app.world_mut().get_mut::<RunView>(owner).unwrap().next = due;
    poll(app.world_mut(), owner);
    let view = app.world().get::<RunView>(owner).unwrap();
    assert_eq!(view.next, due);
    assert_eq!(view.offset, 0);
    assert!(view.requested && !view.pending);
}
