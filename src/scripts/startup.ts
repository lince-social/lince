import { sql, $ } from "bun";

export async function deleteDatabaseIfExists() {
  await $`dropdb -h localhost -p 2000 -f --if-exists -U postgres --no-password newlince`;
}

export async function createDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -c "CREATE DATABASE newlince TEMPLATE template0"`;
}

export async function schemaDatabase() {
  await $`psql -U postgres -h localhost -p 2000 -d newlince < db/schema.sql`;
  return console.log("schema database");
}

export async function checkEmptyDatabase() {
  await sql`SELECT * FROM configuration`;
}

export function saveDatabase() {
  return console.log("save database");
}

export function loadDatabase() {
  return console.log("load database");
}

export function seedDatabase() {
  return console.log("seed database");
}

export async function main() {
  await deleteDatabaseIfExists();
  await createDatabase();
  schemaDatabase();
  // loadDatabase();
  // if (checkEmptyDatabase() === true) {
  //   seedDatabase();
  // }
}

main();
