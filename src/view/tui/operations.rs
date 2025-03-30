// use std::io::Error;

// use sqlx::{Connection, SqliteConnection, SqlitePool};

// // let opt = sqlite::SqliteConnectOptions::new()
// //     .filename(db_path)
// //     .create_if_missing(true);
// // let pool = sqlite::SqlitePool::connect_with(opt).await;
// // if pool.is_err() {
// //     return Err(Error::new(ErrorKind::Other, "Pool error"));
// // }
// // let pool = pool.unwrap();

// pub async fn tui_create(table: String) -> Result<(), Error> {
//     let config_dir = dirs::config_dir().unwrap();
//     let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
//     let conn = SqliteConnection::connect(&db_path).await.unwrap();
//     let columns = conn.execute("")

//     Ok(())
// }
