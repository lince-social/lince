import { sql } from "bun";

export default async function handleConfigurationClick(id: string) {
  try {
    await sql`UPDATE configuration SET quantity = 0`;
    await sql`UPDATE configuration SET quantity = 1 WHERE id = ${id}`;
  } catch (error) {
    console.log("Error updating quantities:", error);
  }
}
