#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalConstraints {
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalInteraction {
    Move,
    Resize(ResizeEdges),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalFrameDrag {
    pub interaction: ModalInteraction,
    pub start_mouse_x: f32,
    pub start_mouse_y: f32,
    pub start_rect: ModalRect,
}

pub fn begin_drag(
    rect: ModalRect,
    mouse_x: f32,
    mouse_y: f32,
    frame_hit: f32,
    title_bar_height: f32,
) -> Option<ModalFrameDrag> {
    let interaction = hit_test(rect, mouse_x, mouse_y, frame_hit, title_bar_height)?;
    Some(ModalFrameDrag {
        interaction,
        start_mouse_x: mouse_x,
        start_mouse_y: mouse_y,
        start_rect: rect,
    })
}

pub fn begin_drag_with_interaction(
    rect: ModalRect,
    mouse_x: f32,
    mouse_y: f32,
    interaction: ModalInteraction,
) -> ModalFrameDrag {
    ModalFrameDrag {
        interaction,
        start_mouse_x: mouse_x,
        start_mouse_y: mouse_y,
        start_rect: rect,
    }
}

pub fn apply_drag(
    drag: ModalFrameDrag,
    mouse_x: f32,
    mouse_y: f32,
    constraints: ModalConstraints,
) -> ModalRect {
    let dx = mouse_x - drag.start_mouse_x;
    let dy = mouse_y - drag.start_mouse_y;

    match drag.interaction {
        ModalInteraction::Move => ModalRect {
            x: drag.start_rect.x + dx,
            y: drag.start_rect.y + dy,
            width: drag.start_rect.width,
            height: drag.start_rect.height,
        },
        ModalInteraction::Resize(edges) => {
            let mut rect = drag.start_rect;
            if edges.left {
                let new_x = drag.start_rect.x + dx;
                let new_w = drag.start_rect.width - dx;
                let clamped_w = clamp_size(new_w, constraints.min_width, constraints.max_width);
                rect.x = drag.start_rect.x + (drag.start_rect.width - clamped_w);
                rect.width = clamped_w;
                if new_x > rect.x {
                    rect.x = new_x;
                }
            }
            if edges.right {
                rect.width = clamp_size(
                    drag.start_rect.width + dx,
                    constraints.min_width,
                    constraints.max_width,
                );
            }
            if edges.top {
                let new_y = drag.start_rect.y + dy;
                let new_h = drag.start_rect.height - dy;
                let clamped_h = clamp_size(new_h, constraints.min_height, constraints.max_height);
                rect.y = drag.start_rect.y + (drag.start_rect.height - clamped_h);
                rect.height = clamped_h;
                if new_y > rect.y {
                    rect.y = new_y;
                }
            }
            if edges.bottom {
                rect.height = clamp_size(
                    drag.start_rect.height + dy,
                    constraints.min_height,
                    constraints.max_height,
                );
            }
            rect
        }
    }
}

fn clamp_size(value: f32, min: f32, max: f32) -> f32 {
    if max > min {
        value.clamp(min, max)
    } else {
        value.max(min)
    }
}

fn hit_test(
    rect: ModalRect,
    mouse_x: f32,
    mouse_y: f32,
    frame_hit: f32,
    title_bar_height: f32,
) -> Option<ModalInteraction> {
    if mouse_x < rect.x
        || mouse_x > rect.x + rect.width
        || mouse_y < rect.y
        || mouse_y > rect.y + rect.height
    {
        return None;
    }

    let left = mouse_x - rect.x <= frame_hit;
    let right = rect.x + rect.width - mouse_x <= frame_hit;
    let top = mouse_y - rect.y <= frame_hit;
    let bottom = rect.y + rect.height - mouse_y <= frame_hit;
    let edges = ResizeEdges {
        left,
        right,
        top,
        bottom,
    };
    if edges.left || edges.right || edges.top || edges.bottom {
        return Some(ModalInteraction::Resize(edges));
    }

    if mouse_y - rect.y <= title_bar_height {
        return Some(ModalInteraction::Move);
    }

    None
}
