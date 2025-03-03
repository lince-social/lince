pub fn get_configs() {
    "asoiasjd"
}
// import { sql } from "bun";
// import * as elements from "typed-html";
// import { ConfigurationsHovered } from "./Configurations";
// import Tables from "../tables/Tables";
//
// export async function getActiveConfiguration() {
//   return await sql`SELECT id, configurationName, quantity FROM configuration WHERE quantity = 1`;
// }
//
// export async function getInactiveConfigurations() {
//   return await sql`SELECT id, configurationName, quantity FROM configuration WHERE quantity <> 1`;
// }
//
// export async function ConfigurationChange(id: string) {
//   try {
//     await sql`
//       UPDATE configuration
//       SET quantity = CASE WHEN id = ${id} THEN 1 ELSE 0 END;
//     `;
//
//     return (
//       <main id="main">
//         <div>{await ConfigurationsHovered()}</div>
//         <div>{await Tables()}</div>
//       </main>
//     );
//   } catch (error) {
//     console.log("Error updating configuration:", error);
//   }
// }
