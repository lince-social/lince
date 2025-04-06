use crate::domain::entities::configuration::Configuration;
use maud::{Markup, html};

pub fn presentation_web_configuration_row(configuration: Configuration) -> Markup {
    html!(div class="framed" { button {(configuration.name)}})
}
