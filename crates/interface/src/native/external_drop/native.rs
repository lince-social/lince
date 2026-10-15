use bevy::prelude::*;
use x11rb::protocol::xproto::ConnectionExt;

#[derive(Resource, Default)]
struct Cursor {
    connection: Option<x11rb::rust_connection::RustConnection>,
    attempted: bool,
}

pub(super) fn position(world: &mut World, window: Entity) -> Option<Option<Vec2>> {
    let handle = world
        .get::<bevy::window::RawHandleWrapper>(window)?
        .get_window_handle();
    let id = match handle {
        raw_window_handle::RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok()?,
        raw_window_handle::RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return None,
    };
    world.init_resource::<Cursor>();
    let mut cursor = world.resource_mut::<Cursor>();
    if !cursor.attempted {
        cursor.attempted = true;
        cursor.connection = x11rb::connect(None).ok().map(|(connection, _)| connection);
    }
    let reply = cursor
        .connection
        .as_ref()?
        .query_pointer(id)
        .ok()?
        .reply()
        .ok()?;
    let window = world.get::<Window>(window)?;
    if !reply.same_screen
        || reply.win_x < 0
        || reply.win_y < 0
        || u32::from(reply.win_x as u16) >= window.resolution.physical_width()
        || u32::from(reply.win_y as u16) >= window.resolution.physical_height()
    {
        return Some(None);
    }
    Some(Some(
        Vec2::new(f32::from(reply.win_x), f32::from(reply.win_y))
            / window.resolution.scale_factor(),
    ))
}
