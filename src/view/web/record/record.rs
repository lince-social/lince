use maud::{Markup, html};

use crate::model::database::repositories::record::create_record;

pub async fn get_record() -> Markup {
    let record = create_record().await;
    if record.is_err() {
        println!("errooo {}", record.as_ref().unwrap_err())
    }
    let record = record.unwrap();
    if record.is_some() {
        println!(" aaaaaa {:?}", record.as_ref().unwrap());
    } else {
        println!("acho que n tem")
    }
    html!({ pre { @if let Some(record) = record {
        "There is a record"
    } @else {
            "No records"
        }
    }})
}
