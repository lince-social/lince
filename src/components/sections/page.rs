use axum::response::Html;

pub async fn root() -> Html<&'static str> {
    Html("Hello world! Im an amendobobo")
}

// import * as elements from "typed-html";
//
//
// export default async function Page({ children }: elements.Children) {
//   return `<!DOCTYPE html>
//     <html lang="en">
//       <head>
//         <meta charset="UTF-8">
//         <meta name="viewport" content="width=device-width, initial-scale=1.0">
//         <meta http-equiv="X-UA-Compatible" content="ie=edge">
//         <title>Lince</title>
//         <script src="https://unpkg.com/htmx.org@2.0.4"></script>
//         <script src="https://cdn.tailwindcss.com"></script>
//         <style>
//           body {
//             background-color: #1e1e2e;
//             color: #ffffff;
//           }
//         </style
//       </head>
//         ${children}
//     </html>`;
// }
