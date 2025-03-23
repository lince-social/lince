use rusqlite::Connection;
use std::io::{Error, ErrorKind};

pub async fn connection() -> Result<Connection, Error> {
    let config_dir = dirs::config_dir();
    if config_dir.is_none() {
        return Err(Error::new(ErrorKind::NotADirectory, "Config dir is none"));
    }
    let config_dir = String::from(config_dir.unwrap().to_str().unwrap());

    let db_path = config_dir + "/lince/lince.db";

    match Connection::open(db_path) {
        Ok(connection) => Ok(connection),
        Err(e) => Err(Error::new(ErrorKind::Other, e.to_string())),
    }
}
