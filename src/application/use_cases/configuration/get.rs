use std::io::Error;

use axum::response::Html;

use crate::infrastructure::database::connection::connection;

pub async fn get_use_case() -> Result<Html<String>, Error> {
    let conn = connection().await;
    let conn = match conn {
        Ok(conn) => conn,
        Err(err) => panic!("{}", err),
    };
    // let configuration = match conn.execute("SELECT * FROM configuration", ()) {
    //     Ok(rows) => {
    //         let mut configuration = String::new();
    //         for row in rows {
    //             configuration.push_str(&format!("{:?}", row));
    //         }
    //         configuration
    //     }
    //     Err(err) => panic!("{}", err),
    // };
    let configuration = conn.prepare("SELECT * FROM configuration");

    if configuration.is_err() {
        return Err(Error::new(
            std::io::ErrorKind::Other,
            "Failed to prepare statement",
        ));
    }
    let configuration = configuration.unwrap();
    // let configs =

    Html(format!(
        r#"
        <pre>{configuration:?}</pre>
        "#
    ))
}
