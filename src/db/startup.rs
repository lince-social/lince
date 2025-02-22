import { $, file, sql, write } from "bun";

export async function deleteDatabaseIfExists() {
  await $`dropdb -h localhost -p 2000 -f --if-exists -U postgres --no-password lince`;
}

export async function createDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -c "CREATE DATABASE lince TEMPLATE template0"`;
}

export async function schemaDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d lince < db/schema.sql`;
}

export async function grantPermissions() {
  await $`psql -U postgres -h localhost -p 2000 -c "GRANT ALL PRIVILEGES ON DATABASE lince TO postgres"`;
}

export async function checkEmptyDatabase() {
  const configs = await sql`SELECT * FROM view;`;
  return configs.length === 0;
}

export async function getOrigin() {
  const path = import.meta.dir + "/origin.txt";
  const origin = file(path);
  const text = await origin.text();
  return text;
}

export async function changeOrigin(newOrigin: string) {
  const path = import.meta.dir + "/origin.txt";
  write(path, newOrigin);
}

export async function createDatabaseFile() {
  const origin = await getOrigin();
  await $`find ${origin} || touch ${origin}`;
}

export async function loadDatabase() {
  const origin = await getOrigin();
  await $`psql -U postgres -h localhost -p 2000 -d lince < ${origin}`;
}

export async function seedDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d lince < db/seed.sql`;
}

export async function saveDatabase() {
  const origin = await getOrigin();
  await $`pg_dump -U postgres -h localhost -p 2000 -d lince --no-owner --no-privileges --no-password --no-comments --data-only -F plain -f ${origin}`;
}

export async function startup() {
  await deleteDatabaseIfExists();
  await createDatabase();
  await schemaDatabase();
  await grantPermissions();
  await createDatabaseFile();
  await loadDatabase();
  const databaseIsEmpty = await checkEmptyDatabase();
  if (databaseIsEmpty) {
    await seedDatabase();
  }
  await saveDatabase();
}

startup();
