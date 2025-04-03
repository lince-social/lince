use axum::response::Html;
use maud::Markup;

use crate::application::use_cases::section::main::main_use_case;

pub async fn main_component() -> Markup {
    let main_data = main_use_case();
}

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
