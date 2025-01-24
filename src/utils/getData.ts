import { sql } from "bun";

export async function getActiveConfiguration() {
  const data = await sql`SELECT * FROM configuration`;
  return data.filter((configuration) => configuration.quantity === 1);
}

export async function getInactiveConfigurations() {
  const data = await sql`SELECT * FROM configuration`;
  return data.filter((configuration) => configuration.quantity !== 1);
}

export async function getViews() { }

export async function getRecords() { }
