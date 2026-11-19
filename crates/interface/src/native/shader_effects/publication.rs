use super::*;

#[derive(Default)]
struct Change {
    load: Option<Extension>,
    loaded: Option<bool>,
    message: Option<String>,
    quantity: Option<Option<String>>,
    follow_quantity: Option<Option<String>>,
    rebuild: bool,
}

fn prepare(world: &World, owner: Entity) -> Change {
    let editor = world.get::<Editor>(owner).unwrap();
    let Target::Record(binding) = &editor.target else {
        return Change::default();
    };
    let Some(feed) = world.resource::<Feeds>().get(&binding.source, &binding.uid) else {
        return Change::default();
    };
    let Some(row) = &feed.row else {
        return Change {
            loaded: editor.loaded.then_some(false),
            message: (editor.message != feed.error).then(|| feed.error.clone()),
            follow_quantity: (!editor.custom_quantity).then_some(None),
            ..default()
        };
    };
    let extension = world
        .resource::<feeds::CheckedRows>()
        .extension(row)
        .unwrap_or_else(|| feeds::default_extension());
    let revision_changed = extension.revision != editor.revision;
    let dirty = (!editor.loaded || revision_changed) && editor.catalog.dirty(&editor.effect);
    let load = (!editor.loaded || revision_changed) && !dirty && editor.pending.is_none();
    let restore = !load && !editor.loaded && !revision_changed && extension == &editor.catalog.base;
    let message = if restore {
        Some("Draft restored")
    } else if !load && !editor.loaded {
        Some("The Record loaded while the draft was being edited. Reload working before saving.")
    } else if !load && revision_changed && dirty {
        Some("Record effect changed elsewhere. Reload working before saving.")
    } else {
        None
    };
    let quantity_changed = !editor.custom_quantity && editor.quantity != row.quantity;
    Change {
        load: load.then(|| extension.clone()),
        loaded: ((load || restore) && !editor.loaded).then_some(true),
        message: message
            .filter(|message| editor.message != *message)
            .map(str::to_owned),
        quantity: ((load || restore) && quantity_changed).then(|| row.quantity.clone()),
        follow_quantity: (!load && !restore && editor.loaded && quantity_changed)
            .then(|| row.quantity.clone()),
        rebuild: load || restore,
    }
}

pub(super) fn update_editor(world: &mut World, owner: Entity) {
    let _publication = nucleus::performance::Span::new("shader-editor-feed-publication");
    let change = prepare(world, owner);
    if let Some(extension) = change.load {
        nucleus::performance::add("shader-editor-catalog-loads", 1);
        catalog::load(world, owner, extension, false);
    }
    {
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        if let Some(loaded) = change.loaded {
            editor.loaded = loaded;
        }
        if let Some(message) = change.message {
            editor.message = message;
        }
        if let Some(quantity) = change.quantity {
            editor.quantity = quantity;
        }
        if change.rebuild && !editor.rebuild {
            editor.rebuild = true;
        }
    }
    if let Some(quantity) = change.follow_quantity {
        set_record_quantity(world, owner, quantity);
    }
    let (status, message, rebuild) = {
        let editor = world.get::<Editor>(owner).unwrap();
        (
            editor.status,
            world
                .get::<Text>(editor.status)
                .is_some_and(|text| text.0 != editor.message)
                .then(|| editor.message.clone()),
            editor.rebuild,
        )
    };
    if let Some(message) = message {
        world.get_mut::<Text>(status).unwrap().0 = message;
        nucleus::performance::add("shader-editor-status-copies", 1);
    }
    if world
        .get::<Editor>(owner)
        .unwrap()
        .pending
        .as_ref()
        .is_some_and(|pending| pending.1.elapsed() > Duration::from_secs(20))
    {
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        editor.message = "Save confirmation timed out. Reload working before retrying.".into();
        editor.pending = None;
        editor.catalog.pending = None;
    }
    if rebuild {
        render_editor(world, owner);
    }
}
