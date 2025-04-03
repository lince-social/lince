use super::{configuration::Configuration, view::View};

pub struct ConfigurationWithViews {
    configuration: Configuration,
    views: Vec<(View, i32)>,
}
