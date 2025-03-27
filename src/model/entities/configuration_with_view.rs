use super::{configuration::Configuration, view::View};

pub struct ConfigurationWithView {
    configuration: Configuration,
    views: Vec<(View, i32)>,
}
