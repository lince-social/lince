use super::*;
use crate::layout::desktop::{Geometry, Width, Workspace as Model};

fn width(dimension: Dimension) -> Width {
    match dimension { Dimension::Fraction([numerator, denominator]) => Width::Fraction(numerator, denominator), Dimension::Pixels(pixels) => Width::Pixels(f64::from(pixels)) }
}

pub(super) fn bounds(world: &World, root: Entity, workspace: u64) -> Option<Rectangle> {
    world.get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
        .and_then(|views| views.outputs.iter().find(|output| output.workspace == workspace))
        .and_then(|output| output.usable)
        .or_else(|| usable(world, root))
}

fn configure(model: &mut Model, id: PlacementId, properties: &Properties, bounds: Rectangle, scale: f64, base: crate::layout::Rules, current: Geometry) -> Result<Option<crate::layout::Rules>, String> {
    properties.validate()?;
    let item = model.items().iter().find(|item| item.id == id).ok_or("The application is no longer managed.")?;
    if item.kind != ItemKind::Application { return Err("Opening rules apply to application windows.".into()); }
    if let Some(floating) = properties.floating && floating != item.floating {
        model.toggle_floating(id, current.rectangle().ok_or("The application geometry is invalid.")?).map_err(|_| "This placement cannot change floating state.")?;
    }
    let item = model.items().iter().find(|item| item.id == id).unwrap();
    let tiled = model.mode() == WorkspaceLayout::Tiled && !item.floating;
    let mut proposed = None;
    if tiled {
        if let Some(dimension) = properties.width { model.set_item_width(id, width(dimension)).map_err(|_| "The tiled width is unavailable.")?; }
        if let Some(dimension) = properties.height { model.set_row_height(id, width(dimension)).map_err(|_| "The row height is unavailable.")?; }
    } else if properties.width.is_some() || properties.height.is_some() {
        if !scale.is_finite() || scale <= 0.0 { return Err("The current canvas scale is unavailable.".into()); }
        let mut geometry = item.free;
        let mut rules = base;
        for (axis, dimension) in [properties.width, properties.height].into_iter().enumerate() {
            if let Some(dimension) = dimension {
                let size = (dimension.resolve(bounds.size()[axis]) / scale).clamp(1.0, f64::from(crate::layout::LIMIT));
                geometry.origin[axis] += (geometry.size[axis] - size) * 0.5;
                geometry.size[axis] = size;
                rules.axes[axis].size = (size as f32).clamp(rules.axes[axis].min, rules.axes[axis].max);
                rules.axes[axis].sizing = crate::layout::Sizing::Fixed;
            }
        }
        model.configure(id, rules).map_err(|_| "The application size constraints are invalid.")?;
        model.set_free_geometry(id, geometry).map_err(|_| "The requested application geometry is outside canvas limits.")?;
        if let Some(Dimension::Fraction(ratio)) = properties.width {
            model.set_free_width(id, ratio, scale).map_err(|_| "The requested free width is unavailable.")?;
        } else if matches!(properties.width, Some(Dimension::Pixels(_))) {
            model.clear_free_width(id).map_err(|_| "The requested fixed width is unavailable.")?;
        }
        proposed = Some(rules);
    }
    if let Some(maximized) = properties.maximized {
        model.request_state(id, crate::host::WindowRequest { maximized: Some(maximized), ..Default::default() });
    }
    Ok(proposed)
}

pub fn apply_initial(world: &mut World, root: Entity, workspace: u64, entity: Entity, id: PlacementId, model: &mut Model) {
    let Some(decision) = world.get::<Decision>(entity).cloned() else { return; };
    let result = (|| {
        let bounds = bounds(world, root, workspace).ok_or("The output usable area is unavailable.")?;
        let scale = crate::native::output_views::canvas(world, root, workspace).zoom;
        let layout = world.get::<LayoutBox>(entity).ok_or("The application layout is unavailable.")?;
        let item = world.get::<CanvasItem>(entity).ok_or("The application geometry is unavailable.")?;
        let current = Geometry { origin: (item.position - item.size.as_dvec2() * 0.5).to_array(), size: item.size.as_dvec2().to_array() };
        let mut candidate = model.clone();
        let rules = configure(&mut candidate, id, &decision.properties, bounds, scale, layout.rules, current)?;
        if !candidate.valid() { return Err("Opening preferences produced an invalid layout.".to_owned()); }
        *model = candidate;
        if let Some(rules) = rules { world.get_mut::<LayoutBox>(entity).unwrap().rules = rules; }
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        world.get_mut::<Decision>(entity).unwrap().warning = Some(error);
    }
}

pub struct Prepared {
    transfer: transfers::Prepared,
    stamp: Stamp,
    root: Entity,
    canvas: String,
    target: Entity,
    application: ApplicationWindow,
    views: Option<crate::native::output_views::OutputViews>,
    view: CanvasView,
    destination: u64,
}

impl Prepared {
    pub fn commit(self, world: &mut World) -> Result<Option<transfers::Selection>, String> {
        if !self.stamp.valid(world)
            || world.get::<Workspaces>(self.root).is_none_or(|spaces| spaces.canvas_id != self.canvas)
            || world.get::<ApplicationWindow>(self.target).map(|window| (window.surface, window.content_size)) != Some((self.application.surface, self.application.content_size))
            || world.get_resource::<crate::native::output_views::OutputViews>() != self.views.as_ref()
            || crate::native::output_views::canvas(world, self.root, self.destination) != self.view
        { return Err("The opening rules, application or output changed; review and Reapply again.".into()); }
        self.transfer.commit(world)
    }
}

pub fn prepare_reapply(world: &mut World, root: Entity, source: u64, placement: PlacementId, destination: u64, decision: &Decision) -> Result<Prepared, String> {
    let stamp = decision.stamp.clone().filter(|stamp| stamp.valid(world)).ok_or("Opening rules changed or are unavailable; review and Reapply again.")?;
    let target = world.get::<Children>(root).and_then(|children| children.iter().find(|entity| {
        world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == placement.0)
            && world.get::<WorkspaceMember>(*entity).is_some_and(|member| member.0 == source)
            && world.get::<ApplicationWindow>(*entity).is_some()
            && world.get::<TransientParent>(*entity).is_none()
    })).ok_or("The captured application is no longer available.")?;
    let bounds = bounds(world, root, destination).ok_or("The destination output usable area is unavailable.")?;
    let scale = crate::native::output_views::canvas(world, root, destination).zoom;
    let item = world.get::<CanvasItem>(target).ok_or("The application geometry is unavailable.")?;
    let current = Geometry { origin: (item.position - item.size.as_dvec2() * 0.5).to_array(), size: item.size.as_dvec2().to_array() };
    let previous = world.get::<Workspaces>(root).and_then(|spaces| spaces.entries.iter().find(|space| space.id == destination)).ok_or("The destination workspace is unavailable.")?.desktop.clone();
    let prepared = transfers::prepare_captured(world, root, source, placement, destination, None)?;
    let transfer = prepared.configure_item(world, target, |model, id, rules| {
        let rules = configure(model, id, &decision.properties, bounds, scale, rules, current)?;
        if decision.properties.focus != Some(true) { model.preserve_selection(&previous); }
        Ok(rules)
    })?;
    let canvas = world.get::<Workspaces>(root).unwrap().canvas_id.clone();
    let application = *world.get::<ApplicationWindow>(target).unwrap();
    let views = world.get_resource::<crate::native::output_views::OutputViews>().cloned();
    let view = crate::native::output_views::canvas(world, root, destination);
    Ok(Prepared { transfer, stamp, root, canvas, target, application, views, view, destination })
}
