use super::lib::connection;

pub async fn execute_migration() {
    let pool = connection().await.unwrap();

    // INSERT INTO selection_view (id, quantity, selection_id, view_id)
    // SELECT id,quantity, configuration_id, view_id FROM configuration_view;
    // DROP TABLE configuration_view;
    let migration = sqlx::query(
        "
        PRAGMA foreign_keys = OFF;



        PRAGMA foreign_keys = ON;
        ",
    )
    .execute(&pool)
    .await;
    if migration.is_err() {
        println!("{}", migration.unwrap_err());
    }
}
