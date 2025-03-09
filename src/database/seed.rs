use sqlx::sqlite;

struct Record {
    head: String,
}

// struct View {
//     view_name: String,
//     query: String,
// }

struct Configuration {
    configuration_name: String,
}

// struct ConfigurationView {
//     view_id: i32,
//     configuration_id: i32,
// }

pub fn seed() -> Result<()> {
    let config_dir = dirs::config_dir().unwrap();
    let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
    let opt = sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let conn = sqlite::SqlitePool::connect_with(opt);

    let apple = Record {
        head: "Apple".to_string(),
    };
    sqlx::query("INSERT INTO record (head) SELECT (?1) WHERE NOT EXISTS (SELECT * FROM record)")
        .bind(apple.head)
        .execute(&conn);
    // .expect("Error when seeding record");

    let configuration = Configuration {
        configuration_name: "First Config".to_string(),
    };
    sqlx::query("INSERT INTO configuration (configuration_name) SELECT (?1) WHERE NOT EXISTS (SELECT * FROM configuration)")
        .bind(configuration.configuration_name)
        .execute(&conn);
    // .expect("Error when seeding configuration");

    // let view = View {
    //     view_name: "First View".to_string(),
    //     query: "SELECT * FROM record".to_string(),
    // };
    // conn.execute(
    //     "INSERT INTO view (view_name, query) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT * FROM view)",
    // )
    // .bind(&view.view_name, &view.query)
    // .execute(&conn)
    // .expect("Error when seeding view");
    // sqlx::query(
    //     "INSERT INTO view (view_name, query) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT * FROM view)",
    // )
    // .bind([&view.view_name, &view.query])
    // .execute(&conn)
    // .expect("Error when seeding view");

    // let configuration_view = ConfigurationView {
    //     view_id: 1,
    //     configuration_id: 1,
    // };
    // sqlx::query("INSERT INTO configuration_view (view_id, configuration_id) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT * FROM configuration_view)")
    //         .bind([&configuration_view.view_id, &configuration_view.configuration_id])
    // .execute(&conn)
    //     .expect("Error when seeding configuration_view");
    Ok(())
}
