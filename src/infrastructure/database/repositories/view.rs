use futures::future::join_all;
use sqlx::{Column, Row, TypeInfo};
use std::collections::HashMap;
use std::io::Error;

use super::record::connection;

pub async fn repository_view_toggle(id: String) -> Result<(), Error> {
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&format!(
        "UPDATE configuration_view
           SET quantity = CASE
              WHEN quantity = 1 THEN 0
              ELSE 1
            END
           WHERE view_id = {}",
        &id
    ))
    .execute(&pool)
    .await;

    Ok(())
}

pub async fn repository_view_get_active_view_data()
-> sqlx::Result<Vec<(String, Vec<HashMap<String, String>>)>> {
    let pool = connection().await.unwrap();

    let query_rows = sqlx::query(
        "SELECT v.query AS query
         FROM configuration_view cv
         JOIN view v ON cv.view_id = v.id
         JOIN configuration c ON cv.configuration_id = c.id
         WHERE cv.quantity = 1 AND c.quantity = 1",
    )
    .fetch_all(&pool)
    .await?;

    let queries: Vec<String> = query_rows
        .into_iter()
        .map(|row| row.get::<String, _>("query"))
        .collect();

    let task_futures = queries.into_iter().map(|query_string| {
        let table_name = query_string
            .split_whitespace()
            .enumerate()
            .find_map(|(i, word)| {
                if word.eq_ignore_ascii_case("from") {
                    query_string.split_whitespace().nth(i + 1)
                } else {
                    None
                }
            })
            .unwrap_or("unknown_table")
            .to_string();

        let pool = pool.clone();
        async move {
            let rows = sqlx::query(&query_string).fetch_all(&pool).await?;
            let mut result_rows = Vec::with_capacity(rows.len());

            for row in rows {
                let mut row_map: HashMap<String, String> = HashMap::new();

                let columns = row.columns();

                for (i, col) in columns.iter().enumerate() {
                    let col_name = col.name();
                    let type_name = col.type_info().name().to_uppercase();

                    let value = match type_name.as_str() {
                        "INTEGER" => row
                            .try_get::<i64, _>(i)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string()),
                        "REAL" => row
                            .try_get::<f64, _>(i)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string()),
                        "FLOAT" => row
                            .try_get::<f32, _>(i)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string()),
                        _ => row
                            .try_get::<String, _>(i)
                            .unwrap_or_else(|_| "NULL".to_string()),
                    };

                    row_map.insert(col_name.to_string(), value);
                }

                result_rows.push(row_map);
            }

            Ok::<_, sqlx::Error>((table_name, result_rows))
        }
    });

    let results = join_all(task_futures).await;

    let mut all_query_results = Vec::new();
    for res in results {
        all_query_results.push(res?);
    }

    Ok(all_query_results)
}

// export async function CreateView(configurationId, body) {
//   try {
//     const { viewname, query } = await body;
//     console.log(configurationId, viewname, query);
//
//     // Check if the view already exists
//     const existingView = await sql`
//       SELECT id FROM view
//       WHERE view_name = ${viewname} AND query = ${query}
//       LIMIT 1;
//     `;
//
//     let viewId;
//
//     if (existingView.length > 0) {
//       viewId = existingView[0].id;
//     } else {
//       // Insert the new view and get its ID
//       const insertedView = await sql`
//         INSERT INTO view (view_name, query)
//         VALUES (${viewname}, ${query})
//         RETURNING id;
//       `;
//       viewId = insertedView[0].id;
//     }
//
//     // Insert into configuration_view if it doesn't already exist
//     await sql`
//       INSERT INTO configuration_view (configuration_id, view_id, is_active)
//       VALUES (${configurationId}, ${viewId}, true)
//       ON CONFLICT (configuration_id, view_id) DO NOTHING;
//     `;
//     return await Body();
//   } catch (error) {
//     const { viewname, query } = await body;
//     console.log(
//       `Error: ${error}, when creating new view in configuration with id: ${configurationId}. View received: ${viewname}, Query received: ${query}`,
//     );
//     return { success: false, error: error };
//   }
// }
//
// export async function getViews() {
//   return await sql`SELECT viewName, query FROM view`;
// }
//
// export async function DeleteView(query) {
//   const { viewId, configurationId } = query;
//
//   await sql`
//     DELETE FROM configuration_view
//     WHERE configuration_id = ${configurationId} AND view_id = ${viewId};
//   `;
//
//   return (
//     <main id="main">
//       <div>{await ConfigurationsHovered()}</div>
//       <div>{await Tables()}</div>
//     </main>
//   );
// }
//
// export async function CreateViewComponent(configurationId, view_name, query) {
//   return <p>osidnodicn</p>;
// }
//
// export async function InitialAddView(configurationId, viewname, query) {
//   return (
//     <div>
//       {await AddViewInput(configurationId, viewname, query)}
//       {await MatchedViewProperties(configurationId, viewname, query)}
//     </div>
//   );
// }
// export async function AddViewInput(configurationId, viewname, query) {
//   return (
//     <div>
//       <form
//         id="addviewcomponent"
//         hx-trigger={`keydown[key === "Enter"]`}
//         hx-post={`/view/${configurationId}`}
//         hx-target="#body"
//         class="flex relative space-x-2 p-1"
//       >
//         <input
//           name="viewname"
//           placeholder="Add view"
//           class="rounded text-white bg-transparent border border-white"
//           value={viewname}
//           autofocus
//         />
//         <input
//           name="query"
//           placeholder="Query..."
//           class="rounded text-white bg-transparent border borde-white"
//           value={query}
//         />
//       </form>
//     </div>
//   );
// }
//
// export async function MatchedViewProperties(configurationId, viewname, query) {
//   const views = await sql`SELECT view_name, query FROM view`;
//
//   function containsAllChars(str: string, chars: string): boolean {
//     if (typeof chars !== "string") return false;
//     return chars
//       .split("")
//       .every((char) => str.toLowerCase().includes(char.toLowerCase()));
//   }
//
//   const queriedNames = views
//     .filter((item) => containsAllChars(item.view_name, viewname))
//     .map((item) => item.view_name);
//
//   const queriedQueries = views
//     .filter((item) => containsAllChars(item.query, query))
//     .map((item) => item.query);
//
//   return (
//     <div
//       id="matchedviewproperties"
//       class="z-50 relative flex flex-wrap justify-between w-full mt-2 space-x-2"
//     >
//       <ul class="absolute left-0 top-full mt-2 bg-black text-white p-2 rounded border border-white min-w-[150px] max-w-[45%]">
//         {queriedNames.length === 0 ? (
//           <li class="trucate px-2 py-1">{viewname}</li>
//         ) : (
//           queriedNames.map((item) => <li class="truncate px-2 py-1">{item}</li>)
//         )}
//       </ul>
//       <ul class="absolute right-0 top-full mt-2 bg-black text-white p-2 rounded border border-white min-w-[150px] max-w-[45%]">
//         {queriedQueries.length === 0 ? (
//           <li
//             hx-triger="click"
//             hx-post={`/matchedviewqueryclick/${configurationId}/${query}}`}
//             class="truncate px-2 py-1"
//           >
//             {query}
//           </li>
//         ) : (
//           queriedQueries.map((item) => (
//             <li class="truncate px-2 py-1">{item}</li>
//           ))
//         )}
//       </ul>
//     </div>
//   )
// }
