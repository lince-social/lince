use rusqlite::{Connection, Result};

#[derive(Debug)]
struct Record {
    head: String,
}

struct View {
    view_name: String,
    query: String,
}

struct Configuration {
    configuration_name: String,
}

struct ConfigurationView {
    view_id: i32,
    configuration_id: i32,
}

pub fn seed() -> Result<()> {
    let path = "/home/eduardo/.config/lince/lince.db";
    let conn = Connection::open(path)?;

    let apple = Record {
        head: "Apple".to_string(),
    };
    conn.execute(
        "INSERT INTO record (head) SELECT (?1) WHERE NOT EXISTS (SELECT * FROM record)",
        (apple.head,),
    )
    .expect("Error when seeding record");

    let configuration = Configuration {
        configuration_name: "First Config".to_string(),
    };
    conn.execute(
        "INSERT INTO configuration (configuration_name) SELECT (?1) WHERE NOT EXISTS (SELECT * FROM configuration)",
        (configuration.configuration_name,),
    )
    .expect("Error when seeding configuration");

    let view = View {
        view_name: "First View".to_string(),
        query: "SELECT * FROM record".to_string(),
    };
    conn.execute(
        "INSERT INTO view (view_name, query) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT * FROM view)",
        (view.view_name, view.query),
    )
    .expect("Error when seeding view");

    let configuration_view = ConfigurationView {
        view_id: 1,
        configuration_id: 1,
    };
    conn.execute(
        "INSERT INTO configuration_view (view_id, configuration_id) SELECT ?1, ?2 WHERE NOT EXISTS (SELECT * FROM configuration_view)",
        (configuration_view.view_id, configuration_view.configuration_id),
    )
    .expect("Error when seeding configuration_view");

    Ok(())
}
