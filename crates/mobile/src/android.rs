use bevy::{prelude::*, text::EditableText};
use jni::{
    JNIEnv, JavaVM,
    objects::{JByteArray, JClass, JObject, JString, JValue},
    sys::{jboolean, jint, jlong},
};
use std::{
    collections::VecDeque,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
static EDITOR: Mutex<Option<(u64, String, String)>> = Mutex::new(None);
static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
static WAKE: OnceLock<lince_interface::wake::WakeSignal> = OnceLock::new();
static DECODING: AtomicBool = AtomicBool::new(false);
static FILE_CHOICE: Mutex<Option<(u64, String, String)>> = Mutex::new(None);

enum Event {
    Accessible(u64, i32),
    Edit(u64, String, bool),
    Back,
    Insets([i32; 4]),
    Qr(Result<String, String>),
    Attachment(u64, Result<nucleus::message::MessagePart, String>),
    FileNotice(String),
    #[cfg(all(feature = "android-smoke", debug_assertions))]
    Smoke(String),
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeAccessibility(
    _env: JNIEnv,
    _class: JClass,
    enabled: jboolean,
) {
    crate::accessibility::ENABLED.store(enabled != 0, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeAccessible(
    _env: JNIEnv,
    _class: JClass,
    token: jlong,
    action: jint,
) {
    enqueue(Event::Accessible(token as u64, action));
}

pub fn accessibility_snapshot(value: &str) -> Result<(), String> {
    with_activity(|env, activity| {
        let value = env.new_string(value)?;
        env.call_method(
            activity,
            "accessibilitySnapshot",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&value)],
        )
        .map(|_| ())
    })
}

pub fn open_link(value: &str) -> Result<(), String> {
    with_activity(|env, activity| {
        let value = env.new_string(value)?;
        env.call_method(
            activity,
            "openLink",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&value)],
        )
        .map(|_| ())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeSmoke(
    mut env: JNIEnv,
    _class: JClass,
    command: JString,
) {
    #[cfg(all(feature = "android-smoke", debug_assertions))]
    if let Ok(command) = env.get_string(&command) {
        let command: String = command.into();
        if command.len() <= 16384 {
            enqueue(Event::Smoke(command));
        }
    }
    #[cfg(not(all(feature = "android-smoke", debug_assertions)))]
    {
        let _ = (&mut env, command);
    }
}

pub fn choose_file(scope: String, thread: String) -> Result<(), String> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut choice = FILE_CHOICE.lock().map_err(|_| "File picker unavailable")?;
    if choice.is_some() {
        return Err("Finish the current file selection first".into());
    }
    *choice = Some((id, scope, thread));
    let result = with_activity(|env, activity| {
        env.call_method(activity, "chooseFile", "(J)V", &[JValue::Long(id as i64)])
            .map(|_| ())
    });
    if result.is_err() {
        *choice = None;
    }
    result
}

pub fn save_file(name: &str, mime_type: &str, bytes: &[u8]) -> Result<(), String> {
    with_activity(|env, activity| {
        let name = env.new_string(name)?;
        let mime = env.new_string(mime_type)?;
        let bytes = env.byte_array_from_slice(bytes)?;
        env.call_method(
            activity,
            "saveFile",
            "(Ljava/lang/String;Ljava/lang/String;[B)V",
            &[
                JValue::Object(&name),
                JValue::Object(&mime),
                JValue::Object(&bytes),
            ],
        )
        .map(|_| ())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeAttachment(
    mut env: JNIEnv,
    _class: JClass,
    id: jlong,
    name: JString,
    mime: JString,
    bytes: JByteArray,
) {
    let result = (|| -> Result<nucleus::message::MessagePart, String> {
        let length = env
            .get_array_length(&bytes)
            .map_err(|error| error.to_string())?;
        if length < 0 || length as usize > nucleus::message::MAX_CONTENT_BYTES {
            return Err("Choose a file smaller than 4 MiB".into());
        }
        let name: String = env
            .get_string(&name)
            .map_err(|error| error.to_string())?
            .into();
        let mime: String = env
            .get_string(&mime)
            .map_err(|error| error.to_string())?
            .into();
        let bytes = env
            .convert_byte_array(bytes)
            .map_err(|error| error.to_string())?;
        crate::attachments::prepare(name, mime, &bytes)
    })();
    enqueue(Event::Attachment(id as u64, result));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeAttachmentError(
    mut env: JNIEnv,
    _class: JClass,
    id: jlong,
    message: JString,
) {
    if let Ok(message) = env.get_string(&message) {
        enqueue(Event::Attachment(id as u64, Err(message.into())));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeFileNotice(
    mut env: JNIEnv,
    _class: JClass,
    message: JString,
) {
    if let Ok(message) = env.get_string(&message) {
        enqueue(Event::FileNotice(message.into()));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeCameraFrame(
    env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    width: jint,
    height: jint,
) {
    let length = i64::from(width) * i64::from(height);
    if width <= 0 || height <= 0 || length > 4_000_000 || DECODING.swap(true, Ordering::AcqRel) {
        return;
    }
    let pixels = env
        .get_array_length(&data)
        .ok()
        .filter(|size| i64::from(*size) >= length && i64::from(*size) <= 6_000_000)
        .and_then(|_| env.convert_byte_array(&data).ok());
    let Some(mut pixels) = pixels else {
        DECODING.store(false, Ordering::Release);
        return;
    };
    pixels.truncate(length as usize);
    std::thread::spawn(move || {
        if let Ok(Some(code)) = engine::pairing::decode_qr_luma(width as u32, height as u32, pixels)
        {
            enqueue(Event::Qr(Ok(code)));
        }
        DECODING.store(false, Ordering::Release);
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeCameraError(
    mut env: JNIEnv,
    _class: JClass,
    message: JString,
) {
    if let Ok(message) = env.get_string(&message) {
        enqueue(Event::Qr(Err(message.into())));
    }
}

pub fn discovery(enabled: bool, milliseconds: i64) -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(
            activity,
            "discovery",
            "(ZJ)V",
            &[JValue::Bool(enabled.into()), JValue::Long(milliseconds)],
        )
        .map(|_| ())
    })
}

pub fn scan() -> Result<(), String> {
    with_activity(|env, activity| env.call_method(activity, "scanQr", "()V", &[]).map(|_| ()))
}

#[bevy::prelude::bevy_main]
fn main() {
    let app = bevy::android::ANDROID_APP.get().expect("Android activity");
    let Some(directory) = app.internal_data_path() else {
        return;
    };
    initialize_tls().expect("Android certificate verification");
    crate::run(directory.join("lince"));
}

fn initialize_tls() -> Result<(), String> {
    with_activity(|env, activity| {
        let context = env
            .call_method(
                activity,
                "getApplicationContext",
                "()Landroid/content/Context;",
                &[],
            )?
            .l()?;
        let app = bevy::android::ANDROID_APP.get().unwrap();
        let vm = unsafe { jni_runtime::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        vm.attach_current_thread(|env| {
            let context =
                unsafe { jni_runtime::objects::JObject::from_raw(env, context.into_raw().cast()) };
            rustls_platform_verifier::android::init_with_env(env, context)
        })
        .map_err(|_| jni::errors::Error::JavaException)?;
        Ok(())
    })
}

pub fn wake(signal: lince_interface::wake::WakeSignal) {
    let _ = WAKE.set(signal);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeWake(_: JNIEnv, _: JClass) {
    if let Some(wake) = WAKE.get() {
        wake.ring();
    }
}

fn enqueue(event: Event) {
    if let Ok(mut events) = EVENTS.lock() {
        if events.len() < 16 {
            events.push_back(event);
        }
    }
    if let Some(wake) = WAKE.get() {
        wake.ring();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeEdit(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    value: JString,
    committed: jboolean,
) {
    if let Ok(value) = env.get_string(&value) {
        enqueue(Event::Edit(id as u64, value.into(), committed != 0));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeBack(_: JNIEnv, _: JClass) {
    enqueue(Event::Back);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_social_lince_mobile_MainActivity_nativeInsets(
    _: JNIEnv,
    _: JClass,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) {
    enqueue(Event::Insets([left, top, right, bottom]));
}

fn with_activity<T>(
    call: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
    let app = bevy::android::ANDROID_APP
        .get()
        .ok_or("Android activity unavailable")?;
    let vm =
        unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|error| error.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    call(&mut env, &activity).map_err(|error| error.to_string())
}

pub fn edit(
    event: On<Pointer<Click>>,
    inputs: Query<(&crate::app::Input, &EditableText)>,
    gesture: Res<crate::scroll::Gesture>,
    state: Res<crate::app::Mobile>,
) {
    if gesture.moved {
        return;
    }
    let Ok((input, text)) = inputs.get(event.entity) else {
        return;
    };
    open_editor(input, text, &state.scope_key());
}

pub fn open_editor(input: &crate::app::Input, text: &EditableText, scope: &str) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut editor) = EDITOR.lock() {
        *editor = Some((id, input.key.clone(), scope.into()));
    }
    let result = with_activity(|env, activity| {
        let title = env.new_string(&input.title)?;
        let value = env.new_string(text.value().to_string())?;
        env.call_method(
            activity,
            "edit",
            "(JLjava/lang/String;Ljava/lang/String;ZZ)V",
            &[
                JValue::Long(id as i64),
                JValue::Object(&title),
                JValue::Object(&value),
                JValue::Bool(text.allow_newlines.into()),
                JValue::Bool((input.key == "login/password").into()),
            ],
        )?;
        Ok(())
    });
    if result.is_err() {
        enqueue(Event::Edit(id, String::new(), false));
    }
}

pub fn finish() {
    let _ =
        with_activity(|env, activity| env.call_method(activity, "finish", "()V", &[]).map(|_| ()));
}

pub fn restart() -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(activity, "restartProfile", "()V", &[])
            .map(|_| ())
    })
}

pub fn receive(world: &mut World) {
    let events: Vec<_> = EVENTS
        .lock()
        .map(|mut events| events.drain(..).collect())
        .unwrap_or_default();
    for event in events {
        match event {
            Event::Accessible(token, action) => {
                crate::accessibility::activate(world, token, action)
            }
            #[cfg(all(feature = "android-smoke", debug_assertions))]
            Event::Smoke(command) => crate::smoke::command(world, &command),
            Event::FileNotice(message) => {
                world.resource_mut::<crate::app::Mobile>().status = message
            }
            Event::Attachment(id, result) => {
                let context = FILE_CHOICE.lock().ok().and_then(|mut choice| {
                    if choice
                        .as_ref()
                        .is_some_and(|(pending, _, _)| *pending == id)
                    {
                        choice.take()
                    } else {
                        None
                    }
                });
                if let Some((_, scope, thread)) = context {
                    crate::attachments::selected(world, &scope, thread, result);
                }
            }
            Event::Qr(result) => {
                let _ = with_activity(|env, activity| {
                    env.call_method(activity, "closeScanner", "()V", &[])
                        .map(|_| ())
                });
                match result.and_then(|code| {
                    engine::pairing::EnrolmentInvite::decode(&code)
                        .map(|_| code)
                        .map_err(|error| error.to_string())
                }) {
                    Ok(code) => {
                        for (input, mut text) in world
                            .query::<(&crate::app::Input, &mut EditableText)>()
                            .iter_mut(world)
                        {
                            if input.key == "organ/enrol" {
                                text.editor.set_text(&code);
                            }
                        }
                        let mut state = world.resource_mut::<crate::app::Mobile>();
                        state.drafts.insert("organ/enrol".into(), code);
                        state.status =
                            "Device enrolment QR read. Review the Organ and choose Join.".into();
                    }
                    Err(error) => world.resource_mut::<crate::app::Mobile>().status = error,
                }
                world.resource_mut::<crate::app::Mobile>().dirty = true;
            }
            Event::Insets(values) => {
                let scale = world
                    .query::<&Window>()
                    .iter(world)
                    .next()
                    .map_or(1.0, Window::scale_factor);
                let [left, top, right, bottom] =
                    values.map(|value| px(12.0 + value.max(0) as f32 / scale));
                for mut node in world
                    .query_filtered::<&mut Node, With<crate::app::Shell>>()
                    .iter_mut(world)
                {
                    node.padding = UiRect {
                        left,
                        top,
                        right,
                        bottom,
                    };
                }
            }
            Event::Back => crate::app::queue_back(world),
            Event::Edit(id, value, committed) => {
                let key = EDITOR.lock().ok().and_then(|mut editor| {
                    if editor.as_ref().is_some_and(|(token, _, _)| *token == id) {
                        editor.take().map(|(_, key, scope)| (key, scope))
                    } else {
                        None
                    }
                });
                if !committed {
                    continue;
                }
                let Some((key, scope)) = key else { continue };
                if world.resource::<crate::app::Mobile>().scope_key() != scope {
                    continue;
                }
                let mut query = world.query::<(&crate::app::Input, &mut EditableText)>();
                for (input, mut text) in query.iter_mut(world) {
                    if input.key == key {
                        text.editor.set_text(&value);
                    }
                }
            }
        }
    }
}
