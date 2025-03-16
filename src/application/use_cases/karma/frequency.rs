// def check_update_frequency(id):
//     frequency_row = read_rows(
//         f"SELECT * FROM frequency WHERE id={id} and quantity != 0"
//     )
//     if frequency_row.empty:
//         return 0
//
//     frequency_row = frequency_row.iloc[0]
//
//     configuration_row = execute_sql_command(
//         "SELECT timezone FROM configuration ORDER BY quantity DESC LIMIT 1"
//     ).iloc[0]
//     configuration_timezone = configuration_row["timezone"]
//     tz_offset = timedelta(hours=int(configuration_timezone))
//     tzinfo = timezone(tz_offset)
//     time_now = datetime.now(tzinfo)
//
//     if (
//         frequency_row["finish_date"] is not None
//         and time_now.date() > frequency_row["finish_date"]
//     ):
//         return 0
//     if frequency_row["next_date"].astimezone(tzinfo) > time_now:
//         return 0
//
//     next_date = frequency_row["next_date"].astimezone(tzinfo)
//
//     catch_up_sum = frequency_row["catch_up_sum"]
//
//     occurence = 0
//
//     if (
//         frequency_row["months"] is not None
//         or frequency_row["days"] is not None
//         or frequency_row["seconds"] is not None
//     ):
//         # while next_date <= time_now:
//         next_date += relativedelta(months=int(frequency_row["months"])) + timedelta(
//             days=int(frequency_row["days"]), seconds=int(frequency_row["seconds"])
//         )
//         occurence += 1
//
//     if not pd.isna(frequency_row["day_week"]):
//         next_date += timedelta(days=1)
//         occurence += 1
//         while next_date.isoweekday() not in [
//             int(i) for i in str(int(frequency_row["day_week"]))
//         ]:
//             next_date += timedelta(days=1)
//             occurence += 1
//
//     update_rows(
//         "frequency",
//         set_clause=f"next_date = '{next_date}'",
//         where_clause=f"id = {frequency_row['id']}",
//     )
//
//     if frequency_row["quantity"] < 0:
//         quantity = frequency_row["quantity"] + 1
//         update_rows(
//             "frequency",
//             set_clause=f"quantity = {quantity}",
//             where_clause=f"id = {frequency_row['id']}",
//         )
//
//     if catch_up_sum > 0:
//         return occurence * catch_up_sum
//     elif catch_up_sum < 0:
//         if -catch_up_sum <= occurence:
//             return -catch_up_sum
//     return occurence
