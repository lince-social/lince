use axum::response::{Html, IntoResponse};

pub async fn body() -> impl IntoResponse {
    Html(format!(
        r##"
        <body id="body">
        <header
        id="header"
         hx-get="/section/header"
         hx-trigger="load"
         hx-swap="outerHTML"
         ></header>
         <main
         id="main"
          hx-get="/section/main"
          hx-trigger="load"
          hx-swap="outerHTML"
          ></main>
        </body>
        "##
    ))
}
// export async function FatherBody({ children }) {
//   const header = await (<Header />);
//   const main = await (<Main />);
//   return (
//     <div id="body" class="space-y-2">
//       <div>{header}</div>
//       <div>{main}</div>
//       <div>{children}</div>
//     </div>
//   );
// }
