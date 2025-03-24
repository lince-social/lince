mod controller;
mod model;
mod view;

use axum::{Router, routing::get};
use controller::{configuration::configuration_router, section::section_router};
use model::{database::management::startup::tidy_database, karma::karma::karma};
use std::{thread, time::Duration};
use view::section::page::page;

#[tokio::main]
async fn main() {
    tidy_database().await;

    thread::spawn(|| {
        loop {
            karma();
            thread::sleep(Duration::from_secs(60));
        }
    });

    let app = Router::new()
        .route("/", get(page))
        .nest("/section", section_router().await)
        .nest("/configuration", configuration_router().await);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:6174").await.unwrap();

    println!("Listening on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

// export default async function app() {
//   new Elysia()
//     .use(html())
//     .get("/", async ({ html }) => {
//       const body = await Body();
//       return await html(
//         <Page>
//           <body class="m-4">
//             <div>{body}</div>
//           </body>
//         </Page>,
//       );
//     })
//     .post("/configurationclick/:id", async ({ params: { id } }) => {
//       return await ConfigurationChange(id);
//     })
//     .get("/configurationhovered", async () => {
//       return await ConfigurationsHovered();
//     })
//     .get("/configurationunhovered", async () => {
//       return await ConfigurationsUnhovered();
//     })
//     .put("/view", async ({ body }) => {
//       return await ToggleView(body);
//     })
//     .post(
//       "/view/:configurationid",
//       async ({ params: { configurationid }, body }) => {
//         return await CreateView(configurationid, body);
//       },
//     )
//     .delete("/view", async ({ query }) => {
//       return await DeleteView(query);
//     })
//     .post("/operation", async ({ body }) => {
//       return await OperationComponent(body);
//     })
//     .post("/query", async ({ body }) => {
//       const { query } = await body;
//       return await RunQuery(query);
//     })
//     .post("/data", async ({ body }) => {
//       return await CreateData(body);
//     })
//     .put("/dataform", async ({ body }) => {
//       const { table, set, where } = await body;
//       return await UpdateData(table, set, where);
//     })
//     .put(
//       "/datanotform/:table/:key/:id",
//       async ({ params: { table, key, id }, body }) => {
//         const { value } = body;
//         return await UpdateData(table, `${key}='${value}'`, `id=${id}`);
//       },
//     )
//     .delete("/data", async ({ query }) => {
//       const { table, where } = query;
//       return await RunQuery(`DELETE FROM ${table} WHERE ${where}`);
//     })
//     .get(
//       "/inputcell/:table/:id/:key/:value",
//       async ({ params: { table, id, key, value } }) => {
//         return await DataNotFormComponent(table, id, key, value);
//       },
//     )
//     .post(
//       "/addviewcomponent/:configurationid/",
//       async ({ params: { configurationid }, body }) => {
//         const { viewname, query } = await body;
//         return await AddViewInput(configurationid, viewname, query);
//       },
//     )
//     .post("/deletedata/:table", async ({ params: { table }, body }) => {
//       return await DeleteRow(table, body);
//     })
//     // .post(
//     //   "/matchedviewproperties/:configurationid",
//     //   async ({ params: { configurationId }, body }) => {
//     //     const { viewname, query } = await body;
//     //     // console.log("mvp body: ", body);
//     //     // console.log("mvp viewname from app: ", viewname);
//     //     // console.log("mvpquery from app: ", query);
//     //     // console.log("mvp test");
//     //     return await MatchedViewProperties(configurationId, viewname, query);
//     //   },
//     // )
//     .listen(6174);
// }
//
// app();
