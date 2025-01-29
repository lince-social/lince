import { sql } from "bun";

export default async function handleConfigurationChange(id: string) {
  try {
    await sql`
      UPDATE configuration
      SET quantity = CASE
        WHEN id = ${id} THEN 1
        ELSE 0
      END
      WHERE EXISTS (
        SELECT 1 FROM configuration WHERE id = ${id}
      )
    `;
  } catch (error) {
    console.log("Error updating quantities:", error);
  }
}
