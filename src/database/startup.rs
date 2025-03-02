use super::schema::schema_database;
use super::seed::seed;

pub fn startup_database() {
    let _ = schema_database();
    let _ = seed();
}
