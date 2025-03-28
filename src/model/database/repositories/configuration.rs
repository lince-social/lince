use crate::model::{
    database::management::connection::connection, entities::configuration::Configuration,
};
use std::io::{Error, ErrorKind};

pub async fn get_active() -> Result<Configuration, Error> {
    let conn = connection().await.map_err(|e| {
        Error::new(
            ErrorKind::ConnectionAborted,
            format!("Error connecting to database: {}", e),
        )
    });
    if conn.is_err() {
        return Err(Error::new(
            ErrorKind::NotConnected,
            "Connection to database failed",
        ));
    }
    let conn = conn.unwrap();

    let mut stmt = conn
        .prepare("SELECT * FROM configuration WHERE quantity = 1")
        .unwrap();

    let row = stmt
        .query_row([], |row| {
            Ok(Configuration {
                id: row.get(0).unwrap(),
                quantity: row.get(1).unwrap(),
                name: row.get(2).unwrap(),
                language: row.get(3).unwrap(),
                timezone: row.get(4).unwrap(),
                style: row.get(5).unwrap(),
            })
        })
        .map_err(|e| {
            Error::new(
                ErrorKind::ConnectionAborted,
                format!("Error connecting to database: {}", e),
            )
        });

    row

    // let row = conn.execute("SELECT * FROM configuration WHERE quantity = 1", []);
    // if row.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::InvalidData,
    //         "Error when querying active configuration",
    //     ));
    // }
    // let row: Configuration = row;

    // let mut stmt = conn.prepare(
    // r#"
    //     SELECT
    //         c.name AS config_name,
    //         c.quantity AS config_quantity,
    //         v.name AS view_name,
    //         cv.quantity AS view_quantity
    //     FROM configuration c
    //     LEFT JOIN configuration_view cv ON c.id = cv.configuration_id WHERE c.quantity = 1
    //     LEFT JOIN view v ON cv.view_id = v.id
    //     ORDER BY c.id, v.id
    //     "#,
    // );
    // if stmt.is_err() {
    //     return Err(Error::new(ErrorKind::Other, "Error in statement"));
    // }
    // let stmt = stmt.unwrap();
    // let rows = stmt.query_map([], |row| Ok(row));
    // if rows.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::InvalidData,
    //         "Error when querying active config",
    //     ));
    // }
    // let rows = rows.unwrap();
    // println!("{}", rows);

    // let rows = stmt.query_map([], |row| {
    //         Ok((
    //             row.get::<_, String>("config_name").unwrap(),
    //             row.get::<_, i32>("config_quantity").unwrap(),
    //             row.get::<_, Option<String>>("view_name").unwrap(),
    //             row.get::<_, Option<i32>>("view_quantity").unwrap(),
    //         ))
    //         });
    // let rows = rows.unwrap();

    //     let mut config_map = HashMap::new();

    //     for row in rows {
    //         let (config_name, config_quantity, view_name, view_quantity) = row.unwrap();

    //         let entry = config_map.entry(config_name.clone()).or_insert(ConfigView {
    //             config_name: config_name.clone(),
    //             config_quantity,
    //             views: Vec::new(),
    //         });

    //         if let (Some(view_name), Some(view_quantity)) = (view_name, view_quantity) {
    //             entry.views.push(ViewQuantity {
    //                 view_name,
    //                 view_quantity,
    //             });
    //         }
    //     }

    //     Ok(config_map.into_values().collect())
    // let query = "SELECT name, quantity FROM configuration WHERE quantity = 1";
    // let mut stmt = conn.prepare(query).map_err(|e| {
    //     Error::new(
    //         ErrorKind::InvalidInput,
    //         format!("Failed to prepare query: {}", e),
    //     )
    // })?;

    // let active_config = stmt
    //     .query_map([], |row| {
    //         let name: String = row.get(0).unwrap_or_else(|_| "Unknown".to_string());
    //         let quantity: i32 = row.get(1).unwrap_or(0);
    //         Ok((name, quantity))
    //     })
    //     .unwrap();

    // let mut configs = Vec::new();
    // for config in active_config {
    //     let (name, quantity) = config.map_err(|e| {
    //         Error::new(
    //             ErrorKind::InvalidData,
    //             format!("Failed to process row: {}", e),
    //         )
    //     })?;
    //     configs.push((name, quantity));
    // }

    // Ok(configs)
}

pub async fn get_inactive() {
    let conn = connection().await.unwrap();
    let query = "SELECT * FROM configuration WHERE quantity <> 1";
}

pub async fn set_active(id: String) {
    let conn = connection().await.unwrap();
    let query =
        format!("UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END");
}
