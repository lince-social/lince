pub fn _karma() {
    println!("hello from karma")
}

// import { saveDatabase } from "../../../db/startup";
//
// export default async function Karma() {
//   try {
//     console.log("helorowd")
//     console.log(await sql`SELECT * FROM record`)
//     // olhar pra karma:
//     // ver a coluna condition_id, puxar
//     // const data = await sql`SELECT FROM karma`;
//     // karma_consequence | operator | karma_condition
//     //
//     // CONDITION: SELECT condition FROM karma_condition:
//     // if 'f' take the numbers after and thats the id, run UpdateFrequency(id) and replace in the string the fNUMBERS with the result: number
//     // if 'c' take the numbers after and thats the id, run RunShellCommand(id) and replace in the string the cNUMBERS with the result: number
//     // if 'rq' take the numbers after and thats the id, run GetRecordQuantity(id) and replace in the string the rqNUMBERS with the result: number
//     // if 's' take the numbers after and thats the id, run CalculateSum(id) and replace in the string the sNUMBERS with the result: number
//     // then, eval the condition and grab the result: number
//     //
//     // OPERATOR: SELECT operator from
//     // def if true: apply operator to all consequences in question:
//     // record quantity
//     // command
//     //
//     // ---------- operator:
//     // if null -> =
//     // if * in it, if the condition is zero, apply zero to the consequences.
//     //
//     // ----------- consequences:
//     // if rqNUMBERS -> put the result of operator(condition) into record quantity
//     // if cNUMBERS -> activate the script the module of times of operator(condition)
//   } catch (error) {
//     console.log(`Error when running Karma() at: ${new Date()}: ${error}`)
//   }
// }
//
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
