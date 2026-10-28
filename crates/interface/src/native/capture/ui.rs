use super::*;
use crate::native::full_record::window::Metadata;

#[derive(Clone)]
pub(super) enum Target {
    Native { entity: Entity, placement: [u8; 16] },
    Window { placement: [u8; 16], uid: String },
}

#[derive(Component)]
pub(super) struct Page {
    pub(super) root: Entity,
    pub(super) canvas: String,
    workspace: u64,
    pub(super) target: Target,
    pub(super) origin: Option<bool>,
    pub(super) draft: bool,
    pub(super) pending: Option<u64>,
    pub(super) revision: u64,
    pub(super) message: String,
    toggle: (Entity, Entity),
    save: Entity,
    cancel: Entity,
    status: Entity,
}

#[derive(Clone, Copy)]
enum Change { Toggle, Save, Cancel }

pub(super) fn eligible(world: &World, page: &Page) -> bool {
    world.get::<Workspaces>(page.root).is_some_and(|spaces| spaces.canvas_id == page.canvas && spaces.active == page.workspace)
}

fn value(world: &World, target: &Target) -> Option<bool> {
    match target {
        Target::Native { entity, placement } => world.get::<LayoutBox>(*entity).filter(|layout| layout.id == *placement).map(|layout| layout.capture_excluded),
        Target::Window { placement, uid } => world.get_resource::<Windows>().filter(|windows| windows.available)?.policies.get(placement).filter(|policy| policy.uid == *uid).map(|policy| policy.excluded),
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(page) = world.get::<Page>(entity) else { return; };
        if page.pending.is_some() || !eligible(world, page) || crate::native::shortcuts::keyboard_owned(world)
            || !crate::native::shortcuts::input_eligible(world, page.root, &page.canvas, entity)
        { return; }
        match self {
            Self::Toggle => {
                let mut page = world.get_mut::<Page>(entity).unwrap();
                page.draft = !page.draft;
                page.message.clear();
            }
            Self::Cancel => {
                let current = value(world, &page.target);
                let revision = world.resource::<Windows>().revision;
                let mut page = world.get_mut::<Page>(entity).unwrap();
                page.origin = current;
                page.draft = current.unwrap_or_default();
                page.revision = revision;
                page.message.clear();
            }
            Self::Save => {
                let target = page.target.clone();
                let draft = page.draft;
                let root = page.root;
                let revision = page.revision;
                if page.origin.is_none() || value(world, &target) != page.origin {
                    world.get_mut::<Page>(entity).unwrap().message = "The original setting changed. Cancel to refresh; your draft is retained.".into();
                    return;
                }
                match target {
                    Target::Native { entity: target, placement } => {
                        if !crate::native::canvas_item::eligible(world, root, target) || world.get::<LayoutBox>(target).is_none_or(|layout| layout.id != placement) {
                            world.get_mut::<Page>(entity).unwrap().message = "The original item is unavailable.".into();
                            return;
                        }
                        if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
                        world.get_mut::<LayoutBox>(target).unwrap().capture_excluded = draft;
                        crate::native::layout::records::remember(world, target);
                        let mut page = world.get_mut::<Page>(entity).unwrap();
                        page.origin = Some(draft);
                        page.message = "Applied. This setting is saved with the workspace; workspace status reports save failures.".into();
                    }
                    Target::Window { placement, uid } => {
                        let mut windows = world.resource_mut::<Windows>();
                        if !windows.available || windows.revision != revision || windows.requests.len() >= 16 {
                            drop(windows);
                            world.get_mut::<Page>(entity).unwrap().message = "Window settings changed or the queue is busy. Cancel to refresh, then retry.".into();
                            return;
                        }
                        windows.next = windows.next.checked_add(1).expect("capture settings request");
                        let request = windows.next;
                        windows.requests.push_back(Edit { request, owner: root, placement, uid, revision, excluded: draft, page: entity });
                        drop(windows);
                        let mut page = world.get_mut::<Page>(entity).unwrap();
                        page.pending = Some(request);
                        page.message = "Saving capture exclusion…".into();
                        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.ring(); }
                    }
                }
            }
        }
    }
}

fn panel(world: &mut World, parent: Entity, root: Entity, target: Target) {
    let Some(spaces) = world.get::<Workspaces>(root) else { return; };
    let canvas = spaces.canvas_id.clone();
    let workspace = spaces.active;
    let origin = value(world, &target);
    let revision = world.resource::<Windows>().revision;
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(parent), Name::new("capture.exclusion"))).id();
    crate::native::edit_mode::label(world, page, "External capture", 18.0);
    crate::native::edit_mode::label(world, page, "Exclude this item and its owned content from screenshots and sharing in Park. Your own view remains visible. In Sandbox, the host desktop controls capture.", 14.0);
    let toggle = input_settings::button(world, page, page, "", Change::Toggle);
    let save = input_settings::button(world, page, page, if matches!(target, Target::Native { .. }) { "Apply exclusion" } else { "Save exclusion" }, Change::Save).0;
    let cancel = input_settings::button(world, page, page, "Cancel / refresh", Change::Cancel).0;
    let status = input_settings::feedback(world, page);
    world.entity_mut(page).insert(Page { root, canvas, workspace, target, origin, draft: origin.unwrap_or_default(), pending: None, revision, message: String::new(), toggle, save, cancel, status });
}

pub(crate) fn item_panel(world: &mut World, root: Entity, parent: Entity, entity: Entity) {
    if world.get::<crate::native::layout::desktop::ApplicationWindow>(entity).is_some() { return; }
    let Some(layout) = world.get::<LayoutBox>(entity).copied() else { return; };
    panel(world, parent, root, Target::Native { entity, placement: layout.id });
}

pub(crate) fn window_panel(world: &mut World, parent: Entity, binding: &crate::native::protein_area::RecordBinding) {
    if binding.source != crate::native::protein_area::Source::Local { return; }
    let Some(metadata) = world.get::<Metadata>(binding.area).filter(|metadata| metadata.uid == binding.uid) else { return; };
    let target = Target::Window { placement: metadata.placement, uid: metadata.uid.clone() };
    let Some(root) = crate::native::layout::panel::root(world, binding.area) else { return; };
    panel(world, parent, root, target);
}

pub(super) fn refresh(world: &mut World) {
    let pages: Vec<_> = world.query_filtered::<Entity, With<Page>>().iter(world).collect();
    for entity in pages {
        let page = world.get::<Page>(entity).unwrap();
        let current = value(world, &page.target);
        let ready = eligible(world, page) && current.is_some();
        let pending = page.pending.is_some();
        let changed = page.origin != current;
        let pristine = page.origin.is_none() && page.message.is_empty() && !pending;
        let revision = world.resource::<Windows>().revision;
        if pristine && current.is_some() {
            let mut page = world.get_mut::<Page>(entity).unwrap();
            page.origin = current;
            page.draft = current.unwrap();
            page.revision = revision;
        }
        let page = world.get::<Page>(entity).unwrap();
        let (toggle_label, draft) = (page.toggle.1, page.draft);
        input_settings::text(world, toggle_label, if draft { "Excluded from external capture" } else { "Allowed in external capture" });
        let page = world.get::<Page>(entity).unwrap();
        let (toggle, save, cancel, status, draft, origin) = (page.toggle.0, page.save, page.cancel, page.status, page.draft, page.origin);
        let message = if !ready { "The original item/profile is unavailable. Global capture is enforced by Park; Sandbox cannot control its parent compositor.".into() }
            else if changed && !pristine { "The original setting changed. Cancel to refresh; your draft is retained.".into() }
            else { page.message.clone() };
        input_settings::disable(world, toggle, !ready || pending);
        input_settings::disable(world, save, !ready || pending || changed && !pristine || origin == Some(draft));
        input_settings::disable(world, cancel, pending);
        input_settings::text(world, status, &message);
    }
}

#[cfg(test)]
mod tests;
