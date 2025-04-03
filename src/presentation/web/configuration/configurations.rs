use maud::{Markup, html};

use crate::infrastructure::database::repositories::configuration::get_active;

pub async fn unhovered() -> Markup {
    let active = get_active().await;
    if active.is_err() {
        println!("Error: {}", active.unwrap_err());
        return html!({p {"Error when grabbing active config"}});
    }
    let active = active.unwrap();
    // if active.is_empty() {
    //     return Html(r#"<button style="background-color: lightgray; padding: 10px; border: none; border-radius: 5px;">No active configuration</button>"#.to_string());
    // }

    // Html(configuration_rows(active))
    html!({ pre {@if let Some(_user) = active {
       "Active config"
    } @else {"No active config"}} })
}

// pub async fn hovered() -> Html<String> {
//     let active: Vec<String> = get_active().await;
//     let inactive: Vec<String> = get_inactive().await;
//     Html(active)
// }
// import * as elements from "typed-html";
// import { sql } from "bun";
// import Views from "../views/Views";
//
// export async function ConfigurationRow({ configurationItem }) {
//   const views = await (<Views configurationId={configurationItem.id} />);
//
//   return (
//     <div class="flex space space-x-1 p-2">
//       <button
//         class={`p-1 rounded ${configurationItem.quantity === 1 ? "bg-red-800 hover:bg-red-900" : "hover:bg-blue-900 bg-blue-950"}`}
//         hx-post={`/configurationclick/${configurationItem.id}`}
//         hx-target="#main"
//         hx-trigger="click"
//       >
//         {configurationItem.configuration_name}
//       </button>
//       {views}
//     </div>
//   );
// }
//
// export default async function ConfigurationsUnhovered() {
//   const activeConfiguration =
//     await sql`SELECT id, configuration_name, quantity FROM configuration WHERE quantity = 1`;
//   const activeConfigurationRow = await (
//     <ConfigurationRow
//       key={activeConfiguration[0].id}
//       configurationItem={activeConfiguration[0]}
//     />
//   );
//
//   return (
//     <div
//       id="configuration"
//       hx-get="/configurationhovered"
//       hx-trigger="mouseenter"
//       hx-swap="outerHTML"
//       class="group flex bg-slate-800/80 w-full rounded items-center text-white"
//     >
//       <svg
//         xmlns="http://www.w3.org/2000/svg"
//         fill="none"
//         viewBox="0 0 24 24"
//         strokeWidth={1.5}
//         stroke="currentColor"
//         class="ml-2 size-6 group-hover:rotate-180 transition ease-in-out duration-300"
//       >
//         <path
//           strokeLinecap="round"
//           strokeLinejoin="round"
//           d="m19.5 8.25-7.5 7.5-7.5-7.5"
//         />
//       </svg>
//       <div class="w-full">{activeConfigurationRow}</div>
//     </div>
//   );
// }
//
// export async function ConfigurationsHovered() {
//   const activeConfiguration =
//     await sql`SELECT id, configuration_name, quantity FROM configuration WHERE quantity = 1`;
//   const activeConfigurationRow = await (
//     <ConfigurationRow
//       key={activeConfiguration[0].id}
//       configurationItem={activeConfiguration[0]}
//     />
//   );
//
//   const inactiveConfigurations =
//     await sql`SELECT id, configuration_name, quantity FROM configuration WHERE quantity <> 1`;
//   const inactiveConfigurationsRows = await Promise.all(
//     inactiveConfigurations.map(async (inactiveConfiguration) => {
//       return (
//         <ConfigurationRow
//           key={inactiveConfiguration.id}
//           configurationItem={inactiveConfiguration}
//         />
//       );
//     }),
//   );
//
//   return (
//     <div
//       id="configuration"
//       hx-get="/configurationunhovered"
//       hx-trigger="mouseleave"
//       hx-swap="outerHTML"
//       class="flex bg-slate-800/80 rounded-t w-full items-center text-white relative"
//     >
//       <svg
//         xmlns="http://www.w3.org/2000/svg"
//         fill="none"
//         viewBox="0 0 24 24"
//         strokeWidth={1.5}
//         stroke="currentColor"
//         class="ml-2 size-6 rotate-180"
//       >
//         <path
//           strokeLinecap="round"
//           strokeLinejoin="round"
//           d="m19.5 8.25-7.5 7.5-7.5-7.5"
//         />
//       </svg>
//       <div class="flex flex-col w-full">
//         <div>{activeConfigurationRow}</div>
//         <div class="absolute mt-12 inset-x-0 bg-slate-800/80 rounded-b">
//           {inactiveConfigurationsRows}
//         </div>
//       </div>
//     </div>
//   );
// }
