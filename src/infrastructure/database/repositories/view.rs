use crate::{
    domain::entities::table::{Row as RowEntity, Table},
    infrastructure::database::management::lib::connection,
};
use futures::future::join_all;
use sqlx::{Column, Row, TypeInfo};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn repository_view_toggle_view_id(
    view_id: String,
    collection_id: String,
) -> Result<(), Error> {
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&format!(
        "UPDATE collection_view
           SET quantity = CASE
              WHEN quantity = 1 THEN 0
              ELSE 1
            END
           WHERE view_id = {} AND collection_id = {}",
        &view_id, &collection_id
    ))
    .execute(&pool)
    .await;

    Ok(())
}

pub async fn repository_view_toggle_collection_id(collection_id: String) -> Result<(), Error> {
    let pool = connection().await.unwrap();
    let _ = sqlx::query(
        "
        UPDATE collection_view
        SET quantity = CASE
            WHEN EXISTS (
                SELECT 1
                FROM collection_view
                WHERE collection_id = $1
                  AND quantity = 1
            )
            THEN 0
            ELSE 1
        END
        WHERE collection_id = $1;
        ",
    )
    .bind(collection_id)
    .execute(&pool)
    .await;

    Ok(())
}

pub async fn repository_execute_queries(
    queries: Vec<String>,
) -> Result<Vec<(String, Table)>, Error> {
    let pool = connection().await.unwrap();

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
            let rows = sqlx::query(&query_string).fetch_all(&pool).await;
            if rows.is_err() {
                return Err(Error::new(ErrorKind::InvalidData, "Error when querying"));
            }
            let rows = rows.unwrap();
            let mut result_rows: Table = Vec::with_capacity(rows.len());

            for row in rows {
                let mut row_map: RowEntity = HashMap::new();
                for (i, col) in row.columns().iter().enumerate() {
                    let col_name = col.name();
                    let type_name = col.type_info().name().to_uppercase();
                    let value = match type_name.as_str() {
                        "INTEGER" => row
                            .try_get::<i64, _>(i)
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "NULL".to_string()),
                        "REAL" | "FLOAT" => row
                            .try_get::<f64, _>(i)
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

            Ok::<_, Error>((table_name, result_rows))
        }
    });

    let results = join_all(task_futures).await;

    results.into_iter().collect()
}

pub async fn repository_view_get_active_view_data()
-> Result<(Vec<(String, Table)>, Vec<String>), Error> {
    let pool = connection().await.unwrap();

    let query_rows = sqlx::query(
        "SELECT v.query AS query
         FROM collection_view cv
         JOIN view v ON cv.view_id = v.id
         JOIN collection c ON cv.collection_id = c.id
         WHERE cv.quantity = 1 AND c.quantity = 1",
    )
    .fetch_all(&pool)
    .await;

    if query_rows.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Error when querying main data",
        ));
    }
    let query_rows = query_rows.unwrap();

    let queries: Vec<String> = query_rows
        .into_iter()
        .map(|row| row.get::<String, _>("query"))
        .collect();

    let (special_queries, sql_queries): (Vec<_>, Vec<_>) = queries
        .into_iter()
        .partition(|query| ["karma_orchestra".to_string()].contains(query));

    let res = repository_execute_queries(sql_queries).await;
    if res.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Error when querying main data",
        ));
    }
    Ok((res.unwrap(), special_queries))
}

// export async function CreateView(collectionId, body) {
//   try {
//     const { viewname, query } = await body;
//     console.log(collectionId, viewname, query);
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
//     // Insert into collection_view if it doesn't already exist
//     await sql`
//       INSERT INTO collection_view (collection_id, view_id, is_active)
//       VALUES (${collectionId}, ${viewId}, true)
//       ON CONFLICT (collection_id, view_id) DO NOTHING;
//     `;
//     return await Body();
//   } catch (error) {
//     const { viewname, query } = await body;
//     console.log(
//       `Error: ${error}, when creating new view in collection with id: ${collectionId}. View received: ${viewname}, Query received: ${query}`,
//     );
//     return { success: false, error: error };
//   }
// }
//
// export async function CreateViewComponent(collectionId, view_name, query) {
//   return <p>osidnodicn</p>;
// }
//
// export async function InitialAddView(collectionId, viewname, query) {
//   return (
//     <div>
//       {await AddViewInput(collectionId, viewname, query)}
//       {await MatchedViewProperties(collectionId, viewname, query)}
//     </div>
//   );
// }
// export async function AddViewInput(collectionId, viewname, query) {
//   return (
//     <div>
//       <form
//         id="addviewcomponent"
//         hx-trigger={`keydown[key === "Enter"]`}
//         hx-post={`/view/${collectionId}`}
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
// export async function MatchedViewProperties(collectionId, viewname, query) {
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
//             hx-post={`/matchedviewqueryclick/${collectionId}/${query}}`}
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
