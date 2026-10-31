use super::*;
use bevy::prelude::{
    ChildOf, Children, ComputedNode, Display, Entity, InheritedVisibility, Name, Node, Text,
    UiGlobalTransform, Vec2,
};
use lince_interface::native::{self, actions::ActionButton};
use smithay::{
    backend::input::{Axis, AxisSource},
    input::pointer::AxisFrame,
};

#[derive(Clone, Copy, Debug)]
enum Widget {
    Caption(&'static str),
    Named(&'static str),
    Toast(&'static str),
    EditToggle,
}

struct Position {
    point: [f64; 2],
    visible: bool,
    scroll: Option<([f64; 2], f64)>,
}

fn locate(
    canvas: &mut crate::presentation::Presentation,
    widget: Widget,
    size: [u32; 2],
) -> Option<Position> {
    let world = canvas.world_mut();
    let toggle = if matches!(widget, Widget::EditToggle) {
        world
            .query::<&native::edit_mode::EditMode>()
            .iter(world)
            .next()
            .map(|mode| mode.toggle)
    } else {
        None
    };
    let entity = world
        .query::<(
            Entity,
            Option<&Children>,
            Option<&Name>,
            Option<&ActionButton>,
            Option<&bevy::a11y::AccessibilityNode>,
        )>()
        .iter(world)
        .find(|(entity, children, name, button, accessibility)| {
            let matched = match widget {
                Widget::Caption(caption) => {
                    accessibility.is_some_and(|node| node.label() == Some(caption))
                        || button.is_some()
                            && children.is_some_and(|children| {
                                children.iter().any(|child| {
                                    world
                                        .get::<Text>(*child)
                                        .is_some_and(|text| text.0 == caption)
                                })
                            })
                }
                Widget::Named(expected) => name.is_some_and(|name| name.as_str() == expected),
                Widget::Toast(prefix) => {
                    accessibility.is_some_and(|node| {
                        node.label()
                            == Some(lince_interface::i18n::Token::CloseToast.text())
                    }) && world
                        .get::<ChildOf>(*entity)
                        .and_then(|parent| {
                            world.get::<bevy::a11y::AccessibilityNode>(parent.parent())
                        })
                        .and_then(|node| node.label())
                        .is_some_and(|message| message.starts_with(prefix))
                }
                Widget::EditToggle => toggle == Some(*entity),
            };
            if !matched
                || world
                    .get::<InheritedVisibility>(*entity)
                    .is_some_and(|visible| !visible.get())
            {
                return false;
            }
            let mut cursor = Some(*entity);
            for _ in 0..64 {
                let Some(entity) = cursor else { return true };
                if world
                    .get::<Node>(entity)
                    .is_some_and(|node| node.display == Display::None)
                {
                    return false;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .map(|(entity, _, _, _, _)| entity)?;
    let node = world.get::<ComputedNode>(entity)?;
    if node.size().min_element() <= 0.0 {
        return None;
    }
    let point = world
        .get::<UiGlobalTransform>(entity)?
        .transform_point2(Vec2::ZERO);
    let viewport = bevy::prelude::Rect::from_corners(
        Vec2::ZERO,
        Vec2::from_array(size.map(|value| value as f32)),
    );
    let visible = viewport.contains(point)
        && world
            .get::<bevy::ui::CalculatedClip>(entity)
            .is_none_or(|clip| clip.clip.contains(point));
    let mut cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    let mut scroll = None;
    for _ in 0..64 {
        let Some(entity) = cursor else { break };
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.overflow.y == bevy::ui::OverflowAxis::Scroll)
            && let (Some(node), Some(transform)) = (
                world.get::<ComputedNode>(entity),
                world.get::<UiGlobalTransform>(entity),
            )
        {
            let center = transform.transform_point2(Vec2::ZERO);
            let mut rectangle =
                bevy::prelude::Rect::from_center_size(center, node.size()).intersect(viewport);
            if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
                rectangle = rectangle.intersect(clip.clip);
            }
            if rectangle.size().min_element() > 0.0 {
                scroll = Some((
                    [(rectangle.max.x - 3.0) as f64, rectangle.center().y as f64],
                    (point.y - rectangle.center().y).clamp(-512.0, 512.0) as f64,
                ));
                break;
            }
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    Some(Position {
        point: point.as_dvec2().to_array(),
        visible,
        scroll,
    })
}

impl Toolkit {
    pub(super) fn inspect<T: Send + 'static>(
        &mut self,
        inspect: impl FnOnce(&mut crate::presentation::Presentation) -> T + Send + 'static,
    ) -> T {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.fixture
            .desktop
            .worker
            .input(Input::Test(Box::new(move |canvas| {
                sender
                    .send(inspect(canvas))
                    .unwrap_or_else(|_| panic!("The toolkit inspection receiver closed"));
            })))
            .unwrap();
        self.tick("native-inspect");
        receiver.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    fn widget(&mut self, widget: Widget) -> [f64; 2] {
        let size = self.fixture.size;
        let mut trace = Vec::new();
        for _ in 0..24 {
            if let Some(position) = self.inspect(move |canvas| locate(canvas, widget, size)) {
                trace.push((position.point, position.visible, position.scroll));
                if position.visible {
                    return position.point;
                }
                if let Some((point, delta)) = position.scroll {
                    self.advance_time();
                    self.fixture.desktop.move_pointer(point, self.time).unwrap();
                    self.tick("native-scroll-hover");
                    self.advance_time();
                    self.fixture
                        .desktop
                        .axis(
                            [0.0, delta],
                            AxisFrame::new(self.time)
                                .source(AxisSource::Continuous)
                                .value(Axis::Vertical, delta),
                        )
                        .unwrap();
                    self.tick("native-scroll");
                }
            } else if matches!(widget, Widget::EditToggle)
                && let Some(corner) = self
                    .inspect(move |canvas| locate(canvas, Widget::Caption("Show controls"), size))
            {
                assert!(corner.visible);
                self.advance_time();
                self.fixture
                    .desktop
                    .move_pointer(corner.point, self.time)
                    .unwrap();
                self.tick("toolbar-reveal");
            }
        }
        let state = self.inspect(|canvas| {
            let world = canvas.world_mut();
            let modes = world.query::<&native::edit_mode::EditMode>().iter(world).map(|mode| mode.enabled).collect::<Vec<_>>();
            let summaries = world.query::<&Name>().iter(world).filter(|name| name.as_str().starts_with("shortcut.summary.")).count();
            (modes, summaries)
        });
        panic!("The native toolkit control is unavailable or unreachable: {widget:?}, state={state:?}, scroll={trace:?}");
    }

    fn native_click(&mut self, widget: Widget) {
        let point = self.widget(widget);
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("native-hover");
        assert!(matches!(
            self.fixture.desktop.target(),
            crate::desktop::input::Target::Native
        ));
        self.button(272, true);
        self.button(272, false);
    }

    pub(super) fn dismiss_toast(&mut self, prefix: &'static str) {
        self.native_click(Widget::Toast(prefix));
        let size = self.fixture.size;
        self.wait("native-toast-dismissed", |app| {
            app.inspect(move |canvas| locate(canvas, Widget::Toast(prefix), size).is_none())
        });
    }

    fn settings(&mut self) {
        self.chord(&[(125, true), (25, true), (25, false), (125, false)]);
        self.wait("settings-operation", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .is_some()
        });
        for code in [53, 35, 18, 38, 25, 28] {
            self.type_key(code);
        }
        assert!(self.inspect(|canvas| {
            let world = canvas.world_mut();
            world
                .query::<&native::edit_mode::EditMode>()
                .iter(world)
                .any(|mode| mode.enabled)
        }));
    }

    fn close_settings(&mut self) {
        self.native_click(Widget::EditToggle);
        assert!(self.inspect(|canvas| {
            let world = canvas.world_mut();
            world
                .query::<&native::edit_mode::EditMode>()
                .iter(world)
                .all(|mode| !mode.enabled)
        }));
    }

    fn native_focus(&self) {
        assert!(self.fixture.desktop.native_keyboard);
        let native = self.fixture.desktop.server.native_surface().unwrap();
        assert_eq!(
            self.fixture
                .desktop
                .server
                .state
                .seat
                .get_keyboard()
                .unwrap()
                .current_focus(),
            Some(native)
        );
    }

    fn return_text(&mut self, main: SurfaceId, text: &str, code: u32, suffix: char) {
        self.click(main, "controls", "field", 272);
        self.type_key(107);
        self.type_key(code);
        self.wait("capture-return", |app| {
            app.state()["text"] == format!("{text}{suffix}")
        });
        assert!(self.fixture.desktop.server.state.buttons.is_empty());
    }

    pub(super) fn capture_handoff(&mut self, main: SurfaceId) {
        let before = self.geometry(main).mapping;
        let size = self.size(main);
        let menus = self.events("menu-open");
        let point = self.point(main, "controls", "resize");
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("settings-resize-hover");
        self.chord(&[(125, true)]);
        self.button(273, true);
        self.chord(&[(125, false)]);
        self.wait("settings-resize", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_some()
        });
        self.advance_time();
        self.fixture
            .desktop
            .move_pointer([point[0] + 12.0, point[1] + 8.0], self.time)
            .unwrap();
        self.tick("settings-resize-preview");
        self.settings();
        assert!(
            self.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        self.button(273, false);
        self.configured(main);
        assert_eq!(self.size(main), size);
        assert_eq!(self.geometry(main).mapping, before);
        assert_eq!(self.events("menu-open"), menus);
        let target = Widget::Named("shortcut.summary.layout.toggle-tiling");
        self.widget(target);
        let text = self.state()["text"].as_str().unwrap().to_owned();
        let keys = self.state()["captureKeys"].clone();
        self.native_click(target);
        self.wait("shortcut-capture", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard_capture
        });
        self.native_focus();
        self.chord(&[(29, true), (60, true)]);
        assert_eq!(self.state()["text"], text);
        assert_eq!(self.state()["captureKeys"], keys);
        self.native_click(Widget::Caption("Cancel draft"));
        self.wait("shortcut-cancel", |app| {
            !app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard_modal
        });
        self.close_settings();
        let parent = self.window("Park toolkit parent").unwrap();
        self.wait("held-capture-return", |app| {
            app.fixture
                .desktop
                .server
                .state
                .seat
                .get_keyboard()
                .unwrap()
                .current_focus()
                == Some(parent.surface.clone())
        });
        self.chord(&[(60, false), (29, false)]);
        assert_eq!(self.state()["captureKeys"], keys);
        self.return_text(main, &text, 34, 'g');
    }

    pub(super) fn dialog_handoff(&mut self, main: SurfaceId) {
        self.settings();
        let target = Widget::Named("shortcut.summary.layout.toggle-tiling");
        self.widget(target);
        self.close_settings();
        self.click(main, "controls", "dialog", 272);
        self.wait("capture-dialog", |app| {
            app.window("Park toolkit dialog").is_some()
        });
        let dialog = self.window("Park toolkit dialog").unwrap();
        self.wait("capture-dialog-focus", |app| {
            app.fixture
                .desktop
                .server
                .state
                .seat
                .get_keyboard()
                .unwrap()
                .current_focus()
                == Some(dialog.surface.clone())
        });
        self.configured_size(dialog.id, "dialogSize");
        let center = |mapping: lince_interface::host::ContentMapping| {
            mapping
                .canvas_point(std::array::from_fn(|axis| {
                    mapping.surface.origin()[axis] + mapping.surface.size()[axis] * 0.5
                }))
                .unwrap()
        };
        let parent_center = center(self.geometry(main).mapping);
        let dialog_center = center(self.geometry(dialog.id).mapping);
        assert!(
            (0..2).all(|axis| (parent_center[axis] - dialog_center[axis]).abs() < 0.01),
            "An unmoved free dialog must be centered on its parent: {parent_center:?} / {dialog_center:?}"
        );
        let closed = self.events("dialog-auto-close");
        let armed = self.events("dialog-close-armed");
        self.click(dialog.id, "dialogControls", "deferred", 272);
        self.wait("dialog-close-armed", |app| {
            app.events("dialog-close-armed") == armed + 1
        });
        self.chord(&[(125, true), (18, true), (18, false), (125, false)]);
        self.wait("dialog-edit-open", |app| app.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<&native::edit_mode::EditMode>().iter(world).any(|mode| mode.enabled)
        }));
        self.native_click(target);
        self.wait("dialog-capture-owner", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard_capture
        });
        assert!(self.window("Park toolkit dialog").is_some());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.native_focus();
            assert!(
                self.fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .keyboard_capture
            );
            if self.events("dialog-auto-close") == closed + 1
                && self.window("Park toolkit dialog").is_none()
            {
                break;
            }
            self.tick("dialog-close-during-capture");
            assert!(
                Instant::now() < deadline,
                "The inactive Qt dialog did not close during capture"
            );
        }
        let text = self.state()["text"].as_str().unwrap().to_owned();
        self.type_key(60);
        self.native_focus();
        assert_eq!(self.state()["text"], text);
        self.native_click(Widget::Caption("Cancel capture"));
        self.wait("dialog-capture-cancel", |app| {
            !app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard_capture
        });
        self.native_click(Widget::Caption("Cancel draft"));
        self.close_settings();
        self.return_text(main, &text, 35, 'h');
    }
}
