import { sql } from "bun";
import * as elements from "typed-html";
import Body from "../sections/Body";

export async function getTableData() {
  const result = await sql`
    SELECT v.query 
    FROM configuration_view cv
    JOIN view v ON cv.view_id = v.id
    WHERE cv.is_active = true
      AND cv.configuration_id = (SELECT id FROM configuration WHERE quantity = 1);
  `;

  const mappedQueries = result.map((row) => row.query);

  const tableNames = mappedQueries.map((query) => {
    const words = query.split(/\s+/);
    const fromIndex = words.findIndex((word) => word.toUpperCase() === "FROM");
    return fromIndex !== -1 ? words[fromIndex + 1] || null : null;
  });

  const data = await Promise.all(mappedQueries.map((query) => sql(query)));

  return [data, tableNames];
}

export async function DeleteRow(table, body) {
  try {
    const { ...fields } = await body;

    const conditions = [];

    for (let [key, value] of Object.entries(fields)) {
      if (typeof value === "string") {
        value = value.replace(/'/g, "''");
        if (value !== "") {
          const condition = `${key} = ${typeof value === "string" ? `'${value}'` : value}`;
          conditions.push(condition);
        }
      }
    }

    const whereClause = conditions.join(" AND ");

    const query = `DELETE FROM ${table} WHERE ${whereClause}`;
    await sql(query);
    return await Body();
  } catch (error) {
    console.log(
      `Error deleting data: ${body} in table: ${table}, error: ${error}`,
    );
  }
}
