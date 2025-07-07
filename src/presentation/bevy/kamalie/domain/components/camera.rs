use bevy::ecs::component::Component;

#[derive(Component)]
pub struct CameraController {
    pub distance: f32,
    pub angle_y: f32,
    pub angle_x: f32,
    pub front_view: bool,
}
