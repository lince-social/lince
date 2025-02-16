// def return_sum_delta_record(id):
//     sum_row = read_rows(f"SELECT * FROM sum WHERE id={id} AND quantity != 0")
//
//     if sum_row.empty:
//         print(f"No such sum row with id {id}")
//         return 0
//     else:
//         sum_row = sum_row.iloc[0]
//
//     if sum_row["quantity"] < 0:
//         quantity = sum_row["quantity"] + 1
//         update_rows(
//             "sum",
//             set_clause=f"quantity = {quantity}",
//             where_clause=f"id = {sum_row['id']}",
//         )
//
//     if sum_row["interval_relative"] == True:
//         if sum_row["end_lag"] is not None:
//             end_lag = sum_row["end_lag"]
//             end_date = datetime.datetime.now() - timeshift(end_lag)
//         else:
//             end_date = datetime.now()
//     else:
//         end_date = sum_row["end_date"]
//
//     start_date = end_date - sum_row["interval_length"]
//
//     match sum_row["sum_mode"]:
//         case -1:
//             changes = read_rows(f"""SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
//                 WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row["record_id"]} AND new_quantity - old_quantity < 0 """)
//         case 0:
//             changes = read_rows(f"""SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
//                 WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row["record_id"]} """)
//         case 1:
//             changes = read_rows(f"""SELECT SUM(new_quantity - old_quantity) AS total_changes FROM history
//                 WHERE change_time BETWEEN '{start_date}' AND '{end_date}' AND record_id = {sum_row["record_id"]} AND new_quantity - old_quantity > 0 """)
//
//     return changes["total_changes"].iloc[0] if not changes.empty else 0
