use super::*;
use crate::host::performance::{self, Count, Stage};

#[derive(Component, Default)]
struct Row {
    runs: crate::native::retained::Children<usize>,
}

#[derive(Component)]
struct RunLabel(Entity);

pub(super) fn render(world: &mut World, owner: Entity, frame: std::sync::Arc<vt::Frame>) {
    let _measurement = performance::Span::new(Stage::NativeTerminalPresentation);
    let terminal = world.get::<TerminalSand>(owner).unwrap();
    if terminal
        .frame
        .as_ref()
        .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, &frame))
    {
        return;
    }
    let (screen, cursor, mut lines, previous) = {
        let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
        (
            terminal.screen,
            terminal.cursor,
            std::mem::take(&mut terminal.lines),
            terminal.frame.take(),
        )
    };
    for line in lines.drain(frame.lines.len().min(lines.len())..) {
        world.despawn(line);
    }
    while lines.len() < frame.lines.len() {
        let y = lines.len();
        lines.push(
            world
                .spawn((
                    Row::default(),
                    ChildOf(screen),
                    Pickable::IGNORE,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(y as f32 * CELL_HEIGHT),
                        width: percent(100),
                        height: px(CELL_HEIGHT),
                        ..default()
                    },
                ))
                .id(),
        );
    }
    world.init_resource::<Mono>();
    let fonts = (
        world.resource::<Mono>().0.clone(),
        world.resource::<Mono>().1.clone(),
    );
    for (y, cells) in frame.lines.iter().enumerate() {
        if previous.as_ref().is_some_and(|previous| {
            previous.background == frame.background && previous.lines.get(y) == Some(cells)
        }) {
            performance::count(Count::TerminalRowsRetained);
            continue;
        }
        let line = lines[y];
        let mut row = world.entity_mut(line).take::<Row>().unwrap();
        let visible = runs(cells).into_iter().filter_map(|run| {
            let cell = &cells[run.start];
            if run.text.trim().is_empty() && cell.background == frame.background && !cell.underline
            {
                None
            } else {
                Some((run.start, (run, cell)))
            }
        });
        row.runs.reconcile(
            world,
            line,
            visible,
            |world, parent, (run, cell)| create_run(world, parent, run, cell, &fonts),
            |world, entity, (run, cell)| update_run(world, entity, run, cell, &fonts),
        );
        world.entity_mut(line).insert(row);
        performance::count(Count::TerminalRowsReconciled);
    }
    world
        .get_mut::<BackgroundColor>(screen)
        .unwrap()
        .set_if_neq(BackgroundColor(color(frame.background)));
    if previous
        .as_ref()
        .is_none_or(|previous| previous.cursor != frame.cursor)
        && let Some(mut node) = world.get_mut::<Node>(cursor)
    {
        let display = if frame.cursor.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if let Some((x, y)) = frame.cursor {
            let left = px(f32::from(x) * CELL_WIDTH);
            let top = px(f32::from(y) * CELL_HEIGHT);
            if node.left != left {
                node.left = left;
            }
            if node.top != top {
                node.top = top;
            }
        }
    }
    let mut terminal = world.get_mut::<TerminalSand>(owner).unwrap();
    terminal.lines = lines;
    terminal.frame = Some(frame);
}

fn color(rgb: [u8; 3]) -> Color {
    Color::srgb_u8(rgb[0], rgb[1], rgb[2])
}

fn node(run: &Run, cell: &vt::Cell) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(run.start as f32 * CELL_WIDTH),
        width: px(run.width as f32 * CELL_WIDTH),
        height: px(CELL_HEIGHT),
        border: UiRect {
            bottom: px(if cell.underline { 1 } else { 0 }),
            ..default()
        },
        ..default()
    }
}

fn font(cell: &vt::Cell, fonts: &(Handle<Font>, Handle<Font>)) -> TextFont {
    TextFont {
        font: if cell.bold { &fonts.1 } else { &fonts.0 }.clone().into(),
        font_size: FontSize::Px(15.0),
        ..default()
    }
}

fn create_run(
    world: &mut World,
    parent: Entity,
    run: &Run,
    cell: &vt::Cell,
    fonts: &(Handle<Font>, Handle<Font>),
) -> Entity {
    let root = world
        .spawn((
            ChildOf(parent),
            Pickable::IGNORE,
            node(run, cell),
            BackgroundColor(color(cell.background)),
            BorderColor::all(color(cell.foreground)),
        ))
        .id();
    let label = world
        .spawn((
            ChildOf(root),
            Pickable::IGNORE,
            Text::new(&run.text),
            font(cell, fonts),
            TextColor(color(cell.foreground)),
            TextLayout::no_wrap(),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        ))
        .id();
    world.entity_mut(root).insert(RunLabel(label));
    performance::count(Count::TerminalRunsCreated);
    root
}

fn update_run(
    world: &mut World,
    root: Entity,
    run: &Run,
    cell: &vt::Cell,
    fonts: &(Handle<Font>, Handle<Font>),
) {
    let label = world.get::<RunLabel>(root).unwrap().0;
    world
        .get_mut::<Node>(root)
        .unwrap()
        .set_if_neq(node(run, cell));
    world
        .get_mut::<BackgroundColor>(root)
        .unwrap()
        .set_if_neq(BackgroundColor(color(cell.background)));
    world
        .get_mut::<BorderColor>(root)
        .unwrap()
        .set_if_neq(BorderColor::all(color(cell.foreground)));
    world
        .get_mut::<TextFont>(label)
        .unwrap()
        .set_if_neq(font(cell, fonts));
    world
        .get_mut::<TextColor>(label)
        .unwrap()
        .set_if_neq(TextColor(color(cell.foreground)));
    let mut text = world.get_mut::<Text>(label).unwrap();
    if text.0 != run.text {
        text.0.clone_from(&run.text);
    }
    performance::count(Count::TerminalRunsRetained);
}
