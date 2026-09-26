#![cfg(all(feature = "native", target_os = "linux"))]

use std::time::{Duration, Instant};

use lince_media::native::{
    capture::{ScreenCapture, screens},
    preview::screen_uses_picker,
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt as _, CreateGCAux, Rectangle},
};

#[test]
#[ignore = "Requires an isolated Xvfb display and LINCE_TEST_X11_DISPLAY matching DISPLAY"]
fn selected_x11_screen_delivers_the_rendered_pixels() {
    let display = std::env::var("LINCE_TEST_X11_DISPLAY").expect("Set an isolated test display");
    assert_eq!(std::env::var("DISPLAY").unwrap(), display);
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(!screen_uses_picker());
    let (connection, index) = x11rb::connect(Some(&display)).unwrap();
    let screen = &connection.setup().roots[index];
    let gc = connection.generate_id().unwrap();
    connection
        .create_gc(gc, screen.root, &CreateGCAux::new().foreground(0x00ff0000))
        .unwrap()
        .check()
        .unwrap();
    connection
        .poly_fill_rectangle(
            screen.root,
            gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: screen.width_in_pixels,
                height: screen.height_in_pixels,
            }],
        )
        .unwrap()
        .check()
        .unwrap();
    connection.flush().unwrap();
    let listed = screens().unwrap();
    assert!(!listed.is_empty());
    let mut capture = ScreenCapture::start(Some(listed[0].0)).unwrap();
    let start = Instant::now();
    loop {
        if let Some(frame) = capture.frame().unwrap() {
            assert_eq!(frame.width(), screen.width_in_pixels as u32);
            assert_eq!(frame.height(), screen.height_in_pixels as u32);
            assert!(
                frame
                    .rgba()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
}
