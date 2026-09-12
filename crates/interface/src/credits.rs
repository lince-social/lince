pub const BEVY_LICENSE: &str = include_str!("../licenses/bevy-MIT.txt");
pub const LATO_LICENSE: &str = include_str!("../../../assets/fonts/Lato/OFL.txt");
pub const LUCIDE_LICENSE: &str = include_str!("../../../assets/icons/lucide/LICENSE");
pub const AVIAN_LICENSE: &str = include_str!("../licenses/avian-MIT.txt");

pub struct Attribution {
    pub name: &'static str,
    pub author: &'static str,
    pub license: &'static str,
}

pub const ATTRIBUTIONS: &[Attribution] = &[
    Attribution {
        name: "Avian physics",
        author: "Jondolf and Avian contributors",
        license: AVIAN_LICENSE,
    },
    Attribution {
        name: "Lucide icons",
        author: "Lucide and Feather contributors",
        license: LUCIDE_LICENSE,
    },
    Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: BEVY_LICENSE,
    },
    Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: LATO_LICENSE,
    },
];

use crate::actions::{Action, ActionButton};
use bevy::{a11y::AccessibilityNode, prelude::*};

#[derive(Component)]
pub struct LicenseAccordion {
    pub body: Entity,
    indicator: Entity,
}

pub struct ToggleLicense;

impl Action for ToggleLicense {
    fn connections(&self, world: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        world
            .get::<LicenseAccordion>(target)
            .map(|accordion| crate::inspection::Connection {
                target: accordion.body,
                name: "License Clicked Toggle Text".into(),
            })
            .into_iter()
            .collect()
    }

    fn apply(&self, world: &mut World, target: Entity) {
        let Some(accordion) = world.get::<LicenseAccordion>(target) else {
            return;
        };
        let (body, indicator) = (accordion.body, accordion.indicator);
        if world.get::<ChildOf>(target).map(ChildOf::parent)
            != world.get::<ChildOf>(body).map(ChildOf::parent)
        {
            return;
        }
        let Some(mut node) = world.get_mut::<Node>(body) else {
            return;
        };
        let expanded = node.display == Display::None;
        node.display = if expanded {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(mut text) = world.get_mut::<Text>(indicator) {
            text.0 = if expanded { "−" } else { "+" }.into();
        }
        if let Some(mut accessibility) = world.get_mut::<AccessibilityNode>(target) {
            accessibility.set_expanded(expanded);
        }
    }
}

pub(crate) fn render(world: &mut World, panel: Entity) {
    crate::edit_mode::label(world, panel, "Licenses and credits", 22.0);
    for credit in ATTRIBUTIONS {
        let section = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexStart,
                    flex_shrink: 0.0,
                    row_gap: px(8),
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        let button = world
            .spawn((
                crate::sand::Square,
                crate::sand::button(0),
                Node {
                    width: percent(100),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    padding: UiRect::all(px(10)),
                    border: UiRect::all(px(crate::sand::BUTTON_BORDER_WIDTH)),
                    flex_shrink: 0.0,
                    ..default()
                },
                crate::token_style::background(crate::tokens::Token::Surface),
                crate::token_style::border(crate::tokens::Token::Accent),
                ChildOf(section),
            ))
            .id();
        crate::edit_mode::label(world, button, credit.name, 16.0);
        let indicator = crate::edit_mode::label(world, button, "+", 16.0);
        let extra = if credit.name == "Lucide icons" {
            include_str!("../../../assets/icons/lucide/CREDITS.txt")
        } else {
            ""
        };
        let body = crate::edit_mode::label(
            world,
            section,
            &format!("{}\n\n{}{}", credit.author, extra, credit.license),
            14.0,
        );
        world.entity_mut(body).insert(Node {
            display: Display::None,
            width: percent(100),
            min_width: px(0),
            padding: UiRect::axes(px(8), px(4)),
            flex_shrink: 0.0,
            ..default()
        });
        world.entity_mut(button).insert((
            LicenseAccordion { body, indicator },
            ActionButton::new(button, crate::actions![ToggleLicense]),
        ));
        if let Some(mut accessibility) = world.get_mut::<AccessibilityNode>(button) {
            accessibility.set_label(credit.name);
            accessibility.set_expanded(false);
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn dependency_accordions_keep_full_licenses_and_toggle_independently() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        let panel = world.spawn(Node::default()).id();
        render(&mut world, panel);
        let buttons: Vec<_> = world
            .query::<(Entity, &LicenseAccordion)>()
            .iter(&world)
            .map(|(button, accordion)| (button, accordion.body))
            .collect();
        assert_eq!(buttons.len(), ATTRIBUTIONS.len());
        for ((_, body), credit) in buttons.iter().zip(ATTRIBUTIONS) {
            assert_eq!(world.get::<Node>(*body).unwrap().display, Display::None);
            let text = &world.get::<Text>(*body).unwrap().0;
            assert!(text.contains(credit.author));
            assert!(text.contains(credit.license));
        }
        for (button, body) in &buttons[..2] {
            ToggleLicense.apply(&mut world, *button);
            assert_eq!(world.get::<Node>(*body).unwrap().display, Display::Flex);
            assert_eq!(
                world
                    .get::<AccessibilityNode>(*button)
                    .unwrap()
                    .is_expanded(),
                Some(true)
            );
        }
        ToggleLicense.apply(&mut world, buttons[0].0);
        assert_eq!(
            world.get::<Node>(buttons[0].1).unwrap().display,
            Display::None
        );
        assert_eq!(
            world.get::<Node>(buttons[1].1).unwrap().display,
            Display::Flex
        );
        assert_eq!(
            world.get::<LicenseAccordion>(buttons[0].0).unwrap().body,
            buttons[0].1
        );
    }

    crate::laboratory_cases! {
        dependency_accordions_keep_full_licenses_and_toggle_independently,
    }
}
