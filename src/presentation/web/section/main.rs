use maud::{Markup, html};

use crate::application::use_cases::section::main::main_use_case;

pub async fn main_component() -> Markup {
    let main_data = main_use_case().await;

    html! {
        div id="main" {
        @match main_data {
            Ok(records) => {
                @for record in &records {
                    div {
                        p { (record.head) }

                        button
                        hx-delete=(format!("/record/{}", record.id))
                            hx-swap="outerHTML"
                            hx-target="#main"
                        hx-trigger="click"
                        { "x" }
                    }
                }
            },
            Err(_) => p { "Error when fetching records" }
        }
        }
    }
}

// pub async fn main_component() -> Markup {
//     let main_data = main_use_case().await;

//     html! {
//         @match main_data {
//             Ok(records) => {
//                     @for record in &records {
//                         div {
//                         p { (record.head) }

//                         button
//                         hx-delete={format!("/record/{}", record.id)}
//                         hx-swap="outerHTML"
//                         hx-target="#main"
//                         hx-trigger="click"
//                         { "x"}

//                         }
//                     }
//             },
//             Err(_) => p { "Error when fetching records" }
//         }
//     }
// }
// pub async fn main_component() -> Markup {
//     let main_data = main_use_case().await;
//     html!({
//         @match main_data {
//             Ok(records) => {
//                ol {
//                   @for record in &records {
//                      li { (record) }
//                   }
//                }
//             },
//             Err(error) => p {"Error when fetching records"}
//         }
//     })
// match main_data {
//     Err(e) => {
//         println!("Error when fetching all records: {e}");
//         html!({ p { "Error when getting records: " (e) }})
//     }
//     Ok(records) => {
//         html!({ ol {
//         @for record in &records {
//            li { (record )}
//         } } })
//     }
// }

// import * as elements from "typed-html";
// import ConfigurationsUnhovered from "../configurations/Configurations";
// import Tables from "../tables/Tables";
//
// export default async function UnhoveredMain() {
//   const configurations = await (<ConfigurationsUnhovered />);
//   const tables = await (<Tables />);
//
//   return (
//     <main id="main">
//       <div>{configurations}</div>
//       <div>{tables}</div>
//     </main>
//   );
// }
//
// export async function HoveredMain() {
//   const configurations = await (<ConfigurationsHovered />);
//   const tables = await (<Tables />);
//
//   return (
//     <main id="main" class="">
//       <div>{configurations}</div>
//       <div>{tables}</div>
//     </main>
//   );
// }
