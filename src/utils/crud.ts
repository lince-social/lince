import { sql } from "bun";

export async function getData(query) {
  const data = await sql`${query}`;
  return data;
}

export async function getTableData() {
  const result = await sql`SELECT views FROM configuration WHERE quantity = 1`;
  const views = result[0].views;

  const activeViews = Object.keys(views).filter((viewName) => views[viewName]);

  const queriedQueries = await Promise.all(
    activeViews.map(async (activeView) => {
      return await sql`SELECT query FROM view WHERE view_name = ${activeView}`;
    }),
  );

  const mappedQueries = queriedQueries.map((query) => query[0].query);

  const tableNames = mappedQueries.map((query) => {
    const words = query.split(" ");
    let tableName = null;
    for (let i = 0; i < words.length; i++) {
      if (words[i].toUpperCase() === "FROM" && i + 1 < words.length) {
        tableName = words[i + 1]; // Get the word after "FROM"
        break;
      }
    }
    return tableName;
  });

  const data = await Promise.all(
    mappedQueries.map(async (query) => {
      const queriedData = await sql(query);
      return queriedData;
    }),
  );
  return [data, tableNames];
}

export async function createData(query) { }

export async function updateData(table, set, where) { }

export async function deleteData(table, where) { }

export async function getActiveConfiguration() {
  const data =
    await sql`SELECT id, configurationName, quantity, views FROM configuration WHERE quantity = 1`;
  return data;
}

export async function getInactiveConfigurations() {
  const data =
    await sql`SELECT id, configurationName, quantity, views  FROM configuration WHERE quantity <> 1`;
  return data;
}

export async function getViews() {
  const data = await sql`SELECT viewName, query FROM view`;
  return data;
}

export async function handleDataCreate(tableName: string, row: any[]) {
  return console.log(tableName, row);
}

// export async function handleDataDelete(idArray: number[], tableName: string) {
//   try {
//     await prisma[tableName].deleteMany({
//       where: {
//         id: {
//           in: idArray,
//         },
//       },
//     });
//     revalidatePath("/");
//     console.log(typeof idArray);
//     console.log(idArray);
//   } catch (error) {
//     console.error("Error in API:", error);
//   }
// }
//
// export async function handleDataRead(tableName) {
//   const data = prisma.$queryRawUnsafe(`SELECT * FROM ${tableName}`);
//   return data;
// }
//
// export async function handleDataUpdate(tableName, set, where) {
//   return console.log(tableName, set, where);
// }
//
