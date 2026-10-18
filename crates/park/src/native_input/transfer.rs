use super::{Editor, allowed, text_state};
use bevy::{asset::AssetId, prelude::*, text::{EditableText, TextEdit}};
use lince_interface::{host::{HostRequest, OutputId}, native};
use std::{collections::VecDeque, sync::{Arc, atomic::{AtomicBool, Ordering}}};

pub const MAX_TRANSFER: usize = 1024 * 1024;
pub const MAX_VALUE: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Destination {
    Editor { context: Editor, point: [f32; 2], transform: [f32; 6], size: [f32; 2], scale: f32 },
    Canvas(native::external_drop::NativeTarget),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub window: u64,
    pub output: OutputId,
    pub pointer: [f64; 2],
    pub destination: Destination,
}

impl Target {
    pub fn same_owner(&self, other: &Self) -> bool {
        self.window == other.window && self.output == other.output && match (&self.destination, &other.destination) {
            (Destination::Canvas(a), Destination::Canvas(b)) => a.same_owner(b),
            (Destination::Editor { context: a, transform: at, size: az, scale: ac, .. },
                Destination::Editor { context: b, transform: bt, size: bz, scale: bc, .. }) => a == b && at == bt && az == bz && ac == bc,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Payload { Text(String), Uri(String) }

#[derive(Clone)]
pub struct Request {
    pub token: u64,
    pub target: Target,
    pub payload: Payload,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Text, Uri }

pub fn mime_kind(mime: &str) -> Option<Kind> {
    if mime.len() > 256 { return None; }
    let mut parts = mime.split(';').map(str::trim);
    let base = parts.next()?;
    let kind = if base.eq_ignore_ascii_case("text/plain") { Kind::Text }
        else if base.eq_ignore_ascii_case("text/uri-list") { Kind::Uri } else { return None };
    for part in parts {
        let (name, value) = part.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("charset") || !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8") { return None; }
    }
    Some(kind)
}

pub fn decode(mime: &str, bytes: Vec<u8>) -> Result<Payload, String> {
    if bytes.is_empty() || bytes.len() > MAX_TRANSFER { return Err("The dropped data exceeds its supported bounds.".into()); }
    let kind = mime_kind(mime).ok_or("This drop type is unsupported.")?;
    let text = String::from_utf8(bytes).map_err(|_| "Dropped text must use UTF-8.")?;
    if text.contains('\0') { return Err("Dropped text contains an embedded NUL.".into()); }
    if kind == Kind::Text {
        if text.len() > MAX_VALUE { return Err("Drop text of at most 16 KiB.".into()); }
        return Ok(Payload::Text(text));
    }
    let mut uris = text.lines().map(|line| line.trim_end_matches('\r')).filter(|line| !line.is_empty() && !line.starts_with('#'));
    let uri = uris.next().ok_or("The drop contains no URI.")?;
    if uri.len() > MAX_VALUE || uri.chars().any(char::is_whitespace) { return Err("The dropped URI is invalid or too large.".into()); }
    if uris.next().is_some() { return Err("Drop one file or link at a time.".into()); }
    if !["file://", "http://", "https://"].iter().any(|scheme| uri.starts_with(scheme)) {
        return Err("Drop a local file or an HTTP(S) link.".into());
    }
    Ok(Payload::Uri(uri.into()))
}

#[derive(Resource, Default)]
struct Transfers {
    pending: VecDeque<Request>,
    checking: Vec<(u64, Entity, usize, usize, Arc<AtomicBool>)>,
    replies: VecDeque<(u64, bool)>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Transfers>().add_systems(PostUpdate, (
        prepare.after(super::apply).before(bevy::text::EditableTextSystems),
        finish.after(super::finish).before(native::record_binding::SyncBindings),
    ));
}

pub(crate) fn enqueue(world: &mut World, request: Request) {
    let mut transfers = world.resource_mut::<Transfers>();
    if transfers.pending.len() + transfers.checking.len() + transfers.replies.len() < 8 { transfers.pending.push_back(request); }
    else { request.cancelled.store(true, Ordering::Release); }
}

pub(crate) fn capture(world: &mut World, window: Entity, image: AssetId<Image>, output: OutputId, offset: [f64; 2]) -> Option<Target> {
    let point = world.get::<Window>(window)?.cursor_position()?;
    let focused = world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get());
    let editor = focused.and_then(|entity| {
        let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(entity)?.get()?;
        let bevy::camera::RenderTarget::Image(target) = world.get::<bevy::camera::RenderTarget>(camera)? else { return None };
        if target.handle.id() != image { return None; }
        let node = world.get::<ComputedNode>(entity)?;
        let transform = world.get::<UiGlobalTransform>(entity)?;
        let local = transform.try_inverse()?.transform_point2(point) + node.size() * 0.5;
        if !local.is_finite() || !local.cmpge(Vec2::ZERO).all() || !local.cmplt(node.size()).all()
            || world.get::<bevy::ui::CalculatedClip>(entity).is_some_and(|clip| !clip.clip.contains(point)) { return None; }
        let scale = node.inverse_scale_factor();
        if !scale.is_finite() || scale <= 0.0 { return None; }
        Some(Destination::Editor { context: text_state(world, entity, [0, 0, 1, 1])?,
            point: (local / scale).to_array(), transform: transform.to_cols_array(), size: node.size().to_array(), scale })
    });
    let destination = editor.or_else(|| native::external_drop::capture_native_target(world, window).map(Destination::Canvas))?;
    Some(Target { window: window.to_bits(), output, pointer: [f64::from(point.x) + offset[0], f64::from(point.y) + offset[1]], destination })
}

fn prepare(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Transfers>().pending);
    for request in requests {
        if request.cancelled.load(Ordering::Acquire) { continue; }
        let Request { token, target, payload, cancelled } = request;
        let window = world.query::<(Entity, &Window)>().iter(world).any(|(entity, _)| entity.to_bits() == target.window);
        let active_output = world.get_resource::<native::output_views::OutputViews>()
            .is_none_or(|views| views.focused == target.output && views.outputs.iter().any(|output| output.id == target.output && output.usable.is_some()));
        if !window || !active_output {
            world.resource_mut::<Transfers>().replies.push_back((token, false));
            continue;
        }
        match target.destination {
            Destination::Canvas(target) => {
                let result = match payload {
                    Payload::Uri(uri) => native::external_drop::receive_native_uri(world, &target, &uri),
                    Payload::Text(text) => {
                        let value = text.trim();
                        if ["file://", "http://", "https://"].iter().any(|prefix| value.starts_with(prefix)) && !value.chars().any(char::is_whitespace) {
                            native::external_drop::receive_native_uri(world, &target, value)
                        } else { native::external_drop::receive_native_text(world, &target, &text) }
                    }
                };
                if let Err(message) = &result { native::notifications::report(world, "Drop", message); }
                world.resource_mut::<Transfers>().replies.push_back((token, result.is_ok()));
            }
            Destination::Editor { context, point, transform, size, scale } => {
                let entity = world.query::<(Entity, &EditableText)>().iter(world)
                    .find_map(|(entity, _)| (entity.to_bits() == context.owner).then_some(entity));
                let value = match payload { Payload::Text(value) | Payload::Uri(value) => value };
                let Some(entity) = entity.filter(|entity| allowed(world, *entity)
                    && world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get()) == Some(*entity)
                    && text_state(world, *entity, context.caret).as_ref() == Some(&context)
                    && world.get::<UiGlobalTransform>(*entity).is_some_and(|current| current.to_cols_array() == transform)
                    && world.get::<ComputedNode>(*entity).is_some_and(|node| node.size().to_array() == size && node.inverse_scale_factor() == scale)) else {
                    world.resource_mut::<Transfers>().replies.push_back((token, false)); continue;
                };
                let input = world.get::<EditableText>(entity).unwrap();
                if value.is_empty() || value.len() > MAX_VALUE || value.contains('\0')
                    || !point.iter().all(|value| value.is_finite()) || !input.pending_edits.is_empty() || input.pending_paste.is_some()
                    || value.chars().any(|character| character.is_control() && !(input.allow_newlines && matches!(character, '\n' | '\t'))) {
                    world.resource_mut::<Transfers>().replies.push_back((token, false)); continue;
                }
                let before = input.value().into_iter().map(str::len).sum();
                let bytes = value.len();
                let mut input = world.get_mut::<EditableText>(entity).unwrap();
                input.queue_edit(TextEdit::clear_ime_compose());
                input.queue_edit(TextEdit::MoveToPoint(Vec2::from_array(point)));
                input.queue_edit(TextEdit::Insert(value.into()));
                world.resource_mut::<bevy::input_focus::InputFocus>().set(entity, bevy::input_focus::FocusCause::Navigated);
                world.resource_mut::<Transfers>().checking.push((token, entity, before, bytes, cancelled));
            }
        }
    }
}

fn finish(world: &mut World) {
    let checking = std::mem::take(&mut world.resource_mut::<Transfers>().checking);
    for (token, entity, before, bytes, cancelled) in checking {
        let accepted = !cancelled.load(Ordering::Acquire) && world.get::<EditableText>(entity)
            .is_some_and(|input| input.value().into_iter().map(str::len).sum::<usize>() == before.saturating_add(bytes));
        world.resource_mut::<Transfers>().replies.push_back((token, accepted));
    }
    let Some(sender) = world.get_resource::<native::layout::desktop::HostRequests>().map(|requests| requests.0.clone()) else { return };
    loop {
        let reply = world.resource_mut::<Transfers>().replies.pop_front();
        let Some((token, accepted)) = reply else { break };
        match sender.try_send(HostRequest::DropFinished { token, accepted }) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                world.resource_mut::<Transfers>().replies.push_front((token, accepted));
                if let Some(wake) = world.get_resource::<native::wake::WakeSignal>() { wake.ring(); }
                break;
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                world.resource_mut::<Transfers>().replies.clear();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn editor_request(world: &mut World, owner: Entity, value: &str) -> Request {
        let window = world.spawn(Window::default()).id();
        world.entity_mut(owner).insert((ComputedNode::default(), UiGlobalTransform::default()));
        let node = world.get::<ComputedNode>(owner).unwrap();
        Request {
            token: 7,
            target: Target { window: window.to_bits(), output: OutputId(1), pointer: [0.0; 2],
                destination: Destination::Editor {
                    context: text_state(world, owner, [0, 0, 1, 1]).unwrap(), point: [0.0; 2],
                    transform: world.get::<UiGlobalTransform>(owner).unwrap().to_cols_array(),
                    size: node.size().to_array(), scale: node.inverse_scale_factor(),
                } },
            payload: Payload::Text(value.into()), cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn process(world: &mut World) -> (u64, bool) {
        prepare(world);
        world.run_system_once(bevy::text::apply_text_edits).unwrap();
        finish(world);
        world.resource_mut::<Transfers>().replies.pop_front().unwrap()
    }

    #[test]
    fn editor_drop_acknowledges_only_after_existing_editor_accepts_unicode() {
        let (mut world, owner) = super::super::tests::editing_world("old");
        world.init_resource::<Transfers>();
        let request = editor_request(&mut world, owner, "é世");
        enqueue(&mut world, request);
        assert!(world.resource::<Transfers>().replies.is_empty());
        assert_eq!(process(&mut world), (7, true));
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "é世old");
    }

    #[test]
    fn rejected_filtered_or_oversized_editor_insert_is_not_acknowledged() {
        for limited in [false, true] {
            let (mut world, owner) = super::super::tests::editing_world("12");
            world.init_resource::<Transfers>();
            if limited {
                world.get_mut::<EditableText>(owner).unwrap().max_characters = Some(2);
            } else {
                world.entity_mut(owner).insert(bevy::text::EditableTextFilter::new(|character| character.is_ascii_digit()));
            }
            let request = editor_request(&mut world, owner, "letters");
            enqueue(&mut world, request);
            assert_eq!(process(&mut world), (7, false));
            assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "12");
        }
    }

    #[test]
    fn stale_focus_content_geometry_and_destroyed_windows_reject_drop() {
        for change in 0..4 {
            let (mut world, owner) = super::super::tests::editing_world("old");
            world.init_resource::<Transfers>();
            let request = editor_request(&mut world, owner, "wrong");
            match change {
                0 => { world.resource_mut::<bevy::input_focus::InputFocus>().clear(); }
                1 => { world.get_mut::<EditableText>(owner).unwrap().editor.set_text("changed"); }
                2 => { world.entity_mut(owner).remove::<UiGlobalTransform>(); }
                _ => { world.despawn(Entity::from_bits(request.target.window)); }
            }
            enqueue(&mut world, request);
            assert_eq!(process(&mut world), (7, false));
            assert!(!world.get::<EditableText>(owner).unwrap().value().to_string().contains("wrong"));
        }
    }

    #[test]
    fn cancelled_requests_do_not_edit_and_pending_transfers_are_bounded() {
        let (mut world, owner) = super::super::tests::editing_world("old");
        world.init_resource::<Transfers>();
        let request = editor_request(&mut world, owner, "wrong");
        request.cancelled.store(true, Ordering::Release);
        enqueue(&mut world, request);
        prepare(&mut world);
        assert!(world.resource::<Transfers>().checking.is_empty());
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "old");
        for _ in 0..8 {
            let request = editor_request(&mut world, owner, "text");
            enqueue(&mut world, request);
        }
        let excess = editor_request(&mut world, owner, "excess");
        let cancelled = excess.cancelled.clone();
        enqueue(&mut world, excess);
        assert!(cancelled.load(Ordering::Acquire));
        assert_eq!(world.resource::<Transfers>().pending.len(), 8);
    }

    #[test]
    fn payloads_preserve_utf8_and_reject_ambiguous_uri_lists() {
        assert_eq!(decode("text/plain; charset=\"UTF-8\"", "Olá 世界".as_bytes().to_vec()).unwrap(), Payload::Text("Olá 世界".into()));
        assert_eq!(decode("text/uri-list", b"# files\r\nfile:///tmp/a%20b.txt\r\n".to_vec()).unwrap(), Payload::Uri("file:///tmp/a%20b.txt".into()));
        for bytes in [b"file:///tmp/a\nfile:///tmp/b".as_slice(), b"file:///tmp/a b", b"javascript:alert(1)", b"\0"] {
            assert!(decode("text/uri-list", bytes.to_vec()).is_err());
        }
        assert!(decode("text/plain", vec![0xff]).is_err());
        assert!(decode("text/html", b"<b>text</b>".to_vec()).is_err());
        assert!(decode("text/plain", vec![b'a'; MAX_VALUE + 1]).is_err());
    }
}
