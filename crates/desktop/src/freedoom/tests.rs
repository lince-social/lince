use super::*;

#[test]
fn game_keys_match_the_engine_and_leave_tab_to_focus_navigation() {
    assert_eq!(key(KeyCode::KeyW), key(KeyCode::ArrowUp));
    assert_eq!(key(KeyCode::ControlLeft), Some(0x9d));
    assert_eq!(key(KeyCode::Tab), None);
}

#[cfg(unix)]
#[test]
fn embedded_game_produces_frames_pauses_and_terminates_on_drop() {
    let game = process::Game::start(None).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let frame = loop {
        if let Some(frame) = game.frame() {
            break frame;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Freedoom never produced a frame"
        );
        assert!(
            game.finished().is_none(),
            "Freedoom exited before its first frame"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(frame.len(), 320 * 200 * 4);
    assert!(frame.chunks_exact(4).all(|pixel| pixel[3] == 255));
    game.suspend(true);
    std::thread::sleep(std::time::Duration::from_millis(100));
    game.frame();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(game.frame().is_none());
    game.suspend(false);
    drop(game);
}
