use crate::app::Mobile;
use bevy::prelude::*;
use nucleus::message::MessagePart;

pub fn prepare(name: String, mime_type: String, bytes: &[u8]) -> Result<MessagePart, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    if bytes.len() > nucleus::message::MAX_CONTENT_BYTES {
        return Err("Choose a file smaller than 4 MiB".into());
    }
    let part = MessagePart::Attachment {
        name,
        mime_type,
        data: STANDARD.encode(bytes),
    };
    nucleus::message::validate(std::slice::from_ref(&part))?;
    Ok(part)
}

pub fn selected(
    world: &mut World,
    scope: &str,
    thread: String,
    result: Result<MessagePart, String>,
) {
    if world.resource::<Mobile>().scope_key() != scope {
        return;
    }
    let result = result.and_then(|part| {
        let mut attachments = world.resource::<Mobile>().attachments.clone();
        let parts = attachments.entry(thread).or_default();
        parts.push(part);
        nucleus::message::validate(parts)?;
        if serde_json::to_vec(&attachments)
            .map_err(|error| error.to_string())?
            .len()
            > 8 * 1024 * 1024
        {
            return Err("Send or remove some attached files before adding more".into());
        }
        world.resource_mut::<Mobile>().attachments = attachments;
        Ok(())
    });
    let mut state = world.resource_mut::<Mobile>();
    state.status = result.map_or_else(
        |error| error,
        |_| "File attached to the draft. Press Send to share it.".into(),
    );
    state.dirty = true;
    state.request_save();
}

pub fn choose(world: &World, thread: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        crate::android::choose_file(world.resource::<Mobile>().scope_key(), thread.into())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (world, thread);
        Err("Choose attachments using the Android document picker".into())
    }
}

pub fn save(part: &serde_json::Value) -> Result<(), String> {
    let MessagePart::Attachment {
        name,
        mime_type,
        data,
    } = serde_json::from_value(part.clone()).map_err(|error| error.to_string())?
    else {
        return Err("This item is not an attachment".into());
    };
    let bytes = nucleus::message::decode(&data)?;
    #[cfg(target_os = "android")]
    {
        crate::android::save_file(&name, &mime_type, &bytes)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (name, mime_type, bytes);
        Err("Save attachments using the Android document picker".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_picker_results_cannot_enter_a_different_persons_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::new();
        world.insert_resource(Mobile::new(directory.path().into()));
        let scope = world.resource::<Mobile>().scope_key();
        world.resource_mut::<Mobile>().identity =
            crate::session::Identity::Person("another-person".into());
        let part = prepare("private.txt".into(), "text/plain".into(), b"private").unwrap();
        selected(&mut world, &scope, "thread".into(), Ok(part.clone()));
        assert!(world.resource::<Mobile>().attachments.is_empty());
        let scope = world.resource::<Mobile>().scope_key();
        selected(&mut world, &scope, "thread".into(), Ok(part));
        assert_eq!(world.resource::<Mobile>().attachments["thread"].len(), 1);
    }
}
