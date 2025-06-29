// def return_column_information(column):
//     collection_df = read_rows("select * from collection")
//     max_quantity_config = collection_df[
//         collection_df["quantity"] == collection_df["quantity"].max()
//     ].iloc[0]
//     column_information_mode = max_quantity_config["column_information_mode"]
//
//     info = ""
//
//     if column_information_mode == "short" or column_information_mode == "verbose":
//         match column:
//             case "id":
//                 info += '"SERIAL PRIMARY KEY,".'
//             case "view":
//                 info += '"TEXT NOT NULL DEFAULT "SELECT * FROM record"".'
//             case "quantity":
//                 info += '"REAL NOT NULL DEFAULT 1".'
//             case "save_mode":
//                 info += '"VARCHAR(9) NOT NULL DEFAULT "Automatic" CHECK (save_mode in ("Automatic", "Manual")),".'
//             case "view_id":
//                 info += '"INTEGER NOT NULL DEFAULT 1".'
//             case "column_information_mode":
//                 info += '"VARCHAR(7) NOT NULL DEFAULT "verbose" CHECK (column_information_mode in ("verbose", "short", "silent")),".'
//             case "keymap":
//                 info += '"jsonb NOT NULL DEFAULT "{}"".'
//             case "truncation":
//                 info += '"jsonb NOT NULL DEFAULT "{"head": 150, "body": 150, "view": 100, "command": 150}"".'
//             case "table_query":
//                 info += '"jsonb NOT NULL DEFAULT "{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC", "command": "SELECT * FROM command ORDER BY id ASC"}"".'
//             case "language":
//                 info += '"VARCHAR(15) NOT NULL DEFAULT "en-US"".'
//             case "timezone":
//                 info += '"VARCHAR(3) NOT NULL DEFAULT "-3"".'
//             case "head":
//                 info += '"TEXT".'
//             case "body":
//                 info += '"TEXT".'
//             case "location":
//                 info += '"POINT".'
//             case "record_id":
//                 info += '"INTEGER NOT NULL".'
//             case "change_time":
//                 info += '"TIMESTAMP WITH TIME ZONE DEFAULT NOW()".'
//             case "old_quantity":
//                 info += '"REAL NOT NULL".'
//             case "new_quantity":
//                 info += '"REAL NOT NULL".'
//             case "expression":
//                 info += '"TEXT".'
//             case "day_week":
//                 info += '"INTEGER,".'
//             case "months":
//                 info += '"REAL DEFAULT 0 NOT NULL,".'
//             case "days":
//                 info += '"REAL DEFAULT 0 NOT NULL,".'
//             case "seconds":
//                 info += '"REAL DEFAULT 0 NOT NULL,".'
//             case "next_date":
//                 info += '"TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,".'
//             case "finish_date":
//                 info += '"DATE".'
//             case "sum_mode":
//                 info += '"INTEGER NOT NULL DEFAULT 0 CHECK (sum_mode in (-1,0,1)),".'
//             case "interval_relative":
//                 info += '"VARCHAR(10) NOT NULL DEFAULT "relative" CHECK (interval_relative IN ("fixed", "relative")),".'
//             case "interval_length":
//                 info += '"INTERVAL NOT NULL,".'
//             case "end_lag":
//                 info += '"INTERVAL".'
//             case "end_date":
//                 info += '"TIMESTAMP WITH TIME ZONE DEFAULT now()".'
//             case "command":
//                 info += '"TEXT NOT NULL".'
//             case "records_received":
//                 info += '"json,".'
//             case "records_contributed":
//                 info += '"json,".'
//             case "agreement":
//                 info += '"JSON,".'
//             case "agreement_time":
//                 info += '"TIMESTAMP WITH TIME ZONE,".'
//             case "transfer_confirmation":
//                 info += '"JSON,".'
//             case "transfer_time":
//                 info += '"TIMESTAMP WITH TIME ZONE".'
//
//     if column_information_mode == "verbose":
//         match column:
//             case "id":
//                 info += "Responsible for giving an unique idendifier to some row on a table."
//             case "view":
//                 info += "Responsible for setting the data shown."
//             case "quantity":
//                 info += "Responsible for controlling the availability or activeness of something."
//             case "save_mode":
//                 info += "Responsible for saving the database after operations in an automatic way, or when manually done."
//             case "view_id":
//                 info += (
//                     "Responsible for referencing the view that goes into collection."
//                 )
//             case "column_information_mode":
//                 info += "Responsible for selecting different quantities of information about columns when you fill them at row creation."
//             case "keymap":
//                 info += "Responsible for i dunno."
//             case "truncation":
//                 info += "Responsible for making content appear on the screen with line breaks after a certain amount of characters."
//             case "table_query":
//                 info += "Responsible for setting how tables will be shown when queried through [N]r."
//             case "language":
//                 info += "Responsible for setting the language."
//             case "timezone":
//                 info += "Responsible for setting timezone correctly for frequency table and date shown."
//             case "head":
//                 info += "Responsible for setting a head text information to the record."
//             case "body":
//                 info += "Responsible for setting a body text information to the record."
//             case "location":
//                 info += (
//                     "Responsible for setting a location something is supposed to be at."
//                 )
//             case "record_id":
//                 info += "Responsible for setting a reference to a record."
//             case "change_time":
//                 info += "Responsible for saving when a change of a record quantity happened."
//             case "old_quantity":
//                 info += "Responsible for saving an old quantity of a record."
//             case "new_quantity":
//                 info += "Responsible for saving a new quantity of a record."
//             case "expression":
//                 info += "Responsible for creating a Lince function for consequences if some conditions are met."
//             case "day_week":
//                 info += "Responsible for setting in what day of the week this frequency will activate. Monday is 1."
//             case "months":
//                 info += "Responsible for setting how many months will pass before this frequency activates."
//             case "days":
//                 info += "Responsible for setting how many days will pass before this frequency activates."
//             case "seconds":
//                 info += "Responsible for setting how many seconds will pass before this frequency activates."
//             case "next_date":
//                 info += "Responsible for showing when will the next ocurrence of a frequency will happen."
//             case "finish_date":
//                 info += "Responsible for setting a finish date so the frequency does not activate anymore."
//             case "sum_mode":
//                 info += "Responsible for setting the sum of negative changes, positive ones, or all (delta)."
//             case "interval_relative":
//                 info += "Responsible for setting a sum mode that has a fixed period, from day 1 to now or day 24, or a relative one, from today to 6 months from today, and if end_lag exists, then the sum will be from 6 months+ end_lag ago, till today + end_lag. Example: 6 months + 1 month lag untill 1 month ago (lag)."
//             case "interval_length":
//                 info += "Responsible for setting the amount of time the sum period will count on."
//             case "end_lag":
//                 info += 'Responsible for shifting the end date to a certain time backwards, if the interval_relative is "relative" and end_date is to the present moment, setting this will shift not only the end date but the starting date a certain amount, while still keeping the "relative" property.'
//             case "end_date":
//                 info += "Responsible for setting the end of the sum period."
//             case "command":
//                 info += "Responsible for executing shell commands."
//             case "records_received":
//                 info += "Responsible for saving information of records being received during the transfer."
//             case "records_contributed":
//                 info += "Responsible for saving information of records being contributed during the transfer."
//             case "agreement":
//                 info += "Responsible for saving information that transfer conditions for have been agreed upon."
//             case "agreement_time":
//                 info += "Responsible for saving informatino on the time of agreement of receivement and contribution."
//             case "transfer_confirmation":
//                 info += "Responsible for saving information that the transfer was successful."
//             case "transfer_time":
//                 info += "Responsible for saving information about the moment of the transfer."
//
//     return info
