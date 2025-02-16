import { sql } from "bun";

export default async function Karma() {
  const data = await sql`SELECT FROM karma`;
}

// def karma():
//     karma_df = read_rows("SELECT * FROM karma")
//
//     for index, karma_row in karma_df.iterrows():
//         expression = karma_row["expression"]
//         expression = [item.strip() for item in expression.split("=", 1)]
//
//         try:
//             expression_one_record_quantities = re.findall("rq[0-9]+", expression[1])
//             for id in expression_one_record_quantities:
//                 quantity = execute_sql_command(
//                     f"SELECT quantity FROM record WHERE id = {id[2:]}"
//                 )["quantity"].iloc[0]
//                 expression[1] = expression[1].replace(id, str(quantity))
//
//             expression_one_frequencies = re.findall("f[0-9]+", expression[1])
//             for frequency in expression_one_frequencies:
//                 frequency_return = check_update_frequency(id=frequency[1:])
//                 expression[1] = expression[1].replace(frequency, str(frequency_return))
//
//             expression_one_commands = re.findall("co?[0-9]+", expression[1])
//             for command in expression_one_commands:
//                 if "o" in command:
//                     command_return = execute_shell_command(id=command[2:], output=True)
//                 else:
//                     command_return = execute_shell_command(id=command[1:], output=False)
//                 expression[1] = expression[1].replace(command, str(command_return))
//
//             expression_one_sums = re.findall("s[0-9]+", expression[1])
//             for sum in expression_one_sums:
//                 sum_return = return_sum_delta_record(id=sum[1:])
//                 expression[1] = expression[1].replace(sum, str(sum_return))
//
//             result = eval(expression[1])
//             if result == None:
//                 continue
//         except:
//             continue
//
//         if result != 0 or (result == 0 and expression[0].endswith("*")):
//             expression[0] = expression[0].replace("*", "")
//             expression[0] = expression[0].strip()
//             left_expression = expression[0].split(",")
//
//             for consequence in left_expression:
//                 consequence = consequence.strip()
//
//                 if "rq" in consequence:
//                     table = "record"
//                     set_column = "quantity"
//                     set_value = result
//                     where_column = "id"
//                     where_value = f"{consequence[2:]}"
//                     execute_sql_command(
//                         f"UPDATE {table} SET {set_column} = {set_value} WHERE {where_column} = {where_value}"
//                     )
//                     dump_db()
//
//                 if "co" in consequence:
//                     execute_shell_command(consequence[2:], output=True)
//                     continue
//
//                 if "c" in consequence:
//                     execute_shell_command(consequence[1:], output=False)
//                     continue
//
//     return True
