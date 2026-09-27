use bevy::{prelude::*, text::EditableText};
use jni::{
    JNIEnv, JavaVM,
    objects::{JClass, JObject, JString, JValue},
    sys::{jboolean, jlong},
};
use std::{
    collections::VecDeque,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
static EDITOR: Mutex<Option<(u64, String)>> = Mutex::new(None);
static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
static WAKE: OnceLock<lince_interface::wake::WakeSignal> = OnceLock::new();

enum Event {
    Edit(u64, String, bool),
    Back,
    Insets([i32; 4]),
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
) {
    if gesture.moved {
        return;
    }
    let Ok((input, text)) = inputs.get(event.entity) else {
        return;
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut editor) = EDITOR.lock() {
        *editor = Some((id, input.key.clone()));
    }
    let result = with_activity(|env, activity| {
        let title = env.new_string(&input.title)?;
        let value = env.new_string(text.value().to_string())?;
        env.call_method(
            activity,
            "edit",
            "(JLjava/lang/String;Ljava/lang/String;Z)V",
            &[
                JValue::Long(id as i64),
                JValue::Object(&title),
                JValue::Object(&value),
                JValue::Bool(text.allow_newlines.into()),
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

pub fn receive(world: &mut World) {
    let events: Vec<_> = EVENTS
        .lock()
        .map(|mut events| events.drain(..).collect())
        .unwrap_or_default();
    for event in events {
        match event {
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
                    if editor.as_ref().is_some_and(|(token, _)| *token == id) {
                        editor.take().map(|(_, key)| key)
                    } else {
                        None
                    }
                });
                if !committed {
                    continue;
                }
                let Some(key) = key else { continue };
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
