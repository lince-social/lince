use super::*;
use lince_interface::host::{DesktopAction, ItemId, OutputId, WorkspaceLayout};
use std::sync::Arc;

const TEST: &str = "presentation::accessibility::platform::dispatcher::outputs::actual_independent_output_draws_gate_bus_text_and_discarded_coalesced_late_and_hidden_owner_work_cannot_restore_stale_semantics";
const FIRST: &str = "First output editor";
const SECOND: &str = "Companion output editor";

#[derive(Component, Default)]
struct Steps(usize);

#[derive(Clone)]
struct Stamp { field: Entity, marker: Entity, texts: [&'static str; 3], colors: [Color; 3] }

impl native::actions::Action for Stamp {
    fn apply(&self, world: &mut World, target: Entity) {
        let mut steps = world.get_mut::<Steps>(target).unwrap();
        let index = steps.0.min(2); steps.0 += 1;
        let mut field = world.get_mut::<EditableText>(self.field).unwrap();
        field.queue_edit(bevy::text::TextEdit::SelectAll);
        field.queue_edit(bevy::text::TextEdit::Insert(self.texts[index].into()));
        world.get_mut::<BackgroundColor>(self.marker).unwrap().0 = self.colors[index];
    }
}

#[derive(Clone, Copy)]
struct Card { owner: Entity, placement: [u8; 16], field: Entity }

fn card(world: &mut World, root: Entity, workspace: u64, name: &'static str, label: &'static str, button: &'static str,
    initial: &'static str, texts: [&'static str; 3], colors: [Color; 3]) -> Card
{
    let owner = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, initial, bevy::math::DVec2::ZERO);
    world.entity_mut(owner).insert((Name::new(name), Steps::default()));
    let field = world.get::<native::sand_store::StoredSand>(owner).unwrap().content.unwrap();
    if world.get::<native::layout::LayoutBox>(owner).is_none() {
        let size = world.get::<native::canvas::CanvasItem>(owner).unwrap().size;
        world.entity_mut(owner).insert(native::layout::LayoutBox::new(size));
    }
    world.get_mut::<bevy::a11y::AccessibilityNode>(field).unwrap().set_label(label);
    let marker = world.spawn((Node { position_type: PositionType::Absolute, left: px(16), top: px(70), width: px(32), height: px(32), ..default() },
        BackgroundColor(Color::srgb(0.1, 0.3, 0.5)), Pickable::IGNORE, ChildOf(owner))).id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Button); accessible.set_label(button);
    let button = world.spawn((native::sand::button(0), Node { position_type: PositionType::Absolute, right: px(4), top: px(4), width: px(100), height: px(28), ..default() },
        bevy::a11y::AccessibilityNode(accessible), ActionButton::new(owner, ActionSequence::default().then(Stamp { field, marker, texts, colors })), ChildOf(owner))).id();
    world.spawn((Text::new("Update"), TextFont::default(), Pickable::IGNORE, ChildOf(button)));
    Card { owner, placement: world.get::<native::layout::LayoutBox>(owner).unwrap().id, field }
}

type Drawn = (Arc<crate::runtime::Completed>, Arc<renderer::Presented>, Duration);

fn colored(pixels: &[u8], color: [u8; 3]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| (0..3).all(|axis| pixel[axis].abs_diff(color[axis]) <= 3)).count()
}

fn pending_color(fixture: &mut Fixture, output: OutputId, color: [u8; 3]) -> Result<Drawn> {
    wait(|| {
        fixture.collect();
        let Some(frame) = fixture.desktop.pending.as_ref().filter(|frame| frame.frame.ready().unwrap()).cloned() else { return Ok(None); };
        let (presented, pixels, time) = fixture.draw_output(&frame, output);
        Ok((colored(&pixels, color) > 200).then_some((frame, presented, time)))
    })
}

fn text(provider: &Provider, object: &Object) -> Result<String> {
    Ok(proxy(&provider.bus, object, "org.a11y.atspi.Text")?.call("GetText", &(0_i32, -1_i32))?)
}

fn update(provider: &Provider, object: &Object) -> Result<()> {
    assert!(proxy(&provider.bus, object, "org.a11y.atspi.Action")?.call::<_, _, bool>("DoAction", &(0_i32,))?);
    Ok(())
}

fn ready_action(fixture: &mut Fixture, provider: &Provider, label: &str) -> Result<Object> {
    fixture.until(|_| {
        let Some(object) = named(&provider.bus, label)? else { return Ok(None); };
        let count = proxy(&provider.bus, &object, "org.a11y.atspi.Action")?.get_property::<i32>("NActions").unwrap_or(0);
        Ok((count > 0).then_some(object))
    })
}

fn select(fixture: &mut Fixture, workspace: u64, placement: [u8; 16]) {
    fixture.desktop.worker.input(Input::Action(DesktopAction::SelectPlacement { workspace, item: Some(placement) })).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.focused == Some(ItemId::Native(placement)))).map(|_| ()))).unwrap();
}

fn child(root: PathBuf) -> Result<()> {
    let provider = Provider::new(&root)?;
    provider.status.set_property("ScreenReaderEnabled", true)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?;
    let _entered = runtime.enter();
    let mut fixture = Fixture::new(&runtime, &root.join("profile"));
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.input.control.context.is_some()).map(|_| ())))?;
    let monitors = [crate::server::Monitor { id: OutputId(1), name: "Access-A".into(), origin: [0, 0], size: [500, 720] },
        crate::server::Monitor { id: OutputId(2), name: "Access-B".into(), origin: [500, 0], size: [500, 720] }];
    fixture.desktop.server.configure_outputs(&monitors)?; fixture.desktop.server.registered_output_catalog()?; fixture.desktop.invalidate_outputs()?;
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)).map(|_| ())))?;
    let geometry = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap();
    let a = geometry.outputs.iter().find(|output| output.id == OutputId(1)).unwrap().workspace;
    let b = geometry.outputs.iter().find(|output| output.id == OutputId(2)).unwrap().workspace;
    let (canvas_root, first, second) = fixture.read(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<Workspaces>>().single(world).unwrap();
        let ids = world.get::<Workspaces>(root).unwrap().entries.iter().map(|space| space.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
        for workspace in &mut world.get_mut::<Workspaces>(root).unwrap().entries { workspace.desktop.set_mode(WorkspaceLayout::Tiled); }
        let first = card(world, root, a, "First note", FIRST, "Update first output", "a", ["aé界", "second A", "final A"],
            [Color::srgb(1.0, 0.0, 0.0), Color::srgb(1.0, 1.0, 0.0), Color::srgb(1.0, 0.5, 0.0)]);
        let second = card(world, root, b, "Companion note", SECOND, "Update companion output", "b", ["bé界", "second B", "final B"],
            [Color::srgb(0.0, 1.0, 1.0), Color::srgb(0.4, 0.0, 1.0), Color::srgb(0.5, 0.5, 0.5)]);
        (root, first, second)
    });
    let first_field = fixture.until(|_| Ok(named(&provider.bus, FIRST)?))?;
    let second_field = fixture.until(|_| Ok(named(&provider.bus, SECOND)?))?;
    let first_button = ready_action(&mut fixture, &provider, "Update first output")?;
    let second_button = ready_action(&mut fixture, &provider, "Update companion output")?;
    assert_eq!(text(&provider, &first_field)?, "a"); assert_eq!(text(&provider, &second_field)?, "b");
    update(&provider, &first_button)?;
    let first_draw = pending_color(&mut fixture, OutputId(1), [255, 0, 0])?;
    assert_eq!(text(&provider, &first_field)?, "a"); assert_eq!(text(&provider, &second_field)?, "b");
    update(&provider, &second_button)?;
    let companion_draw = pending_color(&mut fixture, OutputId(2), [0, 255, 255])?;
    let companion_sequence = companion_draw.0.sequence;
    assert!(companion_sequence > first_draw.0.sequence);
    fixture.present_output(OutputId(2), companion_draw.0.clone(), companion_draw.1.clone(), companion_draw.2);
    wait(|| {
        fixture.collect();
        Ok((text(&provider, &second_field)? == "bé界" && text(&provider, &first_field)? == "a").then_some(()))
    })?;
    fixture.present_output(OutputId(1), first_draw.0.clone(), first_draw.1.clone(), first_draw.2);
    assert_eq!(fixture.desktop.current.as_ref().unwrap().sequence, companion_sequence);
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.output, Some(OutputId(2)));
    wait(|| { fixture.collect(); Ok((text(&provider, &first_field)? == "aé界" && text(&provider, &second_field)? == "bé界").then_some(())) })?;
    update(&provider, &first_button)?;
    let discarded = pending_color(&mut fixture, OutputId(1), [255, 255, 0])?;
    assert_eq!(text(&provider, &first_field)?, "aé界");
    update(&provider, &first_button)?;
    let final_draw = pending_color(&mut fixture, OutputId(1), [255, 128, 0])?;
    assert!(final_draw.0.sequence > discarded.0.sequence);
    let final_sequence = final_draw.0.sequence;
    fixture.present_output(OutputId(1), final_draw.0.clone(), final_draw.1.clone(), final_draw.2);
    wait(|| { fixture.collect(); Ok((text(&provider, &first_field)? == "final A").then_some(())) })?;
    fixture.present_output(OutputId(1), discarded.0.clone(), discarded.1.clone(), discarded.2);
    assert_eq!(fixture.desktop.current.as_ref().unwrap().sequence, final_sequence);
    assert_eq!(text(&provider, &first_field)?, "final A"); assert_eq!(text(&provider, &second_field)?, "bé界");
    select(&mut fixture, a, first.placement);
    let serial = fixture.desktop.worker.input(Input::Action(DesktopAction::MarkScratch)).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ())))?;
    let selection = fixture.read(move |canvas| canvas.world_mut().get::<EditableText>(first.field).unwrap().editor.raw_selection().text_range());
    let (_, release) = fixture.pause();
    fixture.desktop.worker.input(Input::Action(DesktopAction::HideScratch)).unwrap();
    let serial = fixture.fill_from(1);
    assert!(proxy(&provider.bus, &first_field, "org.a11y.atspi.Text")?.call::<_, _, bool>("SetCaretOffset", &(0_i32,))?);
    release.send(()).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ())))?;
    fixture.until(|_| Ok(named(&provider.bus, FIRST)?.is_none().then_some(())))?;
    assert_eq!(fixture.read(move |canvas| canvas.world_mut().get::<EditableText>(first.field).unwrap().editor.raw_selection().text_range()), selection);
    assert_eq!(text(&provider, &second_field)?, "bé界");
    let serial = fixture.desktop.worker.input(Input::Action(DesktopAction::ShowScratch)).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.input_serial >= serial).map(|_| ())))?;
    fixture.desktop.worker.input(Input::Action(DesktopAction::UnmarkScratch)).unwrap();
    fixture.until(|_| Ok(named(&provider.bus, FIRST)?))?;
    let peer = fixture.read(move |canvas| card(canvas.world_mut(), canvas_root, a, "Grouped siblingnote", "Grouped sibling editor", "Update sibling output", "c",
        ["c", "cc", "ccc"], [Color::srgb(0.2, 0.4, 0.6); 3]));
    fixture.until(|_| Ok(named(&provider.bus, "Grouped sibling editor")?))?;
    select(&mut fixture, a, first.placement);
    fixture.desktop.worker.input(Input::Action(DesktopAction::CreateCompactGroup)).unwrap();
    let choice = ready_action(&mut fixture, &provider, "1 · Grouped siblingnote")?;
    update(&provider, &choice)?;
    let create = ready_action(&mut fixture, &provider, "Create selected group")?;
    update(&provider, &create)?;
    fixture.until(|_| Ok(named(&provider.bus, "Grouped sibling editor")?.is_none().then_some(())))?;
    fixture.desktop.worker.input(Input::Action(DesktopAction::NextCompactMember)).unwrap();
    fixture.until(|_| Ok(named(&provider.bus, FIRST)?.is_none().then_some(())))?;
    fixture.until(|_| Ok(named(&provider.bus, "Grouped sibling editor")?))?;
    assert_eq!(fixture.read(move |canvas| canvas.world_mut().get::<Steps>(first.owner).unwrap().0), 3);
    assert_eq!(fixture.read(move |canvas| canvas.world_mut().get::<Steps>(peer.owner).unwrap().0), 0);
    assert_eq!(text(&provider, &second_field)?, "bé界");
    fixture.desktop.worker.input(Input::Action(DesktopAction::DissolveCompactGroup)).unwrap();
    fixture.until(|_| Ok(named(&provider.bus, FIRST)?))?;
    let (_, release) = fixture.pause();
    fixture.fill();
    update(&provider, &second_button)?;
    fixture.desktop.server.configure_outputs(&monitors[..1])?; fixture.desktop.server.registered_output_catalog()?; fixture.desktop.invalidate_outputs()?;
    fixture.present_output(OutputId(2), companion_draw.0.clone(), companion_draw.1.clone(), companion_draw.2);
    assert!(fixture.desktop.current.is_none());
    release.send(()).unwrap();
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 1)).map(|_| ())))?;
    fixture.until(|_| Ok(named(&provider.bus, SECOND)?.is_none().then_some(())))?;
    assert_eq!(fixture.read(move |canvas| canvas.world_mut().get::<Steps>(second.owner).unwrap().0), 1);
    fixture.desktop.server.configure_outputs(&monitors)?; fixture.desktop.server.registered_output_catalog()?; fixture.desktop.invalidate_outputs()?;
    fixture.present_output(OutputId(2), companion_draw.0.clone(), companion_draw.1.clone(), renderer::capture::monotonic());
    assert!(fixture.desktop.current.is_none());
    fixture.until(|fixture| Ok(fixture.desktop.current.as_ref().filter(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)).map(|_| ())))?;
    let recovered = fixture.until(|_| Ok(named(&provider.bus, SECOND)?))?;
    assert_eq!(text(&provider, &recovered)?, "bé界");
    assert_eq!(fixture.read(move |canvas| canvas.world_mut().get::<Steps>(second.owner).unwrap().0), 1);
    update(&provider, &ready_action(&mut fixture, &provider, "Update companion output")?)?;
    fixture.until(|_| Ok((text(&provider, &recovered)? == "second B").then_some(())))?;
    fixture.desktop.worker.stop()?;
    wait(|| Ok(named(&provider.bus, "Lince Park")?.is_none().then_some(())))?;
    Ok(())
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn actual_independent_output_draws_gate_bus_text_and_discarded_coalesced_late_and_hidden_owner_work_cannot_restore_stale_semantics() -> Result<()> { run(TEST, child) }
