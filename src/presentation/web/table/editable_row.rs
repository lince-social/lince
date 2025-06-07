use maud::{Markup, html};

use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn presentation_web_table_editable_row(
    services: InjectedServices,
    table: String,
    id: String,
    column: String,
    value: String,
) -> Markup {
    html!(
    td {
                form
                    method="post"
                    action=(format!("/table/{}/{}/{}", table, id, column))
                    hx-patch=(format!("/table/{}/{}/{}", table, id, column))
                    hx-target="#main"
                {
                    textarea name="value" autofocus {
                        (value)
                    }
                    button type="submit" { "Save" }
                }
            }
    )
}
