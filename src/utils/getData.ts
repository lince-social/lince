import { sql } from "bun";

export async function getActiveConfiguration() {
  const data = await sql`SELECT * FROM configuration WHERE quantity = 1`;
  return data;
}

export async function getInactiveConfigurations() {
  const data = await sql`SELECT * FROM configuration WHERE quantity <> 1`;
  return data;
}

export async function getViews() {
  const data = await sql`SELECT * FROM view`
  return data
}

export async function getRecords() { }
