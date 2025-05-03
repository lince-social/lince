use crate::{
    application::schema::view::queried_view::QueriedView,
    domain::entities::configuration::Configuration,
};

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct ConfigurationForBarScheme {
    pub id: u32,
    pub name: String,
    pub quantity: i32,
}

impl From<Configuration> for ConfigurationForBarScheme {
    fn from(value: Configuration) -> Self {
        Self {
            id: value.id,
            name: value.name,
            quantity: value.quantity,
        }
    }
}

pub type ConfigurationRow = (ConfigurationForBarScheme, Vec<QueriedView>);
