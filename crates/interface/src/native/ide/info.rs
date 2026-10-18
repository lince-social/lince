use bevy::prelude::*;

const TEXT: &str = include_str!("info.txt");

pub(super) fn panel(world: &mut World, controls: Entity, owner: Entity) {
    let panel = crate::native::sand_panel::column(world, owner);
    world.entity_mut(panel).insert(Node {
        display: Display::None,
        width: percent(100),
        height: px(280),
        min_height: px(0),
        flex_shrink: 0.0,
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(8)),
        overflow: Overflow::scroll_y(),
        ..default()
    });
    crate::native::scroll_sand::attach(world, panel);
    crate::native::sand_panel::button(world, controls, panel, "Info", Toggle);
}

#[derive(Clone)]
struct Toggle;

impl crate::native::actions::Action for Toggle {
    fn apply(&self, world: &mut World, panel: Entity) {
        let Some(mut node) = world.get_mut::<Node>(panel) else {
            return;
        };
        let showing = node.display == Display::None;
        node.display = if showing {
            Display::Flex
        } else {
            Display::None
        };
        if showing
            && !world.get::<Children>(panel).is_some_and(|children| {
                children.iter().any(|child| {
                    world
                        .get::<crate::native::description::Description>(child)
                        .is_some()
                })
            })
        {
            crate::native::description::spawn(
                world,
                panel,
                TEXT,
                crate::native::description::Context {
                    owner: panel,
                    source: crate::native::protein_area::Source::Local,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_button_reopens_the_same_scrollable_text_without_changing_the_editor() {
        let (mut world, _, owner, _directory, path) = super::super::tests::fixture("draft");
        let button = world
            .query::<(
                &crate::native::icons::Tooltip,
                &crate::native::actions::ActionButton,
            )>()
            .iter(&world)
            .find(|(tooltip, _)| tooltip.0 == "Info")
            .unwrap()
            .1
            .clone();
        let panel = button.target;
        assert_eq!(world.get::<ChildOf>(panel).unwrap().parent(), owner);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::None);
        assert!(
            world
                .get::<crate::native::scroll_sand::ScrollSand>(panel)
                .is_some()
        );
        button.actions.run(&mut world, panel);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::Flex);
        let description = world
            .get::<Children>(panel)
            .unwrap()
            .iter()
            .find(|child| {
                world
                    .get::<crate::native::description::Description>(*child)
                    .is_some()
            })
            .unwrap();
        assert_eq!(
            world
                .get::<crate::native::description::Description>(description)
                .unwrap()
                .source,
            TEXT
        );
        let count = world.entities().len();
        world.get_mut::<ScrollPosition>(panel).unwrap().y = 120.0;
        button.actions.run(&mut world, panel);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::None);
        button.actions.run(&mut world, panel);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::Flex);
        assert_eq!(world.entities().len(), count);
        assert_eq!(world.get::<ScrollPosition>(panel).unwrap().y, 120.0);
        assert_eq!(
            world.get::<super::super::Ide>(owner).unwrap().active,
            Some(path.clone())
        );
        assert_eq!(
            world.resource::<super::super::Documents>().0[&path]
                .buffer
                .text(),
            "draft"
        );
    }
}
