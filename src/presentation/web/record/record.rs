use maud::{Markup, html};

use crate::infrastructure::database::repositories::record::create_record;

pub async fn get_record() -> Markup {
    let record = create_record().await;
    let record = record.unwrap();

    html!({ pre { @if let Some(record) = record {
        (record.head)
    } @else {
            "No records"
        }
    }})
}
