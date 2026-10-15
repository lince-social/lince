use super::*;
use smithay::backend::allocator::Buffer;

#[derive(Default)]
pub(super) struct Check {
    phase: u8,
}

impl Check {
    pub(super) fn complete(&self) -> bool {
        self.phase == 9
    }

    pub(super) fn advance(&mut self, desktop: &mut Desktop) -> io::Result<bool> {
        if self.phase == 8 && desktop.server.windows().is_empty() {
            self.phase = 9;
            return Ok(true);
        }
        let Some(current) = &desktop.current else {
            return Ok(false);
        };
        let Some(geometry) = &current.frame.geometry else {
            return Ok(false);
        };
        let Some(item) = geometry
            .items
            .iter()
            .find(|item| matches!(item.id, ItemId::Application(_)))
        else {
            return Ok(false);
        };
        let ItemId::Application(id) = item.id else {
            unreachable!()
        };
        let item_output = item.output;
        let focused_output = current.frame.output;
        let output_bounds = geometry
            .outputs
            .iter()
            .find(|output| Some(output.id) == focused_output)
            .unwrap()
            .bounds;
        let region = item.mapping.canvas;
        let center = std::array::from_fn(|axis| region.origin()[axis] + region.size()[axis] * 0.5);
        let modal = current.frame.input.modal;
        let fullscreen = current.frame.fullscreen;
        let size = current.frame.dmabuf().size();
        let origin = current.frame.origin;
        let available = desktop
            .server
            .usable_output(focused_output.unwrap_or(OutputId(1)))
            .unwrap();
        let fitted = origin == [f64::from(available.loc.x), f64::from(available.loc.y)]
            && size.w == available.size.w
            && size.h == available.size.h;
        match self.phase {
            0 if desktop.layers.len() == 2
                && fitted
                && desktop
                    .popups
                    .iter()
                    .any(|popup| popup.owner == renderer::PopupOwner::Application(id)) =>
            {
                let popup = desktop
                    .popups
                    .iter()
                    .find(|popup| popup.owner == renderer::PopupOwner::Application(id))
                    .unwrap();
                let point = std::array::from_fn(|axis| {
                    popup.mapping.canvas.origin()[axis] + popup.mapping.canvas.size()[axis] * 0.5
                });
                let surface = popup.nodes.first().unwrap().surface.clone();
                desktop.pointer = point;
                if region.contains(point)
                    || !matches!(desktop.target(), input::Target::Application(target, ref actual, _) if target == id && *actual == surface)
                {
                    return Err(io::Error::other(
                        "an application popup outside its parent did not receive paired input",
                    ));
                }
                desktop.pointer = center;
                if !matches!(desktop.target(), input::Target::Application(target, _, _) if target == id)
                {
                    return Err(io::Error::other(
                        "application input does not match its rendered rectangle",
                    ));
                }
                desktop.pointer = [20.0, 12.0];
                if !matches!(desktop.target(), input::Target::Layer(..)) {
                    return Err(io::Error::other(
                        "the top bar does not receive input in its reserved space",
                    ));
                }
                desktop.pointer = [20.0, f64::from(desktop.size[1]) - 12.0];
                if !matches!(desktop.target(), input::Target::Layer(..)) {
                    return Err(io::Error::other(
                        "the bottom bar does not receive input in its reserved space",
                    ));
                }
                desktop.pointer = center;
                desktop.send(Input::Action(DesktopAction::OpenOperation))?;
                self.phase = 1;
            }
            1 if modal => {
                desktop.pointer = center;
                if !matches!(desktop.target(), input::Target::Native) {
                    return Err(io::Error::other(
                        "Operation did not take input above the application",
                    ));
                }
                desktop.key(1, true, 0)?;
                desktop.key(1, false, 0)?;
                self.phase = 2;
            }
            2 if !modal => {
                desktop.pointer = center;
                desktop.button(272, true, 0)?;
                desktop.button(272, false, 0)?;
                desktop.send(Input::Action(DesktopAction::ToggleFullscreen))?;
                self.phase = 3;
            }
            3 if fullscreen => {
                if size
                    != (
                        output_bounds.size()[0] as i32,
                        output_bounds.size()[1] as i32,
                    )
                        .into()
                    || origin != [0.0; 2]
                {
                    return Err(io::Error::other(
                        "fullscreen did not expand the native view to the output",
                    ));
                }
                desktop.pointer = [20.0, 12.0];
                if matches!(desktop.target(), input::Target::Layer(..)) {
                    return Err(io::Error::other(
                        "a hidden top bar intercepted fullscreen input",
                    ));
                }
                desktop.send(Input::Action(DesktopAction::ToggleFullscreen))?;
                self.phase = 4;
            }
            4 if !fullscreen && fitted => {
                if desktop.output_count > 1 {
                    desktop.send(Input::Action(DesktopAction::MoveToOutput(
                        lince_interface::host::Direction::Right,
                    )))?;
                    self.phase = 5;
                } else {
                    desktop.server.close(id);
                    self.phase = 8;
                }
            }
            5 if item_output != OutputId(1) && focused_output == Some(item_output) => {
                desktop.send(Input::Action(DesktopAction::FocusOutput(
                    lince_interface::host::Direction::Left,
                )))?;
                self.phase = 6;
            }
            6 if item_output != OutputId(1) && focused_output == Some(OutputId(1)) => {
                desktop.pointer = center;
                desktop.button(272, true, 0)?;
                desktop.button(272, false, 0)?;
                self.phase = 7;
            }
            7 if focused_output == Some(item_output) && desktop.pending_output.is_none() => {
                desktop.pointer = center;
                if !matches!(desktop.target(), input::Target::Application(target, _, _) if target == id)
                {
                    return Err(io::Error::other(
                        "the moved application lost its paired input target",
                    ));
                }
                desktop.server.close(id);
                self.phase = 8;
            }
            _ => {}
        }
        Ok(self.complete())
    }
}
