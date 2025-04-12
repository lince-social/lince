use maud::Markup;

use crate::presentation::web::table::tables::presentation_web_tables;

pub async fn presentation_web_main() -> Markup {
    presentation_web_tables().await
}
