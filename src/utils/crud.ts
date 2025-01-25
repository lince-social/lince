import { sql } from "bun";

export async function getData(query) {
  const data = await sql`${query}`
  return data
}

export async function createData(query) {

}

export async function updateData(table, set, where) {

}

export async function deleteData(table, where) {

}

export async function getActiveConfiguration() {
  const data = await sql`SELECT id, configurationName, quantity, views FROM configuration WHERE quantity = 1`;
  return data;
}

export async function getInactiveConfigurations() {
  const data = await sql`SELECT id, configurationName, quantity, views  FROM configuration WHERE quantity <> 1`;
  return data;
}

export async function getViews() {
  const data = await sql`SELECT viewName, query FROM view`
  return data
}

