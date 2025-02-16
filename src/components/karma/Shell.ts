import { $ } from "bun"

export async function ExecuteShellCommand(command: string) {
  await $`command`
}

// def execute_shell_command(id, output):
//     command_row = read_rows(f"SELECT * FROM command WHERE id={id}")
//     if command_row.empty:
//         return False
//
//     command_row = command_row.iloc[0]
//     quantity = command_row["quantity"]
//
//     if quantity == 0:
//         return 0
//     if quantity < 0:
//         update_rows(
//             "command",
//             set_clause=f"quantity = {quantity + 1}",
//             where_clause=f"id = {command_row['id']}",
//         )
//
//     if output:
//         subprocess.run(
//             command_row["command"], text=True, shell=True, capture_output=True
//         ).stdout.strip()
//         with open("/tmp/lince") as file:
//             contents = file.read()
//         os.remove("/tmp/lince")
//         return contents
//
//     os.system(command_row["command"])
//     return False
