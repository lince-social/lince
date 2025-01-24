import { $, sql } from "bun";

export async function deleteDatabaseIfExists() {
  await $`dropdb -h localhost -p 2000 -f --if-exists -U postgres --no-password newlince`;
}

export async function createDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -c "CREATE DATABASE newlince TEMPLATE template0"`;
}

export async function schemaDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d newlince < db/schema.sql`;
}

export async function grantPermissions() {
  await $`psql -U postgres -h localhost -p 2000 -c "GRANT ALL PRIVILEGES ON DATABASE newlince TO postgres"`;
}

export async function checkEmptyDatabase() {
  const configs = await sql`SELECT * FROM view;`;
  return configs.length === 0;
}

export async function loadDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d newlince < ~/.config/lince/newlince.sql`;
}

export async function seedDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d newlince < db/seed.sql`;
}

export async function saveDatabase() {
  await $`pg_dump -U postgres -h localhost -p 2000 -d newlince --no-owner --no-privileges --no-password --no-comments --data-only -F plain -f ~/.config/lince/newlince.sql`;
}

export async function startup() {
  await deleteDatabaseIfExists();
  await createDatabase();
  await schemaDatabase();
  await grantPermissions();
  await loadDatabase();
  const isEmpty = await checkEmptyDatabase();
  if (isEmpty) {
    await seedDatabase();
  }
}

startup();
