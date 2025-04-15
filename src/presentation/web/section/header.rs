use crate::presentation::web::configuration::configurations::presentation_web_configuration_unhovered;

use super::nav::nav;

pub async fn header() -> String {
    "<header>".to_string()
        + nav().await.as_str()
        + presentation_web_configuration_unhovered().await.0.as_str()
        + "</header>"
}
